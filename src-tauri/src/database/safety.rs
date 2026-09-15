use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::backup::Backup;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use super::{apply_migrations, DatabaseState};

const CURRENT_SCHEMA_VERSION: i64 = 15;
const AUTO_BACKUP_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub file_name: String,
    pub path: String,
    pub kind: String,
    pub created_at: i64,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyStatus {
    pub database_path: String,
    pub backup_directory: String,
    pub export_directory: String,
    pub backups: Vec<BackupInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportInfo {
    pub format: String,
    pub path: String,
    pub created_at: i64,
    pub knowledge_count: usize,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub id: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub detail: Option<Value>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportSource {
    platform: String,
    url: Option<String>,
    title: Option<String>,
    author: Option<String>,
    selected_text: String,
    context_before: Option<String>,
    context_after: Option<String>,
    captured_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportSourceLink {
    source_id: String,
    role: String,
    platform: String,
    url: Option<String>,
    title: Option<String>,
    author: Option<String>,
    captured_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportClaimEvidence {
    evidence_id: String,
    stance: String,
    confidence: Option<f64>,
    text: String,
    source_id: String,
    platform: String,
    url: Option<String>,
    title: Option<String>,
    author: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportClaim {
    id: String,
    text: String,
    claim_type: String,
    evidence: Vec<ExportClaimEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportRelation {
    relation_type: String,
    direction: String,
    other_knowledge_id: String,
    other_title: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportKnowledge {
    id: String,
    core_claim: String,
    user_note: Option<String>,
    status: String,
    quality_status: String,
    archived_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
    source: ExportSource,
    sources: Vec<ExportSourceLink>,
    claims: Vec<ExportClaim>,
    topics: Vec<String>,
    tags: Vec<String>,
    relations: Vec<ExportRelation>,
    mastery_score: i64,
    review_count: i64,
    next_review_at: Option<i64>,
    stability: Option<f64>,
    difficulty: Option<f64>,
    lapse_count: i64,
    scheduled_days: Option<i64>,
    last_result: Option<String>,
    scheduler_version: Option<i64>,
}

pub(crate) fn audit_event(
    connection: &Connection,
    action: &str,
    entity_type: &str,
    entity_id: Option<&str>,
    detail: Option<Value>,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_log(id, action, entity_type, entity_id, detail_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                action,
                entity_type,
                entity_id,
                detail.map(|value| value.to_string()),
                now_ms()
            ],
        )
        .map_err(|error| format!("failed to write audit log: {error}"))?;
    Ok(())
}

pub(crate) fn create_automatic_backup_if_due(
    connection: &Connection,
    database_path: &Path,
) -> Result<Option<BackupInfo>, String> {
    let backup_dir = backup_directory(database_path)?;
    let now = now_ms();
    let latest_auto = list_backups_in(&backup_dir)?
        .into_iter()
        .filter(|item| item.kind == "automatic")
        .map(|item| item.created_at)
        .max();
    if latest_auto
        .map(|created_at| now.saturating_sub(created_at) < AUTO_BACKUP_INTERVAL_MS)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    create_backup(connection, database_path, "auto", "automatic").map(Some)
}

#[tauri::command]
pub fn database_safety_status(state: State<'_, DatabaseState>) -> Result<SafetyStatus, String> {
    let backup_dir = backup_directory(&state.path)?;
    let export_dir = export_directory(&state.path)?;
    Ok(SafetyStatus {
        database_path: state.path.to_string_lossy().into_owned(),
        backup_directory: backup_dir.to_string_lossy().into_owned(),
        export_directory: export_dir.to_string_lossy().into_owned(),
        backups: list_backups_in(&backup_dir)?,
    })
}

#[tauri::command]
pub fn database_backup_create(state: State<'_, DatabaseState>) -> Result<BackupInfo, String> {
    state.with_connection(|connection| {
        let info = create_backup(connection, &state.path, "manual", "manual")?;
        audit_event(
            connection,
            "backup.create",
            "database",
            None,
            Some(json!({ "fileName": info.file_name, "kind": info.kind })),
        )?;
        Ok(info)
    })
}

#[tauri::command]
pub fn database_backup_list(state: State<'_, DatabaseState>) -> Result<Vec<BackupInfo>, String> {
    list_backups_in(&backup_directory(&state.path)?)
}

#[tauri::command]
pub fn database_backup_restore(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    file_name: String,
) -> Result<BackupInfo, String> {
    let source_path = backup_path_from_file_name(&state.path, &file_name)?;
    validate_backup(&source_path)?;

    let recovery = state.with_connection(|connection| {
        let recovery = create_backup(connection, &state.path, "pre-restore", "pre-restore")?;
        restore_database_from_path(connection, &source_path)?;
        audit_event(
            connection,
            "backup.restore",
            "database",
            None,
            Some(json!({
                "restoredFrom": file_name,
                "recoveryBackup": recovery.file_name
            })),
        )?;
        Ok(recovery)
    })?;
    if let Err(error) = app.emit(
        "knowledge://changed",
        json!({ "reason": "database-restore" }),
    ) {
        eprintln!("database restored but knowledge refresh event failed: {error}");
    }
    Ok(recovery)
}

#[tauri::command]
pub fn knowledge_export(
    state: State<'_, DatabaseState>,
    format: String,
) -> Result<ExportInfo, String> {
    let format = format.trim().to_ascii_lowercase();
    if format != "json" && format != "markdown" {
        return Err("unsupported export format".into());
    }
    let created_at = now_ms();
    let export_dir = export_directory(&state.path)?;
    state.with_connection(|connection| {
        let knowledge = load_export_knowledge(connection)?;
        let extension = if format == "json" { "json" } else { "md" };
        let path = export_dir.join(format!("knowledge-export-{created_at}.{extension}"));
        let content = if format == "json" {
            serde_json::to_string_pretty(&json!({
                "formatVersion": 3,
                "exportedAt": created_at,
                "knowledge": knowledge
            }))
            .map_err(|error| format!("failed to serialize JSON export: {error}"))?
        } else {
            render_markdown_export(created_at, &knowledge)
        };
        fs::write(&path, content.as_bytes())
            .map_err(|error| format!("failed to write knowledge export: {error}"))?;
        let size_bytes = fs::metadata(&path)
            .map_err(|error| format!("failed to inspect knowledge export: {error}"))?
            .len();
        audit_event(
            connection,
            "knowledge.export",
            "knowledge_library",
            None,
            Some(json!({ "format": format, "count": knowledge.len() })),
        )?;
        Ok(ExportInfo {
            format,
            path: path.to_string_lossy().into_owned(),
            created_at,
            knowledge_count: knowledge.len(),
            size_bytes,
        })
    })
}

#[tauri::command]
pub fn audit_log_list(
    state: State<'_, DatabaseState>,
    limit: Option<u32>,
) -> Result<Vec<AuditRecord>, String> {
    let limit = limit.unwrap_or(100).clamp(1, 500) as i64;
    state.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT id, action, entity_type, entity_id, detail_json, created_at
                 FROM audit_log ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|error| format!("failed to prepare audit query: {error}"))?;
        let rows = statement
            .query_map([limit], |row| {
                let detail_json: Option<String> = row.get(4)?;
                Ok(AuditRecord {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    entity_type: row.get(2)?,
                    entity_id: row.get(3)?,
                    detail: detail_json.and_then(|value| serde_json::from_str(&value).ok()),
                    created_at: row.get(5)?,
                })
            })
            .map_err(|error| format!("failed to query audit log: {error}"))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|error| format!("failed to read audit row: {error}"))?);
        }
        Ok(items)
    })
}

