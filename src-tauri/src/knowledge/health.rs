use serde::Serialize;
use tauri::State;

use crate::database::DatabaseState;

use super::now_ms;

const SAMPLE_LIMIT: i64 = 3;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHealthSample {
    pub knowledge_unit_id: String,
    pub title: String,
    pub status: String,
    pub other_knowledge_unit_id: Option<String>,
    pub other_title: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHealthBucket {
    pub count: i64,
    pub samples: Vec<KnowledgeHealthSample>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHealthReport {
    pub total_knowledge: i64,
    pub generated_at: i64,
    pub uncategorized: KnowledgeHealthBucket,
    pub no_questions: KnowledgeHealthBucket,
    pub never_reviewed: KnowledgeHealthBucket,
    pub possible_duplicates: KnowledgeHealthBucket,
    pub possible_conflicts: KnowledgeHealthBucket,
    pub source_link_issues: KnowledgeHealthBucket,
    pub stale_knowledge: KnowledgeHealthBucket,
}

#[tauri::command]
pub fn knowledge_health(state: State<'_, DatabaseState>) -> Result<KnowledgeHealthReport, String> {
    state.with_connection(|connection| load_health_report(connection))
}

fn load_health_report(connection: &rusqlite::Connection) -> Result<KnowledgeHealthReport, String> {
    let total_knowledge = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_units WHERE deleted_at IS NULL AND archived_at IS NULL AND status <> 'captured'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count active knowledge: {error}"))?;

    Ok(KnowledgeHealthReport {
        total_knowledge,
        generated_at: now_ms(),
        uncategorized: load_knowledge_bucket(
            connection,
            "NOT EXISTS(SELECT 1 FROM knowledge_topics kt WHERE kt.knowledge_unit_id = k.id)",
        )?,
        no_questions: load_knowledge_bucket(
            connection,
            "NOT EXISTS(SELECT 1 FROM questions q WHERE q.knowledge_unit_id = k.id)",
        )?,
        never_reviewed: load_knowledge_bucket(
            connection,
            "EXISTS(SELECT 1 FROM questions q WHERE q.knowledge_unit_id = k.id)
             AND COALESCE((SELECT review_count FROM review_states r WHERE r.knowledge_unit_id = k.id), 0) = 0",
        )?,
        possible_duplicates: load_curation_bucket(connection, "duplicate")?,
        possible_conflicts: load_curation_bucket(connection, "conflict")?,
        source_link_issues: load_knowledge_bucket(
            connection,
            "s.platform IN ('zhihu', 'web')
             AND (
                s.url IS NULL OR trim(s.url) = ''
                OR (lower(s.url) NOT LIKE 'http://%' AND lower(s.url) NOT LIKE 'https://%')
             )",
        )?,
        stale_knowledge: load_knowledge_bucket(
            connection,
            "EXISTS(
                SELECT 1 FROM knowledge_quality_states qs
                WHERE qs.knowledge_unit_id = k.id AND qs.status = 'stale'
             )",
        )?,
    })
}

fn load_knowledge_bucket(
    connection: &rusqlite::Connection,
    predicate: &str,
) -> Result<KnowledgeHealthBucket, String> {
    let base = format!(
        "FROM knowledge_units k
         JOIN sources s ON s.id = k.source_id
         WHERE k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured' AND ({predicate})"
    );
    let count = connection
        .query_row(&format!("SELECT COUNT(*) {base}"), [], |row| row.get(0))
        .map_err(|error| format!("failed to count knowledge health bucket: {error}"))?;
    let query = format!(
        "SELECT k.id,
                COALESCE(NULLIF(trim(k.core_claim), ''), NULLIF(trim(s.title), ''), substr(s.selected_text, 1, 160)),
                k.status
         {base}
         ORDER BY k.updated_at DESC, k.rowid DESC
         LIMIT {SAMPLE_LIMIT}"
    );
    let mut statement = connection
        .prepare(&query)
        .map_err(|error| format!("failed to prepare knowledge health samples: {error}"))?;
    let samples = statement
        .query_map([], |row| {
            Ok(KnowledgeHealthSample {
                knowledge_unit_id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                other_knowledge_unit_id: None,
                other_title: None,
            })
        })
        .map_err(|error| format!("failed to query knowledge health samples: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read knowledge health samples: {error}"))?;
    Ok(KnowledgeHealthBucket { count, samples })
}

fn load_curation_bucket(
    connection: &rusqlite::Connection,
    classification: &str,
) -> Result<KnowledgeHealthBucket, String> {
    let count = connection
        .query_row(
            "SELECT COUNT(*)
             FROM curation_candidates c
             JOIN knowledge_units left_k ON left_k.id = c.left_knowledge_id
             JOIN knowledge_units right_k ON right_k.id = c.right_knowledge_id
             WHERE c.status = 'pending' AND c.classification = ?1
               AND left_k.deleted_at IS NULL AND left_k.archived_at IS NULL
               AND right_k.deleted_at IS NULL AND right_k.archived_at IS NULL",
            [classification],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to count curation health bucket: {error}"))?;
    let mut statement = connection
        .prepare(
            "SELECT c.left_knowledge_id,
                    COALESCE(NULLIF(trim(left_k.core_claim), ''), NULLIF(trim(left_s.title), ''), substr(left_s.selected_text, 1, 160)),
                    left_k.status,
                    c.right_knowledge_id,
                    COALESCE(NULLIF(trim(right_k.core_claim), ''), NULLIF(trim(right_s.title), ''), substr(right_s.selected_text, 1, 160))
             FROM curation_candidates c
             JOIN knowledge_units left_k ON left_k.id = c.left_knowledge_id
             JOIN sources left_s ON left_s.id = left_k.source_id
             JOIN knowledge_units right_k ON right_k.id = c.right_knowledge_id
             JOIN sources right_s ON right_s.id = right_k.source_id
             WHERE c.status = 'pending' AND c.classification = ?1
               AND left_k.deleted_at IS NULL AND left_k.archived_at IS NULL
               AND right_k.deleted_at IS NULL AND right_k.archived_at IS NULL
             ORDER BY c.confidence DESC, c.created_at DESC
             LIMIT ?2",
        )
        .map_err(|error| format!("failed to prepare curation health samples: {error}"))?;
    let samples = statement
        .query_map(rusqlite::params![classification, SAMPLE_LIMIT], |row| {
            Ok(KnowledgeHealthSample {
                knowledge_unit_id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                other_knowledge_unit_id: row.get(3)?,
                other_title: row.get(4)?,
            })
        })
        .map_err(|error| format!("failed to query curation health samples: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read curation health samples: {error}"))?;
    Ok(KnowledgeHealthBucket { count, samples })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::apply_migrations;
    use rusqlite::Connection;

    #[test]
    fn health_report_combines_deterministic_and_latest_curation_signals() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, url, title, selected_text, content_hash, captured_at) VALUES
                    ('s1', 'web', NULL, 'Missing link', 'source one', 'health-hash-1', 10),
                    ('s2', 'zhihu', 'https://www.zhihu.com/a', 'Reviewed source', 'source two', 'health-hash-2', 11),
                    ('s3', 'web', 'https://example.test/a', 'Stale source', 'source three', 'health-hash-3', 12);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at) VALUES
                    ('k1', 's1', 'Needs organization', 'learning', 20, 20),
                    ('k2', 's2', 'Needs first review', 'learning', 21, 21),
                    ('k3', 's3', 'Old guidance', 'reviewing', 22, 22);
                 INSERT INTO review_states(knowledge_unit_id, mastery_score, review_count) VALUES
                    ('k1', 0, 0), ('k2', 40, 0), ('k3', 70, 2);
                 INSERT INTO questions(id, knowledge_unit_id, question_type, question, created_at) VALUES
                    ('q2', 'k2', 'explain', 'Explain k2', 30),
                    ('q3', 'k3', 'explain', 'Explain k3', 31);
                 INSERT INTO topics(id, name, description, created_by, locked, created_at, updated_at)
                    VALUES ('t1', 'Topic', '', 'user', 0, 40, 40);
                 INSERT INTO knowledge_topics(knowledge_unit_id, topic_id, created_by, created_at)
                    VALUES ('k2', 't1', 'user', 41), ('k3', 't1', 'user', 41);
                 UPDATE knowledge_quality_states SET status = 'stale' WHERE knowledge_unit_id = 'k3';
                 INSERT INTO curation_runs(id, status, knowledge_count, candidate_count, created_at, updated_at)
                    VALUES ('cr1', 'ready', 3, 2, 50, 50);
                 INSERT INTO curation_candidates(
                    id, run_id, left_knowledge_id, right_knowledge_id, classification,
                    rationale, confidence, status, created_at
                 ) VALUES
                    ('dup1', 'cr1', 'k1', 'k2', 'duplicate', 'same idea', 0.9, 'pending', 51),
                    ('conf1', 'cr1', 'k2', 'k3', 'conflict', 'opposing guidance', 0.88, 'pending', 52);"
            )
            .unwrap();

        let report = load_health_report(&connection).unwrap();
        assert_eq!(report.total_knowledge, 3);
        assert_eq!(report.uncategorized.count, 1);
        assert_eq!(report.no_questions.count, 1);
        assert_eq!(report.never_reviewed.count, 1);
        assert_eq!(report.possible_duplicates.count, 1);
        assert_eq!(report.possible_conflicts.count, 1);
        assert_eq!(report.source_link_issues.count, 1);
        assert_eq!(report.stale_knowledge.count, 1);
        assert_eq!(
            report.possible_duplicates.samples[0]
                .other_knowledge_unit_id
                .as_deref(),
            Some("k2")
        );
        assert_eq!(report.stale_knowledge.samples[0].knowledge_unit_id, "k3");
    }

    #[test]
    fn archived_knowledge_is_excluded_from_deterministic_health_buckets() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at)
                    VALUES ('s1', 'web', 'Archived', 'archived source', 'health-hash-4', 10);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at, archived_at)
                    VALUES ('k1', 's1', 'Archived knowledge', 'learning', 20, 20, 30);"
            )
            .unwrap();
        let report = load_health_report(&connection).unwrap();
        assert_eq!(report.total_knowledge, 0);
        assert_eq!(report.uncategorized.count, 0);
        assert_eq!(report.no_questions.count, 0);
        assert_eq!(report.source_link_issues.count, 0);
    }
}
