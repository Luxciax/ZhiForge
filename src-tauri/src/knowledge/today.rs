use serde::Serialize;
use tauri::State;

use crate::database::DatabaseState;

use super::{now_ms, review, task_center};

const SAMPLE_LIMIT: i64 = 4;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayInboxItem {
    pub inbox_id: String,
    pub title: String,
    pub status: String,
    pub processing_stage: String,
    pub draft_count: i64,
    pub error: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayResearchGapItem {
    pub gap_id: String,
    pub plan_id: String,
    pub topic: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub search_query: String,
    pub source_count: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayKnowledgeSignal {
    pub knowledge_unit_id: String,
    pub title: String,
    pub signal: String,
    pub mastery_score: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayOverview {
    pub generated_at: i64,
    pub inbox_captured_count: i64,
    pub inbox_ready_count: i64,
    pub inbox_failed_count: i64,
    pub inbox_processing_count: i64,
    pub inbox_items: Vec<TodayInboxItem>,
    pub ai_active_count: i64,
    pub ai_failed_count: i64,
    pub initial_learning_count: i64,
    pub initial_learning_items: Vec<review::ReviewQueueItem>,
    pub due_review_count: i64,
    pub active_review_remaining_count: i64,
    pub review_items: Vec<review::ReviewQueueItem>,
    pub active_research_plan_count: i64,
    pub open_research_gap_count: i64,
    pub collecting_research_gap_count: i64,
    pub research_gaps: Vec<TodayResearchGapItem>,
    pub weak_count: i64,
    pub conflicted_count: i64,
    pub needs_expansion_count: i64,
    pub stale_count: i64,
    pub maintenance_count: i64,
    pub knowledge_signals: Vec<TodayKnowledgeSignal>,
    pub librarian_pending_count: i64,
    pub curation_pending_count: i64,
}

#[tauri::command]
pub fn knowledge_today(state: State<'_, DatabaseState>) -> Result<TodayOverview, String> {
    state.with_connection(|connection| load_today_overview(connection))
}

fn load_today_overview(connection: &rusqlite::Connection) -> Result<TodayOverview, String> {
    let now = now_ms();
    // Task counting also reconciles stale pending/running AI jobs. Do this first so
    // every Inbox count/item below observes the same recovered database state.
    let (ai_active_count, ai_failed_count) = task_center::current_task_counts(connection)?;
    let inbox_captured_count = count_inbox(connection, "captured")?;
    let inbox_ready_count = count_inbox(connection, "ready")?;
    let inbox_failed_count = count_inbox(connection, "failed")?;
    let inbox_processing_count = count_inbox(connection, "processing")?;
    let study_queue = review::load_review_queue(connection, now, 100)?;
    let initial_learning_count = count_initial_learning(connection)?;
    let due_review_count = count_scheduled_review(connection, now)?;
    let initial_learning_items = study_queue
        .iter()
        .filter(|item| item.review_count == 0)
        .take(SAMPLE_LIMIT as usize)
        .cloned()
        .collect::<Vec<_>>();
    let review_items = study_queue
        .iter()
        .filter(|item| item.review_count > 0)
        .take(SAMPLE_LIMIT as usize)
        .cloned()
        .collect::<Vec<_>>();
    let active_review_remaining_count = connection
        .query_row(
            "SELECT COUNT(*)
             FROM review_session_items i
             JOIN review_sessions s ON s.id = i.session_id
             WHERE s.status = 'active' AND i.status = 'pending'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count active review items: {error}"))?;

    let active_research_plan_count = connection
        .query_row(
            "SELECT COUNT(*) FROM research_plans WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count active research plans: {error}"))?;
    let open_research_gap_count = count_research_gaps(connection, "open")?;
    let collecting_research_gap_count = count_research_gaps(connection, "collecting")?;

    let weak_count = count_knowledge_signal(connection, "weak")?;
    let conflicted_count = count_quality_signal(connection, "conflicted")?;
    let needs_expansion_count = count_quality_signal(connection, "needs_expansion")?;
    let stale_count = count_quality_signal(connection, "stale")?;
    let maintenance_count = count_maintenance(connection)?;

    let librarian_pending_count = connection
        .query_row(
            "SELECT COUNT(*) FROM librarian_proposals WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count librarian proposals: {error}"))?;
    let curation_pending_count = connection
        .query_row(
            "SELECT COUNT(*) FROM curation_candidates WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count curation candidates: {error}"))?;

    Ok(TodayOverview {
        generated_at: now,
        inbox_captured_count,
        inbox_ready_count,
        inbox_failed_count,
        inbox_processing_count,
        inbox_items: load_inbox_items(connection)?,
        ai_active_count,
        ai_failed_count,
        initial_learning_count,
        initial_learning_items,
        due_review_count,
        active_review_remaining_count,
        review_items,
        active_research_plan_count,
        open_research_gap_count,
        collecting_research_gap_count,
        research_gaps: load_research_gaps(connection)?,
        weak_count,
        conflicted_count,
        needs_expansion_count,
        stale_count,
        maintenance_count,
        knowledge_signals: load_knowledge_signals(connection)?,
        librarian_pending_count,
        curation_pending_count,
    })
}

fn count_initial_learning(connection: &rusqlite::Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*)
             FROM review_states r
             JOIN knowledge_units k ON k.id = r.knowledge_unit_id
             WHERE r.review_count = 0
               AND k.deleted_at IS NULL AND k.archived_at IS NULL
               AND k.status IN ('weak', 'learning', 'reviewing', 'mastered')
               AND EXISTS(SELECT 1 FROM questions q WHERE q.knowledge_unit_id = k.id)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count first-learning items: {error}"))
}