fn create_backup(
    source: &Connection,
    database_path: &Path,
    prefix: &str,
    kind: &str,
) -> Result<BackupInfo, String> {
    let backup_dir = backup_directory(database_path)?;
    let created_at = now_ms();
    let file_name = format!("{prefix}-{created_at}.db");
    let path = backup_dir.join(&file_name);
    let mut destination = Connection::open(&path)
        .map_err(|error| format!("failed to create backup database: {error}"))?;
    let backup = Backup::new(source, &mut destination)
        .map_err(|error| format!("failed to start database backup: {error}"))?;
    backup
        .run_to_completion(128, Duration::from_millis(5), None)
        .map_err(|error| format!("failed to create database backup: {error}"))?;
    drop(backup);
    ensure_quick_check(&destination)?;
    let size_bytes = fs::metadata(&path)
        .map_err(|error| format!("failed to inspect database backup: {error}"))?
        .len();
    Ok(BackupInfo {
        file_name,
        path: path.to_string_lossy().into_owned(),
        kind: kind.to_string(),
        created_at,
        size_bytes,
    })
}

fn restore_database_from_path(
    destination: &mut Connection,
    source_path: &Path,
) -> Result<(), String> {
    let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open backup for restore: {error}"))?;
    let backup = Backup::new(&source, destination)
        .map_err(|error| format!("failed to start database restore: {error}"))?;
    backup
        .run_to_completion(128, Duration::from_millis(5), None)
        .map_err(|error| format!("failed to restore database: {error}"))?;
    drop(backup);
    apply_migrations(destination)?;
    ensure_quick_check(destination)?;
    Ok(())
}

