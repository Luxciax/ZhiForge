use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

use rusqlite::Connection;
use tauri::{AppHandle, Manager, State};

pub mod safety;

pub struct DatabaseState {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl DatabaseState {
    pub fn initialize(app: &AppHandle) -> Result<Self, String> {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
        fs::create_dir_all(&root)
            .map_err(|error| format!("failed to create app data directory: {error}"))?;

        let path = root.join("knowledge.db");
        let existed = path.exists()
            && fs::metadata(&path)
                .map(|metadata| metadata.len() > 0)
                .unwrap_or(false);
        let mut connection = Connection::open(&path)
            .map_err(|error| format!("failed to open knowledge database: {error}"))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| format!("failed to enable SQLite foreign keys: {error}"))?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|error| format!("failed to configure SQLite busy timeout: {error}"))?;
        if existed {
            if let Err(error) = safety::create_automatic_backup_if_due(&connection, &path) {
                eprintln!("automatic database backup failed: {error}");
            }
        }
        apply_migrations(&mut connection)?;
        connection
            .execute(
                "UPDATE ai_jobs
                 SET status = 'failed',
                     error = 'AI processing was interrupted before the app closed; retry to continue.',
                     updated_at = CAST(strftime('%s', 'now') AS INTEGER) * 1000
                 WHERE status IN ('pending', 'running')",
                [],
            )
            .map_err(|error| format!("failed to recover interrupted AI jobs: {error}"))?;
        connection
            .execute(
                "UPDATE inbox_items
                 SET status = 'failed',
                     processing_stage = 'failed',
                     error = COALESCE(error, 'AI processing was interrupted before the app closed; retry to continue.'),
                     updated_at = CAST(strftime('%s', 'now') AS INTEGER) * 1000
                 WHERE status = 'processing'",
                [],
            )
            .map_err(|error| format!("failed to recover interrupted inbox items: {error}"))?;

