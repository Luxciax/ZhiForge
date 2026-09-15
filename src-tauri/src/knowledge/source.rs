use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::database::DatabaseState;

use super::internalization::SourceKnowledgeUnitSummary;
use super::SourceRecord;

const SOURCE_PLATFORMS: &[&str] = &["zhihu", "web", "pdf", "desktop", "unknown"];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLibraryItem {
    pub id: String,
    pub platform: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub selected_text: String,
    pub captured_at: i64,
    pub knowledge_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDetailRecord {
    pub source: SourceRecord,
    pub knowledge_units: Vec<SourceKnowledgeUnitSummary>,
}

#[tauri::command]
pub fn source_library_list(
    state: State<'_, DatabaseState>,
    query: Option<String>,
    platform: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<SourceLibraryItem>, String> {
    let query = clean_optional(query);
    let platform = clean_optional(platform);
    if let Some(value) = platform.as_deref() {
        if !SOURCE_PLATFORMS.contains(&value) {
            return Err("unsupported source platform".into());
        }
    }
    let limit = limit.unwrap_or(100).clamp(1, 200) as i64;
    state.with_connection(|connection| {
        list_source_library(connection, query.as_deref(), platform.as_deref(), limit)
    })
}

#[tauri::command]
pub fn source_get_detail(
    state: State<'_, DatabaseState>,
    source_id: String,
) -> Result<SourceDetailRecord, String> {
    let source_id = source_id.trim();
    if source_id.is_empty() {
        return Err("source id is empty".into());
    }
    state.with_connection(|connection| load_source_detail(connection, source_id))
}

fn list_source_library(
    connection: &rusqlite::Connection,
    query: Option<&str>,
    platform: Option<&str>,
    limit: i64,
) -> Result<Vec<SourceLibraryItem>, String> {
    let like = query.map(|value| format!("%{}%", escape_like(value)));
    let mut statement = connection
        .prepare(
            "SELECT s.id, s.platform, s.url, s.title, s.author, s.selected_text, s.captured_at,
                    COUNT(DISTINCT k.id)
             FROM sources s
             LEFT JOIN knowledge_source_links l ON l.source_id = s.id
             LEFT JOIN knowledge_units k ON k.id = l.knowledge_unit_id
                AND k.deleted_at IS NULL
                AND k.status <> 'captured'
             WHERE (?1 IS NULL OR s.platform = ?1)
               AND (
                    ?2 IS NULL
                    OR lower(COALESCE(s.title, '')) LIKE lower(?2) ESCAPE '\\'
                    OR lower(COALESCE(s.author, '')) LIKE lower(?2) ESCAPE '\\'
                    OR lower(COALESCE(s.url, '')) LIKE lower(?2) ESCAPE '\\'
                    OR lower(s.selected_text) LIKE lower(?2) ESCAPE '\\'
               )
             GROUP BY s.id, s.platform, s.url, s.title, s.author, s.selected_text, s.captured_at
             ORDER BY s.captured_at DESC, s.rowid DESC
             LIMIT ?3",
        )
        .map_err(|error| format!("failed to prepare source library query: {error}"))?;
    let rows = statement
        .query_map(params![platform, like, limit], |row| {
            Ok(SourceLibraryItem {
                id: row.get(0)?,
                platform: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                author: row.get(4)?,
                selected_text: row.get(5)?,
                captured_at: row.get(6)?,
                knowledge_count: row.get(7)?,
            })
        })
        .map_err(|error| format!("failed to query source library: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read source library: {error}"))?;
    Ok(rows)
}

fn load_source_detail(
    connection: &rusqlite::Connection,
    source_id: &str,
) -> Result<SourceDetailRecord, String> {
    let source = connection
        .query_row(
            "SELECT s.id, s.platform, s.url, s.title, s.author, s.selected_text,
                    s.context_before, s.context_after, s.application, s.window_title,
                    s.content_hash, s.captured_at
             FROM sources s
             WHERE s.id = ?1",
            [source_id],
            |row| {
                Ok(SourceRecord {
                    id: row.get(0)?,
                    platform: row.get(1)?,
                    url: row.get(2)?,
                    title: row.get(3)?,
                    author: row.get(4)?,
                    selected_text: row.get(5)?,
                    context_before: row.get(6)?,
                    context_after: row.get(7)?,
                    application: row.get(8)?,
                    window_title: row.get(9)?,
                    content_hash: row.get(10)?,
                    captured_at: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to load source detail: {error}"))?
        .ok_or_else(|| "source not found".to_string())?;
    let knowledge_units = load_linked_source_knowledge_units(connection, source_id)?;
    Ok(SourceDetailRecord {
        source,
        knowledge_units,
    })
}

fn load_linked_source_knowledge_units(
    connection: &rusqlite::Connection,
    source_id: &str,
) -> Result<Vec<SourceKnowledgeUnitSummary>, String> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT k.id, k.core_claim, k.status,
                    COALESCE(r.mastery_score, 0), COALESCE(r.review_count, 0), k.archived_at,
                    CASE l.role WHEN 'origin' THEN 0 WHEN 'supporting' THEN 1 ELSE 2 END AS role_order
             FROM knowledge_source_links l
             JOIN knowledge_units k ON k.id = l.knowledge_unit_id
             LEFT JOIN review_states r ON r.knowledge_unit_id = k.id
             WHERE l.source_id = ?1
               AND k.deleted_at IS NULL
               AND k.status <> 'captured'
             ORDER BY role_order ASC, k.created_at ASC, k.rowid ASC",
        )
        .map_err(|error| format!("failed to prepare linked source knowledge query: {error}"))?;
    let rows = statement
        .query_map([source_id], |row| {
            Ok(SourceKnowledgeUnitSummary {
                id: row.get(0)?,
                core_claim: row.get(1)?,
                status: row.get(2)?,
                mastery_score: row.get(3)?,
                review_count: row.get(4)?,
                archived_at: row.get(5)?,
            })
        })
        .map_err(|error| format!("failed to query linked source knowledge: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read linked source knowledge: {error}"))?;
    Ok(rows)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::apply_migrations;
    use rusqlite::Connection;

    #[test]
    fn source_library_groups_sibling_knowledge_units() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO sources(
                    id, platform, url, title, author, selected_text, content_hash, captured_at
                 ) VALUES (
                    's1', 'zhihu', 'https://www.zhihu.com/question/1/answer/2',
                    'Shared answer', 'Author', 'alpha evidence and beta evidence', 'hash-source-1', 100
                 )",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at) VALUES
                    ('k1', 's1', 'Alpha claim', 'learning', 101, 101),
                    ('k2', 's1', 'Beta claim', 'reviewing', 102, 102)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO review_states(knowledge_unit_id, mastery_score, review_count) VALUES
                    ('k1', 40, 0),
                    ('k2', 75, 2)",
                [],
            )
            .unwrap();

        let items = list_source_library(&connection, Some("Author"), Some("zhihu"), 20).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "s1");
        assert_eq!(items[0].knowledge_count, 2);

        let detail = load_source_detail(&connection, "s1").unwrap();
        assert_eq!(detail.source.title.as_deref(), Some("Shared answer"));
        assert_eq!(detail.knowledge_units.len(), 2);
        assert_eq!(detail.knowledge_units[0].core_claim, "Alpha claim");
        assert_eq!(detail.knowledge_units[1].mastery_score, 75);
    }

    #[test]
    fn source_library_counts_supporting_linked_knowledge() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at) VALUES
                    ('s-origin', 'web', 'Origin', 'origin evidence', 'source-origin-hash', 10),
                    ('s-support', 'zhihu', 'Supporting answer', 'supporting evidence', 'source-support-hash', 20);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                    VALUES ('k1', 's-origin', 'Shared claim', 'learning', 30, 30);
                 INSERT INTO review_states(knowledge_unit_id, mastery_score, review_count)
                    VALUES ('k1', 55, 1);
                 INSERT INTO knowledge_source_links(knowledge_unit_id, source_id, role, created_by, created_at)
                    VALUES ('k1', 's-support', 'supporting', 'test', 40);"
            )
            .unwrap();

        let items = list_source_library(&connection, Some("Supporting"), Some("zhihu"), 20).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "s-support");
        assert_eq!(items[0].knowledge_count, 1);

        let detail = load_source_detail(&connection, "s-support").unwrap();
        assert_eq!(detail.knowledge_units.len(), 1);
        assert_eq!(detail.knowledge_units[0].id, "k1");
        assert_eq!(detail.knowledge_units[0].core_claim, "Shared claim");
    }

    #[test]
    fn source_library_like_search_treats_wildcards_literally() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply_migrations(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO sources(id, platform, title, selected_text, content_hash, captured_at)
                 VALUES ('s1', 'web', '50%_done', 'literal wildcard source', 'hash-source-2', 100)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                 VALUES ('k1', 's1', 'Literal', 'captured', 101, 101)",
                [],
            )
            .unwrap();

        let items = list_source_library(&connection, Some("50%_done"), None, 20).unwrap();
        assert_eq!(items.len(), 1);
    }
}