fn validate_backup(path: &Path) -> Result<(), String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open backup: {error}"))?;
    ensure_quick_check(&connection)?;
    let version = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| format!("failed to inspect backup schema: {error}"))?
        .ok_or_else(|| "backup does not contain schema metadata".to_string())?;
    if !(1..=CURRENT_SCHEMA_VERSION).contains(&version) {
        return Err(format!("unsupported backup schema version: {version}"));
    }
    Ok(())
}

fn ensure_quick_check(connection: &Connection) -> Result<(), String> {
    let result: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("failed to validate SQLite database: {error}"))?;
    if result != "ok" {
        return Err(format!("SQLite quick_check failed: {result}"));
    }
    Ok(())
}

fn backup_directory(database_path: &Path) -> Result<PathBuf, String> {
    let root = database_path
        .parent()
        .ok_or_else(|| "database path has no parent directory".to_string())?;
    let path = root.join("backups");
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create backup directory: {error}"))?;
    Ok(path)
}

fn export_directory(database_path: &Path) -> Result<PathBuf, String> {
    let root = database_path
        .parent()
        .ok_or_else(|| "database path has no parent directory".to_string())?;
    let path = root.join("exports");
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create export directory: {error}"))?;
    Ok(path)
}

fn backup_path_from_file_name(database_path: &Path, file_name: &str) -> Result<PathBuf, String> {
    let file_name = file_name.trim();
    if file_name.is_empty()
        || file_name.contains('/')
        || file_name.contains('\\')
        || !file_name.ends_with(".db")
    {
        return Err("invalid backup file name".into());
    }
    let path = backup_directory(database_path)?.join(file_name);
    if !path.is_file() {
        return Err("backup file not found".into());
    }
    Ok(path)
}

fn list_backups_in(directory: &Path) -> Result<Vec<BackupInfo>, String> {
    let mut items = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to read backup directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to read backup entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("db") {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let created_at = timestamp_from_file_name(&file_name).unwrap_or_else(|| {
            entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(system_time_ms)
                .unwrap_or(0)
        });
        let kind = if file_name.starts_with("auto-") {
            "automatic"
        } else if file_name.starts_with("pre-restore-") {
            "pre-restore"
        } else {
            "manual"
        };
        let size_bytes = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        items.push(BackupInfo {
            file_name,
            path: path.to_string_lossy().into_owned(),
            kind: kind.to_string(),
            created_at,
            size_bytes,
        });
    }
    items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(items)
}

fn timestamp_from_file_name(file_name: &str) -> Option<i64> {
    file_name
        .strip_suffix(".db")?
        .rsplit('-')
        .next()?
        .parse::<i64>()
        .ok()
}

fn system_time_ms(value: SystemTime) -> Option<i64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

fn now_ms() -> i64 {
    system_time_ms(SystemTime::now()).unwrap_or(0)
}

