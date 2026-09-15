use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::State;

use crate::database::DatabaseState;

use super::{internalization, now_ms};

const STALE_TASK_MS: i64 = 10 * 60 * 1000;
const STALE_TASK_ERROR: &str = "AI processing timed out before completion; retry to continue.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskRecord {
    pub id: String,
    pub task_type: String,
    pub status: String,
    pub title: String,
    pub knowledge_unit_id: Option<String>,
    pub inbox_id: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub retryable: bool,
}

#[tauri::command]
pub fn knowledge_task_list(
    state: State<'_, DatabaseState>,
    limit: Option<u32>,
) -> Result<Vec<AiTaskRecord>, String> {
    let limit = limit.unwrap_or(40).clamp(1, 100) as usize;
    state.with_connection(|connection| load_task_list(connection, limit))
}

pub(crate) fn current_task_counts(connection: &rusqlite::Connection) -> Result<(i64, i64), String> {
    let tasks = load_task_list(connection, 1_000)?;
    let active = tasks
        .iter()
        .filter(|task| matches!(task.status.as_str(), "pending" | "running"))
        .count() as i64;
    let failed = tasks.iter().filter(|task| task.status == "failed").count() as i64;
    Ok((active, failed))
}

fn load_task_list(
    connection: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<AiTaskRecord>, String> {
    recover_stale_knowledge_jobs(connection)?;
    let mut tasks = load_latest_knowledge_jobs(connection, limit)?;
    if let Some(run) = load_latest_librarian_run(connection)? {
        tasks.push(run);
    }
    if let Some(run) = load_latest_curation_run(connection)? {
        tasks.push(run);
    }
    tasks.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    tasks.truncate(limit);
    Ok(tasks)
}

fn recover_stale_knowledge_jobs(connection: &rusqlite::Connection) -> Result<(), String> {
    let now = now_ms();
    let cutoff = now.saturating_sub(STALE_TASK_MS);
    let stale_jobs = {
        let mut statement = connection
            .prepare(
                "SELECT id FROM ai_jobs
                 WHERE status IN ('pending', 'running') AND updated_at < ?1
                 ORDER BY updated_at ASC, rowid ASC",
            )
            .map_err(|error| format!("failed to prepare stale AI job recovery: {error}"))?;
        let rows = statement
            .query_map([cutoff], |row| row.get::<_, String>(0))
            .map_err(|error| format!("failed to query stale AI jobs: {error}"))?;
        let jobs = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read stale AI jobs: {error}"))?;
        jobs
    };
    for job_id in stale_jobs {
        let changed = connection
            .execute(
                "UPDATE ai_jobs
                 SET status = 'failed', error = ?2, updated_at = ?3
                 WHERE id = ?1 AND status IN ('pending', 'running')",
                rusqlite::params![&job_id, STALE_TASK_ERROR, now],
            )
            .map_err(|error| format!("failed to recover stale AI job: {error}"))?;
        if changed == 1 {
            internalization::sync_inbox_failure_for_job(connection, &job_id, STALE_TASK_ERROR)?;
        }
    }
    Ok(())
}

fn load_latest_knowledge_jobs(
    connection: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<AiTaskRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT j.id, j.job_type, j.status,
                    COALESCE(NULLIF(trim(k.core_claim), ''), NULLIF(trim(s.title), ''),
                             NULLIF(trim(s.author), ''), substr(s.selected_text, 1, 120)),
                    j.knowledge_unit_id, i.id, j.error, j.created_at, j.updated_at
             FROM ai_jobs j
             JOIN knowledge_units k ON k.id = j.knowledge_unit_id
             JOIN sources s ON s.id = k.source_id
             LEFT JOIN inbox_items i ON i.anchor_knowledge_id = j.knowledge_unit_id
             WHERE NOT EXISTS(
                 SELECT 1 FROM ai_jobs newer
                 WHERE newer.knowledge_unit_id = j.knowledge_unit_id
                   AND newer.job_type = j.job_type
                   AND (
                       newer.created_at > j.created_at OR
                       (newer.created_at = j.created_at AND newer.rowid > j.rowid)
                   )
             )
               AND (j.job_type <> 'internalize' OR i.id IS NULL OR i.status <> 'ignored')
             ORDER BY j.updated_at DESC, j.created_at DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare AI task list: {error}"))?;
    let rows = statement
        .query_map([limit as i64], |row| {
            let status: String = row.get(2)?;
            Ok(AiTaskRecord {
                id: row.get(0)?,
                task_type: row.get(1)?,
                retryable: status == "failed",
                status,
                title: row.get(3)?,
                knowledge_unit_id: Some(row.get(4)?),
                inbox_id: row.get(5)?,
                error: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query AI task list: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read AI task list: {error}"))?;
    Ok(rows)
}

fn load_latest_librarian_run(
    connection: &rusqlite::Connection,
) -> Result<Option<AiTaskRecord>, String> {
    connection
        .query_row(
            "SELECT id, status, error, created_at, updated_at
             FROM librarian_runs ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [],
            |row| {
                let raw_status: String = row.get(1)?;
                let status = normalize_run_status(&raw_status).to_string();
                Ok(AiTaskRecord {
                    id: row.get(0)?,
                    task_type: "librarian".into(),
                    retryable: status == "failed",
                    status,
                    title: "知识整理建议".into(),
                    knowledge_unit_id: None,
                    inbox_id: None,
                    error: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to load latest librarian task: {error}"))
}

fn load_latest_curation_run(
    connection: &rusqlite::Connection,
) -> Result<Option<AiTaskRecord>, String> {
    connection
        .query_row(
            "SELECT id, status, error, created_at, updated_at
             FROM curation_runs ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [],
            |row| {
                let raw_status: String = row.get(1)?;
                let status = normalize_run_status(&raw_status).to_string();
                Ok(AiTaskRecord {
                    id: row.get(0)?,
                    task_type: "curation".into(),
                    retryable: status == "failed",
                    status,
                    title: "重复与冲突检查".into(),
                    knowledge_unit_id: None,
                    inbox_id: None,
                    error: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to load latest curation task: {error}"))
}

fn normalize_run_status(status: &str) -> &str {
    match status {
        "ready" => "completed",
        "running" => "running",
        "failed" => "failed",
        _ => status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::apply_migrations;
    use rusqlite::Connection;

    #[test]
    fn stale_ai_jobs_become_retryable_and_only_internalization_regresses_processing_inbox() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        let stale_at = now_ms().saturating_sub(STALE_TASK_MS + 1_000);
        let fresh_at = now_ms();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at) VALUES
                    ('s1', 'web', 'Extract source', 'source one', 'stale-source-1', 1),
                    ('s2', 'web', 'Question source', 'source two', 'stale-source-2', 1),
                    ('s3', 'web', 'Fresh source', 'source three', 'stale-source-3', 1);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at) VALUES
                    ('k1', 's1', '', 'captured', 2, 2),
                    ('k2', 's2', 'Question claim', 'learning', 2, 2),
                    ('k3', 's3', '', 'captured', 2, 2);
                 INSERT INTO inbox_items(id, source_id, anchor_knowledge_id, status, processing_stage, created_at, updated_at) VALUES
                    ('i1', 's1', 'k1', 'processing', 'extracting', 3, 3),
                    ('i2', 's2', 'k2', 'accepted', 'accepted', 3, 3),
                    ('i3', 's3', 'k3', 'processing', 'extracting', 3, 3);"
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_jobs(id, knowledge_unit_id, job_type, status, error, created_at, updated_at)
                 VALUES ('extract-stale', 'k1', 'internalize', 'running', NULL, ?1, ?1)",
                [stale_at],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_jobs(id, knowledge_unit_id, job_type, status, error, created_at, updated_at)
                 VALUES ('questions-stale', 'k2', 'question_generation', 'running', NULL, ?1, ?1)",
                [stale_at],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_jobs(id, knowledge_unit_id, job_type, status, error, created_at, updated_at)
                 VALUES ('extract-fresh', 'k3', 'internalize', 'running', NULL, ?1, ?1)",
                [fresh_at],
            )
            .unwrap();

        let tasks = load_task_list(&connection, 20).unwrap();
        let stale_extract = tasks.iter().find(|task| task.id == "extract-stale").unwrap();
        let stale_questions = tasks.iter().find(|task| task.id == "questions-stale").unwrap();
        let fresh_extract = tasks.iter().find(|task| task.id == "extract-fresh").unwrap();
        assert_eq!(stale_extract.status, "failed");
        assert!(stale_extract.retryable);
        assert_eq!(stale_questions.status, "failed");
        assert!(stale_questions.retryable);
        assert_eq!(fresh_extract.status, "running");
        assert!(!fresh_extract.retryable);

        let inboxes = connection
            .prepare("SELECT id, status FROM inbox_items ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(inboxes, vec![
            ("i1".to_string(), "failed".to_string()),
            ("i2".to_string(), "accepted".to_string()),
            ("i3".to_string(), "processing".to_string()),
        ]);
    }

    #[test]
    fn task_center_hides_superseded_failures_and_keeps_latest_task_types() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at)
                    VALUES ('s1', 'web', 'Source title', 'source text', 'task-source-hash', 1);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                    VALUES ('k1', 's1', 'Task claim', 'learning', 2, 2);
                 INSERT INTO inbox_items(
                    id, source_id, anchor_knowledge_id, status, processing_stage,
                    progress_current, progress_total, created_at, updated_at
                 ) VALUES ('i1', 's1', 'k1', 'ready', 'awaiting_review', 1, 1, 3, 3);
                 INSERT INTO ai_jobs(id, knowledge_unit_id, job_type, status, error, created_at, updated_at) VALUES
                    ('old-failed', 'k1', 'internalize', 'failed', 'old error', 10, 10),
                    ('new-ok', 'k1', 'internalize', 'completed', NULL, 20, 21),
                    ('questions-failed', 'k1', 'question_generation', 'failed', 'question error', 22, 23);
                 INSERT INTO librarian_runs(id, status, error, created_at, updated_at) VALUES
                    ('lib-old', 'failed', 'old organizer error', 5, 5),
                    ('lib-new', 'ready', NULL, 24, 25);
                 INSERT INTO curation_runs(id, status, error, created_at, updated_at)
                    VALUES ('cur-new', 'running', NULL, 26, 27);"
            )
            .unwrap();

        let tasks = load_task_list(&connection, 20).unwrap();
        let ids = tasks.iter().map(|task| task.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"new-ok"));
        assert!(ids.contains(&"questions-failed"));
        assert!(ids.contains(&"lib-new"));
        assert!(ids.contains(&"cur-new"));
        assert!(!ids.contains(&"old-failed"));
        assert!(!ids.contains(&"lib-old"));
        let librarian = tasks.iter().find(|task| task.id == "lib-new").unwrap();
        assert_eq!(librarian.status, "completed");
        let failed = tasks.iter().find(|task| task.id == "questions-failed").unwrap();
        assert!(failed.retryable);

        connection.execute("UPDATE inbox_items SET status = 'ignored' WHERE id = 'i1'", []).unwrap();
        let hidden = load_task_list(&connection, 20).unwrap();
        assert!(!hidden.iter().any(|task| task.id == "new-ok"));
        assert!(hidden.iter().any(|task| task.id == "questions-failed"));
    }
}