fn count_scheduled_review(connection: &rusqlite::Connection, now: i64) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*)
             FROM review_states r
             JOIN knowledge_units k ON k.id = r.knowledge_unit_id
             WHERE r.review_count > 0
               AND r.next_review_at IS NOT NULL AND r.next_review_at <= ?1
               AND k.deleted_at IS NULL AND k.archived_at IS NULL
               AND k.status IN ('weak', 'learning', 'reviewing', 'mastered')
               AND EXISTS(SELECT 1 FROM questions q WHERE q.knowledge_unit_id = k.id)",
            [now],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count scheduled review items: {error}"))
}

fn count_inbox(connection: &rusqlite::Connection, status: &str) -> Result<i64, String> {
    connection
        .query_row("SELECT COUNT(*) FROM inbox_items WHERE status = ?1", [status], |row| row.get(0))
        .map_err(|error| format!("failed to count {status} inbox items: {error}"))
}

fn load_inbox_items(connection: &rusqlite::Connection) -> Result<Vec<TodayInboxItem>, String> {
    let mut statement = connection
        .prepare(
            "SELECT i.id,
                    COALESCE(NULLIF(trim(s.title), ''), NULLIF(trim(s.author), ''), substr(s.selected_text, 1, 120)),
                    i.status, i.processing_stage,
                    (SELECT COUNT(*) FROM knowledge_drafts d WHERE d.inbox_id = i.id),
                    i.error, i.updated_at
             FROM inbox_items i
             JOIN sources s ON s.id = i.source_id
             WHERE i.status IN ('captured', 'ready', 'failed', 'processing')
             ORDER BY CASE i.status
                        WHEN 'ready' THEN 0
                        WHEN 'failed' THEN 1
                        WHEN 'captured' THEN 2
                        ELSE 3
                      END,
                      i.updated_at DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare Today inbox items: {error}"))?;
    let rows = statement
        .query_map([SAMPLE_LIMIT], |row| {
            Ok(TodayInboxItem {
                inbox_id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                processing_stage: row.get(3)?,
                draft_count: row.get(4)?,
                error: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| format!("failed to query Today inbox items: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read Today inbox items: {error}"))?;
    Ok(rows)
}

fn count_research_gaps(connection: &rusqlite::Connection, status: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM research_gaps g
             JOIN research_plans p ON p.id = g.plan_id
             WHERE p.status = 'active' AND g.status = ?1",
            [status],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count {status} research gaps: {error}"))
}

fn load_research_gaps(connection: &rusqlite::Connection) -> Result<Vec<TodayResearchGapItem>, String> {
    let mut statement = connection
        .prepare(
            "SELECT g.id, g.plan_id, p.topic, g.title, g.priority, g.status, g.search_query,
                    (SELECT COUNT(*) FROM research_gap_sources rgs WHERE rgs.gap_id = g.id),
                    g.updated_at
             FROM research_gaps g
             JOIN research_plans p ON p.id = g.plan_id
             WHERE p.status = 'active' AND g.status IN ('open', 'collecting')
             ORDER BY CASE g.priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,
                      CASE g.status WHEN 'collecting' THEN 0 ELSE 1 END,
                      g.updated_at DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare Today research gaps: {error}"))?;
    let rows = statement
        .query_map([SAMPLE_LIMIT], |row| {
            Ok(TodayResearchGapItem {
                gap_id: row.get(0)?,
                plan_id: row.get(1)?,
                topic: row.get(2)?,
                title: row.get(3)?,
                priority: row.get(4)?,
                status: row.get(5)?,
                search_query: row.get(6)?,
                source_count: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query Today research gaps: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read Today research gaps: {error}"))?;
    Ok(rows)
}

fn count_knowledge_signal(connection: &rusqlite::Connection, status: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_units
             WHERE deleted_at IS NULL AND archived_at IS NULL AND status = ?1",
            [status],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count {status} knowledge: {error}"))
}

fn count_quality_signal(connection: &rusqlite::Connection, status: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*)
             FROM knowledge_quality_states q
             JOIN knowledge_units k ON k.id = q.knowledge_unit_id
             WHERE q.status = ?1
               AND k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured'",
            [status],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count {status} knowledge quality: {error}"))
}

fn count_maintenance(connection: &rusqlite::Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(DISTINCT k.id)
             FROM knowledge_units k
             LEFT JOIN knowledge_quality_states q ON q.knowledge_unit_id = k.id
             WHERE k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured'
               AND (k.status = 'weak' OR q.status IN ('conflicted', 'needs_expansion', 'stale'))",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count Today maintenance knowledge: {error}"))
}

fn load_knowledge_signals(connection: &rusqlite::Connection) -> Result<Vec<TodayKnowledgeSignal>, String> {
    let mut statement = connection
        .prepare(
            "SELECT k.id,
                    COALESCE(NULLIF(trim(k.core_claim), ''), NULLIF(trim(s.title), ''), substr(s.selected_text, 1, 120)),
                    CASE
                        WHEN q.status = 'conflicted' THEN 'conflicted'
                        WHEN k.status = 'weak' THEN 'weak'
                        WHEN q.status = 'needs_expansion' THEN 'needs_expansion'
                        WHEN q.status = 'stale' THEN 'stale'
                        ELSE 'other'
                    END,
                    COALESCE(r.mastery_score, 0),
                    MAX(k.updated_at, COALESCE(q.updated_at, 0))
             FROM knowledge_units k
             JOIN sources s ON s.id = k.source_id
             LEFT JOIN knowledge_quality_states q ON q.knowledge_unit_id = k.id
             LEFT JOIN review_states r ON r.knowledge_unit_id = k.id
             WHERE k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured'
               AND (k.status = 'weak' OR q.status IN ('conflicted', 'needs_expansion', 'stale'))
             ORDER BY CASE
                        WHEN q.status = 'conflicted' THEN 0
                        WHEN k.status = 'weak' THEN 1
                        WHEN q.status = 'needs_expansion' THEN 2
                        ELSE 3
                      END,
                      k.updated_at DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare Today knowledge signals: {error}"))?;
    let rows = statement
        .query_map([SAMPLE_LIMIT], |row| {
            Ok(TodayKnowledgeSignal {
                knowledge_unit_id: row.get(0)?,
                title: row.get(1)?,
                signal: row.get(2)?,
                mastery_score: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|error| format!("failed to query Today knowledge signals: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read Today knowledge signals: {error}"))?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::apply_migrations;
    use rusqlite::Connection;

    #[test]
    fn today_snapshot_is_consistent_when_stale_internalization_is_recovered() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at)
                    VALUES ('s-stale', 'web', 'Stale source', 'stale source text', 'today-stale-source', 1);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                    VALUES ('k-stale', 's-stale', '', 'captured', 2, 2);
                 INSERT INTO review_states(knowledge_unit_id, mastery_score)
                    VALUES ('k-stale', 0);
                 INSERT INTO inbox_items(
                    id, source_id, anchor_knowledge_id, status, processing_stage, created_at, updated_at
                 ) VALUES ('i-stale', 's-stale', 'k-stale', 'processing', 'extracting', 3, 3);
                 INSERT INTO ai_jobs(
                    id, knowledge_unit_id, job_type, status, error, created_at, updated_at
                 ) VALUES ('j-stale', 'k-stale', 'internalize', 'running', NULL, 4, 4);"
            )
            .unwrap();

        let overview = load_today_overview(&connection).unwrap();
        assert_eq!(overview.ai_active_count, 0);
        assert_eq!(overview.ai_failed_count, 1);
        assert_eq!(overview.inbox_processing_count, 0);
        assert_eq!(overview.inbox_failed_count, 1);
        assert_eq!(overview.inbox_items.len(), 1);
        assert_eq!(overview.inbox_items[0].status, "failed");
        assert!(overview.inbox_items[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("timed out"));
    }

    #[test]
    fn today_prioritizes_actionable_work_and_excludes_captured_placeholders() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at) VALUES
                    ('s-inbox', 'zhihu', 'Inbox source', 'draft source text', 'today-inbox-hash', 10),
                    ('s-captured', 'web', 'Captured source', 'waiting extraction', 'today-captured-hash', 11),
                    ('s-review', 'web', 'Review source', 'review evidence', 'today-review-hash', 12),
                    ('s-weak', 'web', 'Weak source', 'weak evidence', 'today-weak-hash', 13);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at) VALUES
                    ('k-inbox', 's-inbox', '', 'captured', 20, 20),
                    ('k-captured', 's-captured', '', 'captured', 21, 21),
                    ('k-review', 's-review', 'Review claim', 'learning', 22, 22),
                    ('k-weak', 's-weak', 'Weak claim', 'weak', 23, 23);
                 INSERT INTO review_states(knowledge_unit_id, mastery_score, next_review_at) VALUES
                    ('k-inbox', 0, NULL),
                    ('k-captured', 0, NULL),
                    ('k-review', 45, 1),
                    ('k-weak', 20, 9999999999999);
                 INSERT INTO questions(id, knowledge_unit_id, question_type, question, created_at) VALUES
                    ('q-review', 'k-review', 'explain', 'Explain review claim', 30);
                 INSERT INTO inbox_items(id, source_id, anchor_knowledge_id, status, processing_stage, created_at, updated_at) VALUES
                    ('inbox:k-inbox', 's-inbox', 'k-inbox', 'ready', 'awaiting_review', 40, 40),
                    ('inbox:k-captured', 's-captured', 'k-captured', 'captured', 'captured', 41, 41);
                 INSERT INTO knowledge_drafts(id, inbox_id, source_id, position, core_claim, evidence_json, created_at, updated_at)
                    VALUES ('d1', 'inbox:k-inbox', 's-inbox', 0, 'Draft claim', '[\"draft source text\"]', 41, 41);
                 INSERT INTO research_plans(id, topic, summary, coverage_score, status, created_at, updated_at)
                    VALUES ('rp1', 'Agent memory', 'summary', 40, 'active', 50, 50);
                 INSERT INTO research_gaps(id, plan_id, position, title, rationale, priority, search_query, status, created_at, updated_at)
                    VALUES ('rg1', 'rp1', 0, 'Failure modes', 'missing', 'high', 'Agent memory failure modes', 'open', 51, 51);"
            )
            .unwrap();
        connection
            .execute(
                "UPDATE knowledge_quality_states SET status = 'needs_expansion' WHERE knowledge_unit_id = 'k-weak'",
                [],
            )
            .unwrap();

        let overview = load_today_overview(&connection).unwrap();
        assert_eq!(overview.inbox_captured_count, 1);
        assert_eq!(overview.inbox_ready_count, 1);
        assert_eq!(overview.inbox_items.len(), 2);
        assert_eq!(overview.inbox_items[0].draft_count, 1);
        assert!(overview.inbox_items.iter().any(|item| item.status == "captured"));
        assert_eq!(overview.initial_learning_count, 1);
        assert_eq!(overview.initial_learning_items.len(), 1);
        assert_eq!(overview.initial_learning_items[0].knowledge_unit_id, "k-review");
        assert_eq!(overview.due_review_count, 0);
        assert!(overview.review_items.is_empty());
        assert_eq!(overview.active_research_plan_count, 1);
        assert_eq!(overview.open_research_gap_count, 1);
        assert_eq!(overview.research_gaps[0].gap_id, "rg1");
        assert_eq!(overview.weak_count, 1);
        assert_eq!(overview.needs_expansion_count, 1);
        assert_eq!(overview.maintenance_count, 1);
        assert!(overview.knowledge_signals.iter().all(|item| item.knowledge_unit_id != "k-inbox"));
    }
}