fn load_export_knowledge(connection: &Connection) -> Result<Vec<ExportKnowledge>, String> {
    let mut statement = connection
        .prepare(
            "SELECT k.id, k.core_claim, k.user_note, k.status, COALESCE(q.status, 'unverified'),
                    k.archived_at, k.created_at, k.updated_at,
                    s.platform, s.url, s.title, s.author, s.selected_text, s.context_before, s.context_after, s.captured_at,
                    COALESCE(r.mastery_score, 0), COALESCE(r.review_count, 0), r.next_review_at,
                    r.stability, r.difficulty, COALESCE(r.lapse_count, 0), r.scheduled_days,
                    r.last_result, r.scheduler_version
             FROM knowledge_units k
             JOIN sources s ON s.id = k.source_id
             LEFT JOIN review_states r ON r.knowledge_unit_id = k.id
             LEFT JOIN knowledge_quality_states q ON q.knowledge_unit_id = k.id
             WHERE k.deleted_at IS NULL
             ORDER BY k.updated_at DESC",
        )
        .map_err(|error| format!("failed to prepare export query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, i64>(15)?,
                row.get::<_, i64>(16)?,
                row.get::<_, i64>(17)?,
                row.get::<_, Option<i64>>(18)?,
                row.get::<_, Option<f64>>(19)?,
                row.get::<_, Option<f64>>(20)?,
                row.get::<_, i64>(21)?,
                row.get::<_, Option<i64>>(22)?,
                row.get::<_, Option<String>>(23)?,
                row.get::<_, Option<i64>>(24)?,
            ))
        })
        .map_err(|error| format!("failed to query export data: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        let (
            id,
            core_claim,
            user_note,
            status,
            quality_status,
            archived_at,
            created_at,
            updated_at,
            platform,
            url,
            title,
            author,
            selected_text,
            context_before,
            context_after,
            captured_at,
            mastery_score,
            review_count,
            next_review_at,
            stability,
            difficulty,
            lapse_count,
            scheduled_days,
            last_result,
            scheduler_version,
        ) = row.map_err(|error| format!("failed to read export row: {error}"))?;
        items.push(ExportKnowledge {
            topics: query_names(connection, "topics", "knowledge_topics", "topic_id", &id)?,
            tags: query_names(connection, "tags", "knowledge_tags", "tag_id", &id)?,
            relations: query_relations(connection, &id)?,
            sources: query_export_sources(connection, &id)?,
            claims: query_export_claims(connection, &id)?,
            id,
            core_claim,
            user_note,
            status,
            quality_status,
            archived_at,
            created_at,
            updated_at,
            source: ExportSource {
                platform,
                url,
                title,
                author,
                selected_text,
                context_before,
                context_after,
                captured_at,
            },
            mastery_score,
            review_count,
            next_review_at,
            stability,
            difficulty,
            lapse_count,
            scheduled_days,
            last_result,
            scheduler_version,
        });
    }
    Ok(items)
}