        Ok(Self {
            connection: Mutex::new(connection),
            path,
        })
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "knowledge database lock poisoned".to_string())?;
        operation(&mut connection)
    }

    fn path_string(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

#[tauri::command]
pub fn knowledge_database_path(state: State<'_, DatabaseState>) -> String {
    state.path_string()
}

pub(crate) fn apply_migrations(connection: &mut Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )
        .map_err(|error| format!("failed to initialize migration table: {error}"))?;

    let current: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to read database schema version: {error}"))?;

    if current < 1 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE sources (
                    id TEXT PRIMARY KEY,
                    platform TEXT NOT NULL,
                    url TEXT,
                    title TEXT,
                    author TEXT,
                    selected_text TEXT NOT NULL,
                    context_before TEXT,
                    context_after TEXT,
                    application TEXT,
                    window_title TEXT,
                    content_hash TEXT NOT NULL UNIQUE,
                    captured_at INTEGER NOT NULL
                );

                CREATE TABLE knowledge_units (
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL UNIQUE,
                    core_claim TEXT NOT NULL DEFAULT '',
                    concepts_json TEXT NOT NULL DEFAULT '[]',
                    prerequisites_json TEXT NOT NULL DEFAULT '[]',
                    important_details_json TEXT NOT NULL DEFAULT '[]',
                    limitations_json TEXT NOT NULL DEFAULT '[]',
                    user_note TEXT,
                    status TEXT NOT NULL DEFAULT 'captured',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE
                );

                CREATE TABLE evidence (
                    id TEXT PRIMARY KEY,
                    knowledge_unit_id TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    text TEXT NOT NULL,
                    start_offset INTEGER,
                    end_offset INTEGER,
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE
                );

                CREATE TABLE questions (
                    id TEXT PRIMARY KEY,
                    knowledge_unit_id TEXT NOT NULL,
                    question_type TEXT NOT NULL,
                    question TEXT NOT NULL,
                    reference_points_json TEXT NOT NULL DEFAULT '[]',
                    evidence_ids_json TEXT NOT NULL DEFAULT '[]',
                    difficulty INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE
                );

                CREATE TABLE attempts (
                    id TEXT PRIMARY KEY,
                    question_id TEXT NOT NULL,
                    answer TEXT NOT NULL,
                    score REAL,
                    result TEXT,
                    correct_points_json TEXT NOT NULL DEFAULT '[]',
                    missing_points_json TEXT NOT NULL DEFAULT '[]',
                    wrong_points_json TEXT NOT NULL DEFAULT '[]',
                    feedback TEXT,
                    evidence_ids_json TEXT NOT NULL DEFAULT '[]',
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(question_id) REFERENCES questions(id) ON DELETE CASCADE
                );

                CREATE TABLE review_states (
                    knowledge_unit_id TEXT PRIMARY KEY,
                    mastery_score INTEGER NOT NULL DEFAULT 0,
                    correct_count INTEGER NOT NULL DEFAULT 0,
                    wrong_count INTEGER NOT NULL DEFAULT 0,
                    review_count INTEGER NOT NULL DEFAULT 0,
                    last_reviewed_at INTEGER,
                    next_review_at INTEGER,
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE
                );

                CREATE TABLE ai_jobs (
                    id TEXT PRIMARY KEY,
                    knowledge_unit_id TEXT NOT NULL,
                    job_type TEXT NOT NULL,
                    status TEXT NOT NULL,
                    error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE
                );

                CREATE INDEX idx_sources_captured_at ON sources(captured_at DESC);
                CREATE INDEX idx_knowledge_units_status ON knowledge_units(status);
                CREATE INDEX idx_knowledge_units_updated_at ON knowledge_units(updated_at DESC);
                CREATE INDEX idx_questions_knowledge_unit ON questions(knowledge_unit_id);
                CREATE INDEX idx_attempts_question ON attempts(question_id);
                CREATE INDEX idx_review_next ON review_states(next_review_at);
                CREATE INDEX idx_ai_jobs_status ON ai_jobs(status);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 1: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (1, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 1: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 1: {error}"))?;
    }

    if current < 2 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 2: {error}"))?;
        transaction
            .execute(
                "UPDATE review_states
                 SET mastery_score = 40
                 WHERE mastery_score = 0
                   AND review_count = 0
                   AND knowledge_unit_id IN (
                       SELECT id FROM knowledge_units WHERE status = 'learning'
                   )",
                [],
            )
            .map_err(|error| format!("failed to backfill learning mastery baseline: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (2, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 2: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 2: {error}"))?;
    }

    if current < 3 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 3: {error}"))?;
        transaction
            .execute(
                "UPDATE knowledge_units
                 SET status = CASE
                    WHEN COALESCE((SELECT mastery_score FROM review_states WHERE knowledge_unit_id = knowledge_units.id), 0) <= 39 THEN 'weak'
                    WHEN COALESCE((SELECT mastery_score FROM review_states WHERE knowledge_unit_id = knowledge_units.id), 0) <= 69 THEN 'learning'
                    WHEN COALESCE((SELECT mastery_score FROM review_states WHERE knowledge_unit_id = knowledge_units.id), 0) <= 89 THEN 'reviewing'
                    ELSE 'mastered'
                 END
                 WHERE status IN ('learning', 'reviewing', 'weak', 'mastered')
                   AND EXISTS(SELECT 1 FROM review_states WHERE knowledge_unit_id = knowledge_units.id)",
                [],
            )
            .map_err(|error| format!("failed to backfill knowledge learning status: {error}"))?;
        transaction
            .execute(
                "UPDATE review_states
                 SET next_review_at = CAST(strftime('%s', 'now') AS INTEGER) * 1000
                 WHERE next_review_at IS NULL
                   AND knowledge_unit_id IN (
                       SELECT id FROM knowledge_units WHERE status IN ('learning', 'reviewing', 'weak', 'mastered')
                   )",
                [],
            )
            .map_err(|error| format!("failed to backfill next review time: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (3, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 3: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 3: {error}"))?;
    }

    if current < 4 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 4: {error}"))?;
        transaction
            .execute_batch(
                r#"
                ALTER TABLE knowledge_units ADD COLUMN archived_at INTEGER;
                ALTER TABLE knowledge_units ADD COLUMN deleted_at INTEGER;

                CREATE TABLE topics (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    description TEXT NOT NULL DEFAULT '',
                    created_by TEXT NOT NULL DEFAULT 'user',
                    locked INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    archived_at INTEGER,
                    deleted_at INTEGER
                );

                CREATE TABLE knowledge_topics (
                    knowledge_unit_id TEXT NOT NULL,
                    topic_id TEXT NOT NULL,
                    created_by TEXT NOT NULL DEFAULT 'user',
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, topic_id),
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    FOREIGN KEY(topic_id) REFERENCES topics(id) ON DELETE CASCADE
                );

                CREATE TABLE tags (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    created_by TEXT NOT NULL DEFAULT 'user',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE knowledge_tags (
                    knowledge_unit_id TEXT NOT NULL,
                    tag_id TEXT NOT NULL,
                    created_by TEXT NOT NULL DEFAULT 'user',
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, tag_id),
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
                );

                CREATE TABLE knowledge_relations (
                    id TEXT PRIMARY KEY,
                    source_knowledge_id TEXT NOT NULL,
                    target_knowledge_id TEXT NOT NULL,
                    relation_type TEXT NOT NULL,
                    confidence REAL,
                    created_by TEXT NOT NULL DEFAULT 'user',
                    confirmed INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(source_knowledge_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    FOREIGN KEY(target_knowledge_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    CHECK(source_knowledge_id <> target_knowledge_id),
                    UNIQUE(source_knowledge_id, target_knowledge_id, relation_type)
                );

                CREATE TABLE trash_records (
                    id TEXT PRIMARY KEY,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    deleted_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    UNIQUE(entity_type, entity_id)
                );

                CREATE INDEX idx_knowledge_archived ON knowledge_units(archived_at);
                CREATE INDEX idx_knowledge_deleted ON knowledge_units(deleted_at);
                CREATE INDEX idx_topics_deleted ON topics(deleted_at);
                CREATE INDEX idx_knowledge_topics_topic ON knowledge_topics(topic_id, knowledge_unit_id);
                CREATE INDEX idx_knowledge_tags_tag ON knowledge_tags(tag_id, knowledge_unit_id);
                CREATE INDEX idx_relations_source ON knowledge_relations(source_knowledge_id);
                CREATE INDEX idx_relations_target ON knowledge_relations(target_knowledge_id);
                CREATE INDEX idx_trash_deleted ON trash_records(deleted_at DESC);

                CREATE VIRTUAL TABLE knowledge_fts USING fts5(
                    knowledge_unit_id UNINDEXED,
                    core_claim,
                    user_note,
                    selected_text,
                    title,
                    author,
                    tokenize = 'unicode61 remove_diacritics 2'
                );

                INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                SELECT k.id, k.core_claim, COALESCE(k.user_note, ''), s.selected_text,
                       COALESCE(s.title, ''), COALESCE(s.author, '')
                FROM knowledge_units k
                JOIN sources s ON s.id = k.source_id;

                CREATE TRIGGER knowledge_fts_knowledge_insert AFTER INSERT ON knowledge_units BEGIN
                    INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                    SELECT NEW.id, NEW.core_claim, COALESCE(NEW.user_note, ''), s.selected_text,
                           COALESCE(s.title, ''), COALESCE(s.author, '')
                    FROM sources s WHERE s.id = NEW.source_id;
                END;

                CREATE TRIGGER knowledge_fts_knowledge_update
                AFTER UPDATE OF core_claim, user_note, source_id ON knowledge_units BEGIN
                    DELETE FROM knowledge_fts WHERE knowledge_unit_id = OLD.id;
                    INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                    SELECT NEW.id, NEW.core_claim, COALESCE(NEW.user_note, ''), s.selected_text,
                           COALESCE(s.title, ''), COALESCE(s.author, '')
                    FROM sources s WHERE s.id = NEW.source_id;
                END;

                CREATE TRIGGER knowledge_fts_knowledge_delete AFTER DELETE ON knowledge_units BEGIN
                    DELETE FROM knowledge_fts WHERE knowledge_unit_id = OLD.id;
                END;

                CREATE TRIGGER knowledge_fts_source_update
                AFTER UPDATE OF selected_text, title, author ON sources BEGIN
                    DELETE FROM knowledge_fts
                    WHERE knowledge_unit_id IN (SELECT id FROM knowledge_units WHERE source_id = NEW.id);
                    INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                    SELECT k.id, k.core_claim, COALESCE(k.user_note, ''), NEW.selected_text,
                           COALESCE(NEW.title, ''), COALESCE(NEW.author, '')
                    FROM knowledge_units k WHERE k.source_id = NEW.id;
                END;
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 4: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (4, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 4: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 4: {error}"))?;
    }

    if current < 5 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 5: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE audit_log (
                    id TEXT PRIMARY KEY,
                    action TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT,
                    detail_json TEXT,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX idx_audit_created_at ON audit_log(created_at DESC);
                CREATE INDEX idx_audit_entity ON audit_log(entity_type, entity_id, created_at DESC);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 5: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (5, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 5: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 5: {error}"))?;
    }

    if current < 6 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 6: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE librarian_runs (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    knowledge_count INTEGER NOT NULL DEFAULT 0,
                    summary TEXT,
                    error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE librarian_proposals (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    action_type TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    rationale TEXT NOT NULL DEFAULT '',
                    confidence REAL NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    result_json TEXT,
                    created_at INTEGER NOT NULL,
                    decided_at INTEGER,
                    FOREIGN KEY(run_id) REFERENCES librarian_runs(id) ON DELETE CASCADE
                );

                CREATE INDEX idx_librarian_runs_created ON librarian_runs(created_at DESC);
                CREATE INDEX idx_librarian_proposals_run ON librarian_proposals(run_id, status, confidence DESC);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 6: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (6, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 6: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 6: {error}"))?;
    }

    if current < 7 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 7: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE knowledge_quality_states (
                    knowledge_unit_id TEXT PRIMARY KEY,
                    status TEXT NOT NULL DEFAULT 'unverified',
                    reason TEXT,
                    updated_by TEXT NOT NULL DEFAULT 'system',
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    CHECK(status IN ('unverified', 'verified', 'conflicted', 'stale', 'needs_expansion'))
                );

                CREATE TABLE claims (
                    id TEXT PRIMARY KEY,
                    knowledge_unit_id TEXT NOT NULL,
                    text TEXT NOT NULL,
                    claim_type TEXT NOT NULL DEFAULT 'primary',
                    created_by TEXT NOT NULL DEFAULT 'system',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    retired_at INTEGER,
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    CHECK(claim_type IN ('primary', 'supporting'))
                );

                CREATE UNIQUE INDEX idx_claims_primary_active
                    ON claims(knowledge_unit_id)
                    WHERE claim_type = 'primary' AND retired_at IS NULL;
                CREATE INDEX idx_claims_knowledge ON claims(knowledge_unit_id, retired_at, updated_at DESC);

                CREATE TABLE claim_evidence (
                    claim_id TEXT NOT NULL,
                    evidence_id TEXT NOT NULL,
                    stance TEXT NOT NULL DEFAULT 'supports',
                    confidence REAL,
                    created_by TEXT NOT NULL DEFAULT 'system',
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(claim_id, evidence_id, stance),
                    FOREIGN KEY(claim_id) REFERENCES claims(id) ON DELETE CASCADE,
                    FOREIGN KEY(evidence_id) REFERENCES evidence(id) ON DELETE CASCADE,
                    CHECK(stance IN ('supports', 'conflicts')),
                    CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0))
                );
                CREATE INDEX idx_claim_evidence_evidence ON claim_evidence(evidence_id, claim_id);

                CREATE TABLE knowledge_source_links (
                    knowledge_unit_id TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    role TEXT NOT NULL DEFAULT 'supporting',
                    created_by TEXT NOT NULL DEFAULT 'system',
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, source_id, role),
                    FOREIGN KEY(knowledge_unit_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE,
                    CHECK(role IN ('origin', 'supporting', 'conflicting'))
                );
                CREATE INDEX idx_knowledge_source_links_source
                    ON knowledge_source_links(source_id, knowledge_unit_id, role);

                INSERT INTO knowledge_quality_states(knowledge_unit_id, status, reason, updated_by, updated_at)
                SELECT id, 'unverified', NULL, 'migration', updated_at
                FROM knowledge_units;

                INSERT INTO knowledge_source_links(knowledge_unit_id, source_id, role, created_by, created_at)
                SELECT id, source_id, 'origin', 'migration', created_at
                FROM knowledge_units;

                INSERT INTO claims(id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at, retired_at)
                SELECT 'primary:' || id, id, core_claim, 'primary', 'migration', created_at, updated_at, NULL
                FROM knowledge_units
                WHERE trim(core_claim) <> '';

                INSERT INTO claim_evidence(claim_id, evidence_id, stance, confidence, created_by, created_at)
                SELECT 'primary:' || e.knowledge_unit_id, e.id, 'supports', 1.0, 'migration',
                       COALESCE((SELECT updated_at FROM knowledge_units k WHERE k.id = e.knowledge_unit_id), unixepoch() * 1000)
                FROM evidence e
                WHERE EXISTS(
                    SELECT 1 FROM claims c
                    WHERE c.id = 'primary:' || e.knowledge_unit_id
                );

                CREATE TRIGGER knowledge_quality_after_insert
                AFTER INSERT ON knowledge_units BEGIN
                    INSERT OR IGNORE INTO knowledge_quality_states(
                        knowledge_unit_id, status, reason, updated_by, updated_at
                    ) VALUES (NEW.id, 'unverified', NULL, 'system', NEW.created_at);
                    INSERT OR IGNORE INTO knowledge_source_links(
                        knowledge_unit_id, source_id, role, created_by, created_at
                    ) VALUES (NEW.id, NEW.source_id, 'origin', 'system', NEW.created_at);
                END;

                CREATE TRIGGER knowledge_primary_claim_after_insert
                AFTER INSERT ON knowledge_units
                WHEN trim(NEW.core_claim) <> '' BEGIN
                    INSERT OR IGNORE INTO claims(
                        id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at, retired_at
                    ) VALUES (
                        'primary:' || NEW.id, NEW.id, NEW.core_claim, 'primary', 'system', NEW.created_at, NEW.updated_at, NULL
                    );
                END;

                CREATE TRIGGER knowledge_primary_claim_after_core_claim
                AFTER UPDATE OF core_claim ON knowledge_units
                WHEN trim(NEW.core_claim) <> '' BEGIN
                    INSERT INTO claims(
                        id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at, retired_at
                    ) VALUES (
                        'primary:' || NEW.id, NEW.id, NEW.core_claim, 'primary', 'system', NEW.created_at, NEW.updated_at, NULL
                    )
                    ON CONFLICT(id) DO UPDATE SET
                        text = excluded.text,
                        updated_at = excluded.updated_at,
                        retired_at = NULL;
                END;
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 7: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (7, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 7: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 7: {error}"))?;
    }

    if current < 8 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 8: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE curation_runs (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    knowledge_count INTEGER NOT NULL DEFAULT 0,
                    candidate_count INTEGER NOT NULL DEFAULT 0,
                    summary TEXT,
                    error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE curation_candidates (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    left_knowledge_id TEXT NOT NULL,
                    right_knowledge_id TEXT NOT NULL,
                    classification TEXT NOT NULL,
                    relation_type TEXT,
                    rationale TEXT NOT NULL DEFAULT '',
                    confidence REAL NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    result_json TEXT,
                    created_at INTEGER NOT NULL,
                    decided_at INTEGER,
                    FOREIGN KEY(run_id) REFERENCES curation_runs(id) ON DELETE CASCADE,
                    FOREIGN KEY(left_knowledge_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    FOREIGN KEY(right_knowledge_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    CHECK(classification IN ('duplicate', 'related', 'conflict')),
                    CHECK(confidence >= 0.0 AND confidence <= 1.0),
                    CHECK(left_knowledge_id <> right_knowledge_id)
                );

                CREATE INDEX idx_curation_runs_created ON curation_runs(created_at DESC);
                CREATE INDEX idx_curation_candidates_run
                    ON curation_candidates(run_id, status, confidence DESC);
                CREATE INDEX idx_curation_candidates_pair
                    ON curation_candidates(left_knowledge_id, right_knowledge_id, classification);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 8: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (8, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 8: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 8: {error}"))?;
    }

    if current < 9 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 9: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE knowledge_merges (
                    id TEXT PRIMARY KEY,
                    curation_candidate_id TEXT NOT NULL,
                    target_knowledge_id TEXT NOT NULL,
                    source_knowledge_id TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'applied',
                    snapshot_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    reverted_at INTEGER,
                    FOREIGN KEY(curation_candidate_id) REFERENCES curation_candidates(id),
                    FOREIGN KEY(target_knowledge_id) REFERENCES knowledge_units(id),
                    FOREIGN KEY(source_knowledge_id) REFERENCES knowledge_units(id),
                    CHECK(target_knowledge_id <> source_knowledge_id),
                    CHECK(status IN ('applied', 'reverted'))
                );

                CREATE UNIQUE INDEX idx_knowledge_merges_active_candidate
                    ON knowledge_merges(curation_candidate_id)
                    WHERE status = 'applied';
                CREATE INDEX idx_knowledge_merges_target
                    ON knowledge_merges(target_knowledge_id, status, created_at DESC);
                CREATE INDEX idx_knowledge_merges_source
                    ON knowledge_merges(source_knowledge_id, status, created_at DESC);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 9: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (9, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 9: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 9: {error}"))?;
    }

    if current < 10 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 10: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE review_sessions (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL DEFAULT 'active',
                    due_before INTEGER NOT NULL,
                    item_count INTEGER NOT NULL DEFAULT 0,
                    completed_count INTEGER NOT NULL DEFAULT 0,
                    skipped_count INTEGER NOT NULL DEFAULT 0,
                    started_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    ended_at INTEGER,
                    CHECK(status IN ('active', 'completed', 'abandoned'))
                );

                CREATE TABLE review_session_items (
                    session_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    knowledge_unit_id TEXT NOT NULL,
                    question_id TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    completed_at INTEGER,
                    core_claim_snapshot TEXT NOT NULL,
                    selected_text_snapshot TEXT NOT NULL,
                    platform_snapshot TEXT NOT NULL,
                    title_snapshot TEXT,
                    author_snapshot TEXT,
                    mastery_snapshot INTEGER NOT NULL,
                    wrong_count_snapshot INTEGER NOT NULL,
                    review_count_snapshot INTEGER NOT NULL,
                    next_review_at_snapshot INTEGER NOT NULL,
                    PRIMARY KEY(session_id, position),
                    UNIQUE(session_id, knowledge_unit_id),
                    FOREIGN KEY(session_id) REFERENCES review_sessions(id) ON DELETE CASCADE,
                    CHECK(status IN ('pending', 'completed', 'skipped'))
                );

                CREATE UNIQUE INDEX idx_review_sessions_active
                    ON review_sessions(status)
                    WHERE status = 'active';
                CREATE INDEX idx_review_sessions_started
                    ON review_sessions(started_at DESC);
                CREATE INDEX idx_review_session_items_pending
                    ON review_session_items(session_id, status, position);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 10: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (10, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 10: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 10: {error}"))?;
    }

    if current < 11 {
        migrate_knowledge_units_source_cardinality_v11(connection)?;
    }

    if current < 12 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 12: {error}"))?;
        transaction
            .execute_batch(
                r#"
                ALTER TABLE review_states ADD COLUMN stability REAL NOT NULL DEFAULT 1.0;
                ALTER TABLE review_states ADD COLUMN difficulty REAL NOT NULL DEFAULT 5.0;
                ALTER TABLE review_states ADD COLUMN lapse_count INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE review_states ADD COLUMN scheduled_days INTEGER NOT NULL DEFAULT 1;
                ALTER TABLE review_states ADD COLUMN last_result TEXT;
                ALTER TABLE review_states ADD COLUMN scheduler_version INTEGER NOT NULL DEFAULT 1;

                CREATE TABLE review_scheduler_events (
                    id TEXT PRIMARY KEY,
                    attempt_id TEXT NOT NULL UNIQUE,
                    knowledge_unit_id TEXT NOT NULL,
                    result TEXT NOT NULL CHECK(result IN ('correct', 'partial', 'wrong')),
                    reviewed_at INTEGER NOT NULL,
                    previous_mastery_score INTEGER NOT NULL,
                    mastery_score INTEGER NOT NULL,
                    elapsed_days REAL NOT NULL,
                    retrievability REAL NOT NULL,
                    previous_stability REAL NOT NULL,
                    stability REAL NOT NULL,
                    previous_difficulty REAL NOT NULL,
                    difficulty REAL NOT NULL,
                    previous_lapse_count INTEGER NOT NULL,
                    lapse_count INTEGER NOT NULL,
                    previous_scheduled_days INTEGER NOT NULL,
                    scheduled_days INTEGER NOT NULL,
                    next_review_at INTEGER NOT NULL,
                    scheduler_version INTEGER NOT NULL
                );
                CREATE INDEX idx_review_scheduler_events_knowledge
                    ON review_scheduler_events(knowledge_unit_id, reviewed_at DESC);

                UPDATE review_states
                SET stability = CASE
                        WHEN last_reviewed_at IS NOT NULL
                             AND next_review_at IS NOT NULL
                             AND next_review_at > last_reviewed_at
                        THEN MIN(3650.0, MAX(0.25, (next_review_at - last_reviewed_at) / 86400000.0))
                        WHEN mastery_score <= 39 THEN 0.5
                        WHEN mastery_score <= 69 THEN 1.0
                        WHEN mastery_score <= 89 THEN 3.0
                        ELSE 7.0
                    END,
                    difficulty = MIN(
                        10.0,
                        MAX(1.0, 8.0 - (mastery_score / 20.0) + MIN(1.5, MAX(0, wrong_count) * 0.2))
                    ),
                    lapse_count = MAX(0, wrong_count),
                    scheduled_days = CASE
                        WHEN last_reviewed_at IS NOT NULL
                             AND next_review_at IS NOT NULL
                             AND next_review_at >= last_reviewed_at
                        THEN MIN(365, MAX(0, CAST(ROUND((next_review_at - last_reviewed_at) / 86400000.0) AS INTEGER)))
                        WHEN next_review_at IS NULL THEN 0
                        WHEN mastery_score <= 69 THEN 1
                        WHEN mastery_score <= 89 THEN 3
                        ELSE 7
                    END,
                    last_result = NULL,
                    scheduler_version = 1;
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 12: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (12, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 12: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 12: {error}"))?;
    }

    if current < 13 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 13: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE inbox_items (
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL UNIQUE,
                    anchor_knowledge_id TEXT NOT NULL UNIQUE,
                    status TEXT NOT NULL DEFAULT 'captured',
                    processing_stage TEXT NOT NULL DEFAULT 'captured',
                    progress_current INTEGER NOT NULL DEFAULT 0,
                    progress_total INTEGER NOT NULL DEFAULT 0,
                    error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE,
                    FOREIGN KEY(anchor_knowledge_id) REFERENCES knowledge_units(id) ON DELETE CASCADE,
                    CHECK(status IN ('captured', 'processing', 'ready', 'failed', 'accepted', 'ignored'))
                );

                CREATE TABLE knowledge_drafts (
                    id TEXT PRIMARY KEY,
                    inbox_id TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    core_claim TEXT NOT NULL,
                    concepts_json TEXT NOT NULL DEFAULT '[]',
                    prerequisites_json TEXT NOT NULL DEFAULT '[]',
                    important_details_json TEXT NOT NULL DEFAULT '[]',
                    limitations_json TEXT NOT NULL DEFAULT '[]',
                    evidence_json TEXT NOT NULL DEFAULT '[]',
                    decision TEXT NOT NULL DEFAULT 'pending',
                    classification TEXT NOT NULL DEFAULT 'new',
                    related_knowledge_id TEXT,
                    relation_type TEXT,
                    rationale TEXT,
                    confidence REAL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(inbox_id) REFERENCES inbox_items(id) ON DELETE CASCADE,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE,
                    FOREIGN KEY(related_knowledge_id) REFERENCES knowledge_units(id) ON DELETE SET NULL,
                    UNIQUE(inbox_id, position),
                    CHECK(decision IN ('pending', 'accepted', 'ignored')),
                    CHECK(classification IN ('new', 'duplicate', 'supplement', 'conflict')),
                    CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0))
                );

                CREATE INDEX idx_inbox_status_updated
                    ON inbox_items(status, updated_at DESC);
                CREATE INDEX idx_inbox_source
                    ON inbox_items(source_id);
                CREATE INDEX idx_drafts_inbox_position
                    ON knowledge_drafts(inbox_id, position);
                CREATE INDEX idx_drafts_decision
                    ON knowledge_drafts(inbox_id, decision, position);

                INSERT OR IGNORE INTO inbox_items(
                    id, source_id, anchor_knowledge_id, status, processing_stage,
                    progress_current, progress_total, error, created_at, updated_at
                )
                SELECT
                    'inbox:' || k.id,
                    k.source_id,
                    k.id,
                    'captured',
                    'captured',
                    0,
                    0,
                    NULL,
                    k.created_at,
                    k.updated_at
                FROM knowledge_units k
                WHERE k.status = 'captured'
                  AND trim(k.core_claim) = ''
                  AND k.deleted_at IS NULL;
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 13: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (13, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 13: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 13: {error}"))?;
    }

    if current < 14 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 14: {error}"))?;
        transaction
            .execute_batch(
                r#"
                CREATE TABLE research_plans (
                    id TEXT PRIMARY KEY,
                    topic TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    coverage_score INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL DEFAULT 'active',
                    source_knowledge_ids_json TEXT NOT NULL DEFAULT '[]',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    CHECK(coverage_score >= 0 AND coverage_score <= 100),
                    CHECK(status IN ('active', 'completed', 'archived'))
                );

                CREATE TABLE research_gaps (
                    id TEXT PRIMARY KEY,
                    plan_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    title TEXT NOT NULL,
                    rationale TEXT NOT NULL DEFAULT '',
                    priority TEXT NOT NULL DEFAULT 'medium',
                    search_query TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'open',
                    related_knowledge_ids_json TEXT NOT NULL DEFAULT '[]',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(plan_id) REFERENCES research_plans(id) ON DELETE CASCADE,
                    UNIQUE(plan_id, position),
                    CHECK(priority IN ('high', 'medium', 'low')),
                    CHECK(status IN ('open', 'collecting', 'covered', 'dismissed'))
                );

                CREATE TABLE research_gap_sources (
                    gap_id TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(gap_id, source_id),
                    FOREIGN KEY(gap_id) REFERENCES research_gaps(id) ON DELETE CASCADE,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE
                );

                CREATE INDEX idx_research_plans_updated
                    ON research_plans(status, updated_at DESC);
                CREATE INDEX idx_research_gaps_plan
                    ON research_gaps(plan_id, status, position);
                CREATE INDEX idx_research_gap_sources_source
                    ON research_gap_sources(source_id, gap_id);
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 14: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (14, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 14: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 14: {error}"))?;
    }

    if current < 15 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 15: {error}"))?;
        let accepted_column_exists: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('knowledge_drafts') WHERE name = 'accepted_knowledge_id'",
                [],
                |row| row.get(0),
            )
            .map_err(|error| format!("failed to inspect database migration 15 columns: {error}"))?;
        if accepted_column_exists == 0 {
            transaction
                .execute(
                    "ALTER TABLE knowledge_drafts
                     ADD COLUMN accepted_knowledge_id TEXT REFERENCES knowledge_units(id) ON DELETE SET NULL",
                    [],
                )
                .map_err(|error| format!("failed to add accepted knowledge mapping in migration 15: {error}"))?;
        }
        transaction
            .execute_batch(
                r#"
                CREATE INDEX IF NOT EXISTS idx_drafts_accepted_knowledge
                    ON knowledge_drafts(accepted_knowledge_id);

                UPDATE knowledge_drafts
                SET accepted_knowledge_id = related_knowledge_id
                WHERE decision = 'accepted'
                  AND classification = 'duplicate'
                  AND related_knowledge_id IS NOT NULL;

                UPDATE knowledge_drafts
                SET accepted_knowledge_id = (
                    SELECT k.id
                    FROM knowledge_units k
                    WHERE k.source_id = knowledge_drafts.source_id
                      AND k.status <> 'captured'
                      AND k.deleted_at IS NULL
                      AND k.core_claim = knowledge_drafts.core_claim
                    ORDER BY k.created_at ASC, k.rowid ASC
                    LIMIT 1
                )
                WHERE decision = 'accepted'
                  AND classification <> 'duplicate'
                  AND accepted_knowledge_id IS NULL
                  AND 1 = (
                    SELECT COUNT(*)
                    FROM knowledge_units k
                    WHERE k.source_id = knowledge_drafts.source_id
                      AND k.status <> 'captured'
                      AND k.deleted_at IS NULL
                      AND k.core_claim = knowledge_drafts.core_claim
                  );
                "#,
            )
            .map_err(|error| format!("failed to apply database migration 15: {error}"))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (15, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 15: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 15: {error}"))?;
    }

    Ok(())
}

fn migrate_knowledge_units_source_cardinality_v11(
    connection: &mut Connection,
) -> Result<(), String> {
    let foreign_keys_enabled: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|error| {
            format!("failed to read SQLite foreign_keys state before migration 11: {error}")
        })?;
    if foreign_keys_enabled != 0 {
        connection
            .pragma_update(None, "foreign_keys", "OFF")
            .map_err(|error| {
                format!("failed to suspend SQLite foreign keys for migration 11: {error}")
            })?;
    }

    let migration_result = (|| -> Result<(), String> {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start database migration 11: {error}"))?;
        transaction
            .execute_batch(
                r#"
                DROP TRIGGER IF EXISTS knowledge_fts_knowledge_insert;
                DROP TRIGGER IF EXISTS knowledge_fts_knowledge_update;
                DROP TRIGGER IF EXISTS knowledge_fts_knowledge_delete;
                DROP TRIGGER IF EXISTS knowledge_fts_source_update;
                DROP TRIGGER IF EXISTS knowledge_quality_after_insert;
                DROP TRIGGER IF EXISTS knowledge_primary_claim_after_insert;
                DROP TRIGGER IF EXISTS knowledge_primary_claim_after_core_claim;

                CREATE TABLE knowledge_units_v11 (
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL,
                    core_claim TEXT NOT NULL DEFAULT '',
                    concepts_json TEXT NOT NULL DEFAULT '[]',
                    prerequisites_json TEXT NOT NULL DEFAULT '[]',
                    important_details_json TEXT NOT NULL DEFAULT '[]',
                    limitations_json TEXT NOT NULL DEFAULT '[]',
                    user_note TEXT,
                    status TEXT NOT NULL DEFAULT 'captured',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    archived_at INTEGER,
                    deleted_at INTEGER,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE
                );

                INSERT INTO knowledge_units_v11(
                    id, source_id, core_claim, concepts_json, prerequisites_json,
                    important_details_json, limitations_json, user_note, status,
                    created_at, updated_at, archived_at, deleted_at
                )
                SELECT
                    id, source_id, core_claim, concepts_json, prerequisites_json,
                    important_details_json, limitations_json, user_note, status,
                    created_at, updated_at, archived_at, deleted_at
                FROM knowledge_units;

                DROP TABLE knowledge_units;
                ALTER TABLE knowledge_units_v11 RENAME TO knowledge_units;

                CREATE INDEX idx_knowledge_units_status ON knowledge_units(status);
                CREATE INDEX idx_knowledge_units_updated_at ON knowledge_units(updated_at DESC);
                CREATE INDEX idx_knowledge_archived ON knowledge_units(archived_at);
                CREATE INDEX idx_knowledge_deleted ON knowledge_units(deleted_at);

                CREATE TRIGGER knowledge_fts_knowledge_insert AFTER INSERT ON knowledge_units BEGIN
                    INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                    SELECT NEW.id, NEW.core_claim, COALESCE(NEW.user_note, ''), s.selected_text,
                           COALESCE(s.title, ''), COALESCE(s.author, '')
                    FROM sources s WHERE s.id = NEW.source_id;
                END;

                CREATE TRIGGER knowledge_fts_knowledge_update
                AFTER UPDATE OF core_claim, user_note, source_id ON knowledge_units BEGIN
                    DELETE FROM knowledge_fts WHERE knowledge_unit_id = OLD.id;
                    INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                    SELECT NEW.id, NEW.core_claim, COALESCE(NEW.user_note, ''), s.selected_text,
                           COALESCE(s.title, ''), COALESCE(s.author, '')
                    FROM sources s WHERE s.id = NEW.source_id;
                END;

                CREATE TRIGGER knowledge_fts_knowledge_delete AFTER DELETE ON knowledge_units BEGIN
                    DELETE FROM knowledge_fts WHERE knowledge_unit_id = OLD.id;
                END;

                CREATE TRIGGER knowledge_fts_source_update
                AFTER UPDATE OF selected_text, title, author ON sources BEGIN
                    DELETE FROM knowledge_fts
                    WHERE knowledge_unit_id IN (SELECT id FROM knowledge_units WHERE source_id = NEW.id);
                    INSERT INTO knowledge_fts(knowledge_unit_id, core_claim, user_note, selected_text, title, author)
                    SELECT k.id, k.core_claim, COALESCE(k.user_note, ''), NEW.selected_text,
                           COALESCE(NEW.title, ''), COALESCE(NEW.author, '')
                    FROM knowledge_units k WHERE k.source_id = NEW.id;
                END;

                CREATE TRIGGER knowledge_quality_after_insert
                AFTER INSERT ON knowledge_units BEGIN
                    INSERT OR IGNORE INTO knowledge_quality_states(
                        knowledge_unit_id, status, reason, updated_by, updated_at
                    ) VALUES (NEW.id, 'unverified', NULL, 'system', NEW.created_at);
                    INSERT OR IGNORE INTO knowledge_source_links(
                        knowledge_unit_id, source_id, role, created_by, created_at
                    ) VALUES (NEW.id, NEW.source_id, 'origin', 'system', NEW.created_at);
                END;

                CREATE TRIGGER knowledge_primary_claim_after_insert
                AFTER INSERT ON knowledge_units
                WHEN trim(NEW.core_claim) <> '' BEGIN
                    INSERT OR IGNORE INTO claims(
                        id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at, retired_at
                    ) VALUES (
                        'primary:' || NEW.id, NEW.id, NEW.core_claim, 'primary', 'system', NEW.created_at, NEW.updated_at, NULL
                    );
                END;

                CREATE TRIGGER knowledge_primary_claim_after_core_claim
                AFTER UPDATE OF core_claim ON knowledge_units
                WHEN trim(NEW.core_claim) <> '' BEGIN
                    INSERT INTO claims(
                        id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at, retired_at
                    ) VALUES (
                        'primary:' || NEW.id, NEW.id, NEW.core_claim, 'primary', 'system', NEW.created_at, NEW.updated_at, NULL
                    )
                    ON CONFLICT(id) DO UPDATE SET
                        text = excluded.text,
                        updated_at = excluded.updated_at,
                        retired_at = NULL;
                END;
                "#,
            )
            .map_err(|error| format!("failed to rebuild knowledge_units for migration 11: {error}"))?;

        let foreign_key_violation = {
            let mut statement =
                transaction
                    .prepare("PRAGMA foreign_key_check")
                    .map_err(|error| {
                        format!(
                            "failed to prepare foreign key validation for migration 11: {error}"
                        )
                    })?;
            let mut rows = statement.query([]).map_err(|error| {
                format!("failed to run foreign key validation for migration 11: {error}")
            })?;
            rows.next()
                .map_err(|error| {
                    format!("failed to read foreign key validation for migration 11: {error}")
                })?
                .is_some()
        };
        if foreign_key_violation {
            return Err("database migration 11 would leave foreign key violations".into());
        }

        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (11, unixepoch() * 1000)",
                [],
            )
            .map_err(|error| format!("failed to record database migration 11: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit database migration 11: {error}"))?;
        Ok(())
    })();

    let restore_result = if foreign_keys_enabled != 0 {
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| {
                format!("failed to restore SQLite foreign keys after migration 11: {error}")
            })
    } else {
        Ok(())
    };

    migration_result?;
    restore_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_twelve_backfills_scheduler_from_existing_review_state() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
                 CREATE TABLE sources(id TEXT PRIMARY KEY);
                 CREATE TABLE knowledge_units(
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL,
                    core_claim TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL DEFAULT 'captured',
                    created_at INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL DEFAULT 0,
                    deleted_at INTEGER
                 );
                 CREATE TABLE review_states(
                    knowledge_unit_id TEXT PRIMARY KEY,
                    mastery_score INTEGER NOT NULL DEFAULT 0,
                    correct_count INTEGER NOT NULL DEFAULT 0,
                    wrong_count INTEGER NOT NULL DEFAULT 0,
                    review_count INTEGER NOT NULL DEFAULT 0,
                    last_reviewed_at INTEGER,
                    next_review_at INTEGER
                 );
                 INSERT INTO schema_migrations(version, applied_at) VALUES (11, 0);
                 INSERT INTO review_states(
                    knowledge_unit_id, mastery_score, wrong_count, review_count,
                    last_reviewed_at, next_review_at
                 ) VALUES ('k1', 80, 2, 5, 1000, 259201000);"
            )
            .unwrap();

        apply_migrations(&mut connection).unwrap();

        let (stability, difficulty, lapse_count, scheduled_days, last_result, scheduler_version):
            (f64, f64, i64, i64, Option<String>, i64) = connection
            .query_row(
                "SELECT stability, difficulty, lapse_count, scheduled_days, last_result, scheduler_version
                 FROM review_states WHERE knowledge_unit_id = 'k1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .unwrap();
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let scheduler_events_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'review_scheduler_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!((stability - 3.0).abs() < 0.001);
        assert!((difficulty - 4.4).abs() < 0.001);
        assert_eq!(lapse_count, 2);
        assert_eq!(scheduled_days, 3);
        assert_eq!(last_result, None);
        assert_eq!(scheduler_version, 1);
        assert_eq!(scheduler_events_table, 1);
        assert_eq!(version, 15);
    }

    #[test]
    fn migration_thirteen_creates_inbox_and_backfills_unresolved_capture() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE research_gap_sources;
                 DROP TABLE research_gaps;
                 DROP TABLE research_plans;
                 DROP TABLE knowledge_drafts;
                 DROP TABLE inbox_items;
                 DELETE FROM schema_migrations WHERE version >= 13;
                 INSERT INTO sources(id, platform, selected_text, content_hash, captured_at)
                    VALUES ('s-captured', 'web', 'captured text', 'migration13-captured', 10),
                           ('s-learning', 'web', 'learning text', 'migration13-learning', 11);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                    VALUES ('k-captured', 's-captured', '', 'captured', 20, 20),
                           ('k-learning', 's-learning', 'Already formal', 'learning', 21, 21);"
            )
            .unwrap();

        apply_migrations(&mut connection).unwrap();

        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        let inbox_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'inbox_items'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let draft_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_drafts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let backfilled: (String, String, String) = connection
            .query_row(
                "SELECT source_id, anchor_knowledge_id, status FROM inbox_items WHERE id = 'inbox:k-captured'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let formal_backfill_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM inbox_items WHERE anchor_knowledge_id = 'k-learning'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 15);
        assert_eq!(inbox_table, 1);
        assert_eq!(draft_table, 1);
        assert_eq!(backfilled, ("s-captured".into(), "k-captured".into(), "captured".into()));
        assert_eq!(formal_backfill_count, 0);
    }

    #[test]
    fn migration_fourteen_creates_persistent_research_plan_tables() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE research_gap_sources;
                 DROP TABLE research_gaps;
                 DROP TABLE research_plans;
                 DELETE FROM schema_migrations WHERE version >= 14;"
            )
            .unwrap();

        apply_migrations(&mut connection).unwrap();

        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('research_plans', 'research_gaps', 'research_gap_sources')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO research_plans(id, topic, summary, coverage_score, status, created_at, updated_at)
                 VALUES ('rp1', 'Agent memory', 'summary', 42, 'active', 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO research_gaps(id, plan_id, position, title, rationale, priority, search_query, status, created_at, updated_at)
                 VALUES ('rg1', 'rp1', 0, 'Failure modes', 'missing', 'high', 'Agent memory failure modes', 'open', 1, 1)",
                [],
            )
            .unwrap();
        let gap_status: String = connection
            .query_row("SELECT status FROM research_gaps WHERE id = 'rg1'", [], |row| row.get(0))
            .unwrap();

        assert_eq!(version, 15);
        assert_eq!(tables, 3);
        assert_eq!(gap_status, "open");
    }

    #[test]
    fn migration_fifteen_tracks_accepted_draft_knowledge_targets() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "DROP INDEX idx_drafts_accepted_knowledge;
                 DELETE FROM schema_migrations WHERE version = 15;"
            )
            .unwrap();
        connection
            .execute_batch(
                "ALTER TABLE knowledge_drafts RENAME TO knowledge_drafts_v15_current;
                 CREATE TABLE knowledge_drafts (
                    id TEXT PRIMARY KEY,
                    inbox_id TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    core_claim TEXT NOT NULL,
                    concepts_json TEXT NOT NULL DEFAULT '[]',
                    prerequisites_json TEXT NOT NULL DEFAULT '[]',
                    important_details_json TEXT NOT NULL DEFAULT '[]',
                    limitations_json TEXT NOT NULL DEFAULT '[]',
                    evidence_json TEXT NOT NULL DEFAULT '[]',
                    decision TEXT NOT NULL DEFAULT 'pending',
                    classification TEXT NOT NULL DEFAULT 'new',
                    related_knowledge_id TEXT,
                    relation_type TEXT,
                    rationale TEXT,
                    confidence REAL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(inbox_id) REFERENCES inbox_items(id) ON DELETE CASCADE,
                    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE,
                    FOREIGN KEY(related_knowledge_id) REFERENCES knowledge_units(id) ON DELETE SET NULL,
                    UNIQUE(inbox_id, position)
                 );
                 INSERT INTO knowledge_drafts(
                    id, inbox_id, source_id, position, core_claim, concepts_json,
                    prerequisites_json, important_details_json, limitations_json,
                    evidence_json, decision, classification, related_knowledge_id,
                    relation_type, rationale, confidence, created_at, updated_at
                 )
                 SELECT id, inbox_id, source_id, position, core_claim, concepts_json,
                    prerequisites_json, important_details_json, limitations_json,
                    evidence_json, decision, classification, related_knowledge_id,
                    relation_type, rationale, confidence, created_at, updated_at
                 FROM knowledge_drafts_v15_current;
                 DROP TABLE knowledge_drafts_v15_current;"
            )
            .unwrap();

        apply_migrations(&mut connection).unwrap();
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        let column_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('knowledge_drafts') WHERE name = 'accepted_knowledge_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 15);
        assert_eq!(column_count, 1);
    }

    #[test]
    fn migration_two_backfills_only_unreviewed_learning_units() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
                 CREATE TABLE sources(
                    id TEXT PRIMARY KEY,
                    selected_text TEXT NOT NULL,
                    title TEXT,
                    author TEXT
                 );
                 CREATE TABLE knowledge_units(
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL UNIQUE,
                    core_claim TEXT NOT NULL DEFAULT '',
                    concepts_json TEXT NOT NULL DEFAULT '[]',
                    prerequisites_json TEXT NOT NULL DEFAULT '[]',
                    important_details_json TEXT NOT NULL DEFAULT '[]',
                    limitations_json TEXT NOT NULL DEFAULT '[]',
                    user_note TEXT,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE review_states(
                    knowledge_unit_id TEXT PRIMARY KEY,
                    mastery_score INTEGER NOT NULL DEFAULT 0,
                    correct_count INTEGER NOT NULL DEFAULT 0,
                    wrong_count INTEGER NOT NULL DEFAULT 0,
                    review_count INTEGER NOT NULL DEFAULT 0,
                    last_reviewed_at INTEGER,
                    next_review_at INTEGER
                 );
                 CREATE TABLE evidence(
                    id TEXT PRIMARY KEY,
                    knowledge_unit_id TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    text TEXT NOT NULL,
                    start_offset INTEGER,
                    end_offset INTEGER
                 );
                 INSERT INTO schema_migrations(version, applied_at) VALUES (1, 0);
                 INSERT INTO sources(id, selected_text, title, author) VALUES
                    ('s1', 'fresh text', NULL, NULL),
                    ('s2', 'captured text', NULL, NULL),
                    ('s3', 'reviewed text', NULL, NULL);
                 INSERT INTO knowledge_units(id, source_id, status) VALUES
                    ('fresh-learning', 's1', 'learning'),
                    ('captured', 's2', 'captured'),
                    ('reviewed-learning', 's3', 'learning');
                 INSERT INTO review_states(knowledge_unit_id, mastery_score, review_count) VALUES
                    ('fresh-learning', 0, 0),
                    ('captured', 0, 0),
                    ('reviewed-learning', 0, 1);"
            )
            .unwrap();

        apply_migrations(&mut connection).unwrap();

        let fresh: i64 = connection
            .query_row("SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'fresh-learning'", [], |row| row.get(0))
            .unwrap();
        let captured: i64 = connection
            .query_row(
                "SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'captured'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let reviewed: i64 = connection
            .query_row("SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'reviewed-learning'", [], |row| row.get(0))
            .unwrap();
        let reviewed_status: String = connection
            .query_row(
                "SELECT status FROM knowledge_units WHERE id = 'reviewed-learning'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let fresh_next: Option<i64> = connection
            .query_row("SELECT next_review_at FROM review_states WHERE knowledge_unit_id = 'fresh-learning'", [], |row| row.get(0))
            .unwrap();
        let captured_next: Option<i64> = connection
            .query_row(
                "SELECT next_review_at FROM review_states WHERE knowledge_unit_id = 'captured'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(fresh, 40);
        assert_eq!(captured, 0);
        assert_eq!(reviewed, 0);
        assert_eq!(reviewed_status, "weak");
        assert!(fresh_next.is_some());
        assert!(captured_next.is_none());
        assert_eq!(version, 15);

        let inbox_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM inbox_items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(inbox_rows, 1);

        let quality_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM knowledge_quality_states", [], |row| {
                row.get(0)
            })
            .unwrap();
        let source_links: i64 = connection
            .query_row("SELECT COUNT(*) FROM knowledge_source_links", [], |row| {
                row.get(0)
            })
            .unwrap();
        let claim_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM claims", [], |row| row.get(0))
            .unwrap();
        assert_eq!(quality_rows, 3);
        assert_eq!(source_links, 3);
        assert_eq!(claim_rows, 0);

        let fts_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM knowledge_fts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fts_rows, 3);

        connection
            .execute(
                "INSERT INTO sources(id, selected_text, title, author) VALUES ('s4', 'claim text', 'Title', 'Author')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                 VALUES ('new-claim', 's4', 'Primary claim', 'captured', 10, 10)",
                [],
            )
            .unwrap();
        let quality: String = connection
            .query_row(
                "SELECT status FROM knowledge_quality_states WHERE knowledge_unit_id = 'new-claim'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let source_role: String = connection
            .query_row(
                "SELECT role FROM knowledge_source_links WHERE knowledge_unit_id = 'new-claim' AND source_id = 's4'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let claim_text: String = connection
            .query_row(
                "SELECT text FROM claims WHERE knowledge_unit_id = 'new-claim' AND claim_type = 'primary' AND retired_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(quality, "unverified");
        assert_eq!(source_role, "origin");
        assert_eq!(claim_text, "Primary claim");

        connection
            .execute(
                "INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                 VALUES ('second-claim', 's4', 'Second primary claim', 'captured', 11, 11)",
                [],
            )
            .unwrap();
        let same_source_units: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_units WHERE source_id = 's4'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let second_quality: String = connection
            .query_row(
                "SELECT status FROM knowledge_quality_states WHERE knowledge_unit_id = 'second-claim'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let second_source_role: String = connection
            .query_row(
                "SELECT role FROM knowledge_source_links WHERE knowledge_unit_id = 'second-claim' AND source_id = 's4'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let second_claim: String = connection
            .query_row(
                "SELECT text FROM claims WHERE knowledge_unit_id = 'second-claim' AND claim_type = 'primary' AND retired_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let second_fts: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_fts WHERE knowledge_unit_id = 'second-claim'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(same_source_units, 2);
        assert_eq!(second_quality, "unverified");
        assert_eq!(second_source_role, "origin");
        assert_eq!(second_claim, "Second primary claim");
        assert_eq!(second_fts, 1);

        connection
            .execute(
                "UPDATE sources SET selected_text = 'updated shared source', title = 'Updated title' WHERE id = 's4'",
                [],
            )
            .unwrap();
        let refreshed_fts: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_fts
                 WHERE knowledge_unit_id IN ('new-claim', 'second-claim')
                   AND selected_text = 'updated shared source'
                   AND title = 'Updated title'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(refreshed_fts, 2);
    }
}