fn query_export_sources(
    connection: &Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<ExportSourceLink>, String> {
    let mut statement = connection
        .prepare(
            "SELECT l.source_id, l.role, s.platform, s.url, s.title, s.author, s.captured_at
             FROM knowledge_source_links l
             JOIN sources s ON s.id = l.source_id
             WHERE l.knowledge_unit_id = ?1
             ORDER BY CASE l.role WHEN 'origin' THEN 0 WHEN 'supporting' THEN 1 ELSE 2 END,
                      l.created_at ASC, s.captured_at ASC",
        )
        .map_err(|error| format!("failed to prepare export source query: {error}"))?;
    let rows = statement
        .query_map([knowledge_unit_id], |row| {
            Ok(ExportSourceLink {
                source_id: row.get(0)?,
                role: row.get(1)?,
                platform: row.get(2)?,
                url: row.get(3)?,
                title: row.get(4)?,
                author: row.get(5)?,
                captured_at: row.get(6)?,
            })
        })
        .map_err(|error| format!("failed to query export sources: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read export source: {error}"))
}

fn query_export_claims(
    connection: &Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<ExportClaim>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, text, claim_type
             FROM claims
             WHERE knowledge_unit_id = ?1 AND retired_at IS NULL
             ORDER BY CASE claim_type WHEN 'primary' THEN 0 ELSE 1 END, created_at ASC, rowid ASC",
        )
        .map_err(|error| format!("failed to prepare export claim query: {error}"))?;
    let rows = statement
        .query_map([knowledge_unit_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("failed to query export claims: {error}"))?;
    let mut claims = Vec::new();
    for row in rows {
        let (id, text, claim_type) =
            row.map_err(|error| format!("failed to read export claim: {error}"))?;
        claims.push(ExportClaim {
            evidence: query_export_claim_evidence(connection, &id)?,
            id,
            text,
            claim_type,
        });
    }
    Ok(claims)
}

fn query_export_claim_evidence(
    connection: &Connection,
    claim_id: &str,
) -> Result<Vec<ExportClaimEvidence>, String> {
    let mut statement = connection
        .prepare(
            "SELECT e.id, ce.stance, ce.confidence, e.text, e.source_id,
                    s.platform, s.url, s.title, s.author
             FROM claim_evidence ce
             JOIN evidence e ON e.id = ce.evidence_id
             JOIN sources s ON s.id = e.source_id
             WHERE ce.claim_id = ?1
             ORDER BY CASE ce.stance WHEN 'supports' THEN 0 ELSE 1 END, ce.created_at ASC, e.rowid ASC",
        )
        .map_err(|error| format!("failed to prepare export claim evidence query: {error}"))?;
    let rows = statement
        .query_map([claim_id], |row| {
            Ok(ExportClaimEvidence {
                evidence_id: row.get(0)?,
                stance: row.get(1)?,
                confidence: row.get(2)?,
                text: row.get(3)?,
                source_id: row.get(4)?,
                platform: row.get(5)?,
                url: row.get(6)?,
                title: row.get(7)?,
                author: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query export claim evidence: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read export claim evidence: {error}"))
}

fn query_names(
    connection: &Connection,
    table: &str,
    join_table: &str,
    join_column: &str,
    knowledge_unit_id: &str,
) -> Result<Vec<String>, String> {
    let sql = format!(
        "SELECT item.name FROM {table} item JOIN {join_table} link ON link.{join_column} = item.id
         WHERE link.knowledge_unit_id = ?1 ORDER BY item.name COLLATE NOCASE"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("failed to prepare export name query: {error}"))?;
    let rows = statement
        .query_map([knowledge_unit_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query export names: {error}"))?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("failed to read export name: {error}"))?);
    }
    Ok(items)
}

fn query_relations(
    connection: &Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<ExportRelation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT kr.relation_type,
                    CASE WHEN kr.source_knowledge_id = ?1 THEN 'outgoing' ELSE 'incoming' END,
                    CASE WHEN kr.source_knowledge_id = ?1 THEN kr.target_knowledge_id ELSE kr.source_knowledge_id END,
                    COALESCE(NULLIF(other.core_claim, ''), substr(s.selected_text, 1, 160))
             FROM knowledge_relations kr
             JOIN knowledge_units other ON other.id = CASE
                WHEN kr.source_knowledge_id = ?1 THEN kr.target_knowledge_id ELSE kr.source_knowledge_id END
             JOIN sources s ON s.id = other.source_id
             WHERE kr.source_knowledge_id = ?1 OR kr.target_knowledge_id = ?1
             ORDER BY kr.created_at ASC",
        )
        .map_err(|error| format!("failed to prepare export relation query: {error}"))?;
    let rows = statement
        .query_map([knowledge_unit_id], |row| {
            Ok(ExportRelation {
                relation_type: row.get(0)?,
                direction: row.get(1)?,
                other_knowledge_id: row.get(2)?,
                other_title: row.get(3)?,
            })
        })
        .map_err(|error| format!("failed to query export relations: {error}"))?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("failed to read export relation: {error}"))?);
    }
    Ok(items)
}

fn render_markdown_export(exported_at: i64, knowledge: &[ExportKnowledge]) -> String {
    let mut output = format!("# ZhiForge Knowledge Export\n\nExported at: {exported_at}\n\n");
    for item in knowledge {
        let title = if item.core_claim.trim().is_empty() {
            item.source
                .selected_text
                .lines()
                .next()
                .unwrap_or("Knowledge")
        } else {
            item.core_claim.trim()
        };
        output.push_str(&format!("## {}\n\n", markdown_text(title)));
        output.push_str(&format!("- Learning status: {}\n", item.status));
        output.push_str(&format!("- Knowledge quality: {}\n", item.quality_status));
        output.push_str(&format!("- Mastery: {}\n", item.mastery_score));
        output.push_str(&format!("- Reviews: {}\n", item.review_count));
        if let (Some(stability), Some(difficulty), Some(scheduled_days), Some(version)) = (
            item.stability,
            item.difficulty,
            item.scheduled_days,
            item.scheduler_version,
        ) {
            output.push_str(&format!(
                "- Scheduler: stability {:.3} days · difficulty {:.3} · lapses {} · interval {} days · v{}\n",
                stability, difficulty, item.lapse_count, scheduled_days, version
            ));
        }
        if let Some(last_result) = &item.last_result {
            output.push_str(&format!("- Last review result: {}\n", last_result));
        }
        output.push_str(&format!("- Sources: {}\n", item.sources.len()));
        if !item.topics.is_empty() {
            output.push_str(&format!("- Topics: {}\n", item.topics.join(", ")));
        }
        if !item.tags.is_empty() {
            output.push_str(&format!("- Tags: {}\n", item.tags.join(", ")));
        }
        if let Some(author) = &item.source.author {
            output.push_str(&format!("- Author: {}\n", markdown_text(author)));
        }
        if let Some(url) = &item.source.url {
            output.push_str(&format!("- Source: {}\n", url));
        }
        output.push_str("\n### Source excerpt\n\n");
        output.push_str(&item.source.selected_text);
        output.push_str("\n\n");
        if let Some(note) = &item.user_note {
            output.push_str("### My understanding\n\n");
            output.push_str(note);
            output.push_str("\n\n");
        }
        if !item.claims.is_empty() {
            output.push_str("### Claims and evidence\n\n");
            for claim in &item.claims {
                output.push_str(&format!(
                    "- [{}] {}\n",
                    claim.claim_type,
                    markdown_text(&claim.text)
                ));
                for evidence in &claim.evidence {
                    let source = evidence
                        .author
                        .as_deref()
                        .or(evidence.title.as_deref())
                        .unwrap_or(evidence.platform.as_str());
                    output.push_str(&format!(
                        "  - {} / {}: {}\n",
                        evidence.stance,
                        markdown_text(source),
                        markdown_text(&evidence.text)
                    ));
                }
            }
            output.push('\n');
        }
        if !item.relations.is_empty() {
            output.push_str("### Relations\n\n");
            for relation in &item.relations {
                output.push_str(&format!(
                    "- {} / {} → {}\n",
                    relation.direction,
                    relation.relation_type,
                    markdown_text(&relation.other_title)
                ));
            }
            output.push('\n');
        }
    }
    output
}

fn markdown_text(value: &str) -> String {
    value.replace('\n', " ").replace('\r', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_file_name_timestamp_is_parsed() {
        assert_eq!(timestamp_from_file_name("auto-12345.db"), Some(12345));
        assert_eq!(
            timestamp_from_file_name("pre-restore-98765.db"),
            Some(98765)
        );
        assert_eq!(timestamp_from_file_name("not-a-backup.txt"), None);
    }

    #[test]
    fn backup_file_name_rejects_path_escape() {
        let base = PathBuf::from("C:/tmp/knowledge.db");
        assert!(backup_path_from_file_name(&base, "../knowledge.db").is_err());
        assert!(backup_path_from_file_name(&base, "folder\\knowledge.db").is_err());
    }

    #[test]
    fn portable_export_preserves_quality_claim_evidence_and_sources() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::database::apply_migrations(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO sources(
                    id, platform, url, title, author, selected_text, context_before, context_after,
                    application, window_title, content_hash, captured_at
                 ) VALUES (
                    'source-1', 'zhihu', 'https://www.zhihu.com/question/1/answer/2', 'Question', 'Alice',
                    'source excerpt', NULL, NULL, NULL, NULL, 'hash-1', 10
                 )",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                 VALUES ('knowledge-1', 'source-1', 'Primary claim', 'learning', 10, 10)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO evidence(id, knowledge_unit_id, source_id, text, start_offset, end_offset)
                 VALUES ('evidence-1', 'knowledge-1', 'source-1', 'conflicting quote', 0, 17)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO claim_evidence(claim_id, evidence_id, stance, confidence, created_by, created_at)
                 VALUES ('primary:knowledge-1', 'evidence-1', 'conflicts', 0.9, 'test', 11)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE knowledge_quality_states
                 SET status = 'conflicted', reason = 'conflicting evidence', updated_by = 'test', updated_at = 11
                 WHERE knowledge_unit_id = 'knowledge-1'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO review_states(
                    knowledge_unit_id, mastery_score, review_count, next_review_at,
                    stability, difficulty, lapse_count, scheduled_days, last_result, scheduler_version
                 ) VALUES ('knowledge-1', 55, 3, 1234, 2.5, 6.25, 1, 2, 'partial', 1)",
                [],
            )
            .unwrap();

        let exported = load_export_knowledge(&connection).unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].quality_status, "conflicted");
        assert_eq!(exported[0].sources.len(), 1);
        assert_eq!(exported[0].sources[0].role, "origin");
        assert_eq!(exported[0].claims.len(), 1);
        assert_eq!(exported[0].claims[0].text, "Primary claim");
        assert_eq!(exported[0].claims[0].evidence.len(), 1);
        assert_eq!(exported[0].claims[0].evidence[0].stance, "conflicts");
        assert_eq!(exported[0].stability, Some(2.5));
        assert_eq!(exported[0].difficulty, Some(6.25));
        assert_eq!(exported[0].lapse_count, 1);
        assert_eq!(exported[0].scheduled_days, Some(2));
        assert_eq!(exported[0].last_result.as_deref(), Some("partial"));
        assert_eq!(exported[0].scheduler_version, Some(1));

        let markdown = render_markdown_export(12, &exported);
        assert!(markdown.contains("Knowledge quality: conflicted"));
        assert!(markdown.contains(
            "Scheduler: stability 2.500 days · difficulty 6.250 · lapses 1 · interval 2 days · v1"
        ));
        assert!(markdown.contains("Last review result: partial"));
        assert!(markdown.contains("conflicts / Alice: conflicting quote"));
    }

    #[test]
    fn online_backup_preserves_backup_point_data() {
        let root = std::env::temp_dir().join(format!("zhiforge-backup-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database_path = root.join("knowledge.db");
        let source = Connection::open(&database_path).unwrap();
        source
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
                 INSERT INTO schema_migrations(version, applied_at) VALUES (5, 0);
                 CREATE TABLE sample(value TEXT NOT NULL);
                 INSERT INTO sample(value) VALUES ('before');",
            )
            .unwrap();

        let backup = create_backup(&source, &database_path, "manual", "manual").unwrap();
        source
            .execute("UPDATE sample SET value = 'after'", [])
            .unwrap();

        let snapshot =
            Connection::open_with_flags(&backup.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let value: String = snapshot
            .query_row("SELECT value FROM sample", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "before");
        validate_backup(Path::new(&backup.path)).unwrap();

        drop(snapshot);
        drop(source);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn restore_round_trip_returns_live_database_to_backup_point() {
        let root = std::env::temp_dir().join(format!("zhiforge-restore-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database_path = root.join("knowledge.db");
        let mut live = Connection::open(&database_path).unwrap();
        apply_migrations(&mut live).unwrap();
        live.execute_batch(
            "CREATE TABLE sample(value TEXT NOT NULL);
             INSERT INTO sample(value) VALUES ('backup-point');",
        )
        .unwrap();

        let backup = create_backup(&live, &database_path, "manual", "manual").unwrap();
        live.execute("UPDATE sample SET value = 'mutated'", [])
            .unwrap();
        restore_database_from_path(&mut live, Path::new(&backup.path)).unwrap();

        let restored: String = live
            .query_row("SELECT value FROM sample", [], |row| row.get(0))
            .unwrap();
        assert_eq!(restored, "backup-point");
        ensure_quick_check(&live).unwrap();

        drop(live);
        fs::remove_dir_all(&root).unwrap();
    }
}
