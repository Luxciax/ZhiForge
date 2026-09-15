use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use tauri::State;
use uuid::Uuid;

use crate::database::safety::audit_event;
use crate::database::DatabaseState;

use super::now_ms;

const TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const RELATION_TYPES: &[&str] = &[
    "related_to",
    "supports",
    "contradicts",
    "example_of",
    "prerequisite_of",
    "derived_from",
    "extends",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub locked: bool,
    pub knowledge_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub knowledge_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRelationRecord {
    pub id: String,
    pub source_knowledge_id: String,
    pub target_knowledge_id: String,
    pub relation_type: String,
    pub confidence: Option<f64>,
    pub created_by: String,
    pub confirmed: bool,
    pub created_at: i64,
    pub other_knowledge_id: String,
    pub other_title: String,
    pub direction: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryItem {
    pub id: String,
    pub source_id: String,
    pub core_claim: String,
    pub selected_text: String,
    pub user_note: Option<String>,
    pub status: String,
    pub quality_status: String,
    pub platform: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub captured_at: i64,
    pub updated_at: i64,
    pub mastery_score: i64,
    pub review_count: i64,
    pub archived_at: Option<i64>,
    pub topics: Vec<TopicRecord>,
    pub tags: Vec<TagRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashKnowledgeItem {
    pub knowledge_unit_id: String,
    pub core_claim: String,
    pub selected_text: String,
    pub platform: String,
    pub title: Option<String>,
    pub deleted_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeManagementDetail {
    pub topics: Vec<TopicRecord>,
    pub tags: Vec<TagRecord>,
    pub relations: Vec<KnowledgeRelationRecord>,
    pub archived_at: Option<i64>,
}

#[tauri::command]
pub fn knowledge_management_detail(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
) -> Result<KnowledgeManagementDetail, String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    state.with_connection(|connection| {
        let archived_at = connection
            .query_row(
                "SELECT archived_at FROM knowledge_units WHERE id = ?1 AND deleted_at IS NULL",
                [&knowledge_unit_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(|error| format!("failed to load knowledge management state: {error}"))?
            .ok_or_else(|| "knowledge unit not found".to_string())?;
        Ok(KnowledgeManagementDetail {
            topics: list_topics_for(connection, &knowledge_unit_id)?,
            tags: list_tags_for(connection, &knowledge_unit_id)?,
            relations: list_relations_for(connection, &knowledge_unit_id)?,
            archived_at,
        })
    })
}

#[tauri::command]
pub fn knowledge_library_list(
    state: State<'_, DatabaseState>,
    query: Option<String>,
    status: Option<String>,
    quality_status: Option<String>,
    topic_id: Option<String>,
    tag_id: Option<String>,
    include_archived: Option<bool>,
    limit: Option<u32>,
) -> Result<Vec<KnowledgeLibraryItem>, String> {
    let query = clean_optional(query);
    let status = clean_optional(status);
    let quality_status = clean_optional(quality_status);
    let topic_id = clean_optional(topic_id);
    let tag_id = clean_optional(tag_id);
    let include_archived = include_archived.unwrap_or(false);
    let limit = limit.unwrap_or(100).clamp(1, 200) as i64;
    let like = query
        .as_ref()
        .map(|value| format!("%{}%", escape_like(value)));
    let fts = query.as_ref().map(|value| fts_expression(value));

    state.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT
                    k.id, k.source_id, k.core_claim, s.selected_text, k.user_note, k.status,
                    COALESCE(q.status, 'unverified'),
                    s.platform, s.title, s.author, s.captured_at, k.updated_at,
                    COALESCE(r.mastery_score, 0), COALESCE(r.review_count, 0), k.archived_at
                 FROM knowledge_units k
                 JOIN sources s ON s.id = k.source_id
                 LEFT JOIN review_states r ON r.knowledge_unit_id = k.id
                 LEFT JOIN knowledge_quality_states q ON q.knowledge_unit_id = k.id
                 WHERE k.deleted_at IS NULL
                   AND k.status <> 'captured'
                   AND (?1 IS NULL OR k.status = ?1)
                   AND (?2 IS NULL OR COALESCE(q.status, 'unverified') = ?2)
                   AND (?3 IS NULL OR EXISTS(
                       SELECT 1 FROM knowledge_topics kt WHERE kt.knowledge_unit_id = k.id AND kt.topic_id = ?3
                   ))
                   AND (?4 IS NULL OR EXISTS(
                       SELECT 1 FROM knowledge_tags kg WHERE kg.knowledge_unit_id = k.id AND kg.tag_id = ?4
                   ))
                   AND (?5 = 1 OR k.archived_at IS NULL)
                   AND (
                       ?6 IS NULL
                       OR lower(k.core_claim) LIKE lower(?6) ESCAPE '\\'
                       OR lower(COALESCE(k.user_note, '')) LIKE lower(?6) ESCAPE '\\'
                       OR lower(s.selected_text) LIKE lower(?6) ESCAPE '\\'
                       OR lower(COALESCE(s.title, '')) LIKE lower(?6) ESCAPE '\\'
                       OR lower(COALESCE(s.author, '')) LIKE lower(?6) ESCAPE '\\'
                       OR k.id IN (
                           SELECT knowledge_unit_id FROM knowledge_fts
                           WHERE knowledge_fts MATCH ?7
                       )
                   )
                 ORDER BY k.updated_at DESC
                 LIMIT ?8",
            )
            .map_err(|error| format!("failed to prepare knowledge library query: {error}"))?;

        let rows = statement
            .query_map(
                params![
                    status,
                    quality_status,
                    topic_id,
                    tag_id,
                    if include_archived { 1 } else { 0 },
                    like,
                    fts,
                    limit,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                        row.get::<_, i64>(13)?,
                        row.get::<_, Option<i64>>(14)?,
                    ))
                },
            )
            .map_err(|error| format!("failed to query knowledge library: {error}"))?;

        let mut items = Vec::new();
        for row in rows {
            let (
                id,
                source_id,
                core_claim,
                selected_text,
                user_note,
                status,
                quality_status,
                platform,
                title,
                author,
                captured_at,
                updated_at,
                mastery_score,
                review_count,
                archived_at,
            ) = row.map_err(|error| format!("failed to read knowledge library row: {error}"))?;
            items.push(KnowledgeLibraryItem {
                topics: list_topics_for(connection, &id)?,
                tags: list_tags_for(connection, &id)?,
                id,
                source_id,
                core_claim,
                selected_text,
                user_note,
                status,
                quality_status,
                platform,
                title,
                author,
                captured_at,
                updated_at,
                mastery_score,
                review_count,
                archived_at,
            });
        }
        Ok(items)
    })
}
#[tauri::command]
pub fn knowledge_note_update(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
    note: String,
) -> Result<Option<String>, String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    let note = note.trim().to_string();
    if note.chars().count() > 100_000 {
        return Err("note is too large".into());
    }
    let stored = if note.is_empty() { None } else { Some(note) };
    let now = now_ms();
    state.with_connection(|connection| {
        let changed = connection
            .execute(
                "UPDATE knowledge_units SET user_note = ?2, updated_at = ?3
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![knowledge_unit_id, stored, now],
            )
            .map_err(|error| format!("failed to update knowledge note: {error}"))?;
        if changed == 0 {
            return Err("knowledge unit not found".into());
        }
        audit_event(
            connection,
            "knowledge.note.update",
            "knowledge",
            Some(&knowledge_unit_id),
            Some(json!({ "hasNote": stored.is_some(), "length": stored.as_ref().map(|value| value.chars().count()).unwrap_or(0) })),
        )?;
        Ok(stored)
    })
}

#[tauri::command]
pub fn topic_create(
    state: State<'_, DatabaseState>,
    name: String,
    description: Option<String>,
) -> Result<TopicRecord, String> {
    let name = normalized_name(&name, "topic", 120)?;
    let description = description
        .unwrap_or_default()
        .trim()
        .chars()
        .take(2000)
        .collect::<String>();
    let now = now_ms();
    let id = Uuid::new_v4().to_string();
    state.with_connection(|connection| {
        connection
            .execute(
                "INSERT INTO topics(id, name, description, created_by, locked, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'user', 0, ?4, ?4)",
                params![id, name, description, now],
            )
            .map_err(|error| format!("failed to create topic: {error}"))?;
        let topic = load_topic(connection, &id)?
            .ok_or_else(|| "topic was created but could not be reloaded".to_string())?;
        audit_event(
            connection,
            "topic.create",
            "topic",
            Some(&id),
            Some(json!({ "name": topic.name })),
        )?;
        Ok(topic)
    })
}

#[tauri::command]
pub fn topic_list(
    state: State<'_, DatabaseState>,
    include_archived: Option<bool>,
) -> Result<Vec<TopicRecord>, String> {
    state.with_connection(|connection| list_topics(connection, include_archived.unwrap_or(false)))
}

#[tauri::command]
pub fn topic_update(
    state: State<'_, DatabaseState>,
    topic_id: String,
    name: String,
    description: Option<String>,
) -> Result<TopicRecord, String> {
    let topic_id = required_id(&topic_id, "topic")?;
    let name = normalized_name(&name, "topic", 120)?;
    let description = description
        .unwrap_or_default()
        .trim()
        .chars()
        .take(2000)
        .collect::<String>();
    state.with_connection(|connection| {
        update_topic_connection(connection, &topic_id, &name, &description)
    })
}

#[tauri::command]
pub fn topic_set_archived(
    state: State<'_, DatabaseState>,
    topic_id: String,
    archived: bool,
) -> Result<TopicRecord, String> {
    let topic_id = required_id(&topic_id, "topic")?;
    state.with_connection(|connection| set_topic_archived_connection(connection, &topic_id, archived))
}

#[tauri::command]
pub fn topic_assign(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
    topic_id: String,
) -> Result<(), String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    let topic_id = required_id(&topic_id, "topic")?;
    state.with_connection(|connection| {
        ensure_active_knowledge(connection, &knowledge_unit_id)?;
        ensure_active_topic(connection, &topic_id)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO knowledge_topics(knowledge_unit_id, topic_id, created_by, created_at)
                 VALUES (?1, ?2, 'user', ?3)",
                params![knowledge_unit_id, topic_id, now_ms()],
            )
            .map_err(|error| format!("failed to assign topic: {error}"))?;
        audit_event(
            connection,
            "knowledge.topic.assign",
            "knowledge",
            Some(&knowledge_unit_id),
            Some(json!({ "topicId": topic_id })),
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn topic_unassign(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
    topic_id: String,
) -> Result<(), String> {
    state.with_connection(|connection| {
        connection
            .execute(
                "DELETE FROM knowledge_topics WHERE knowledge_unit_id = ?1 AND topic_id = ?2",
                params![knowledge_unit_id.trim(), topic_id.trim()],
            )
            .map_err(|error| format!("failed to remove topic assignment: {error}"))?;
        audit_event(
            connection,
            "knowledge.topic.unassign",
            "knowledge",
            Some(knowledge_unit_id.trim()),
            Some(json!({ "topicId": topic_id.trim() })),
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn tag_add(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
    name: String,
) -> Result<TagRecord, String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    let name = normalized_name(&name, "tag", 80)?;
    let now = now_ms();
    state.with_connection(|connection| {
        ensure_active_knowledge(connection, &knowledge_unit_id)?;
        let tag_id = connection
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                [&name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("failed to look up tag: {error}"))?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        connection
            .execute(
                "INSERT OR IGNORE INTO tags(id, name, created_by, created_at, updated_at)
                 VALUES (?1, ?2, 'user', ?3, ?3)",
                params![tag_id, name, now],
            )
            .map_err(|error| format!("failed to create tag: {error}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO knowledge_tags(knowledge_unit_id, tag_id, created_by, created_at)
                 VALUES (?1, ?2, 'user', ?3)",
                params![knowledge_unit_id, tag_id, now],
            )
            .map_err(|error| format!("failed to assign tag: {error}"))?;
        let tag = load_tag(connection, &tag_id)?
            .ok_or_else(|| "tag could not be reloaded".to_string())?;
        audit_event(
            connection,
            "knowledge.tag.add",
            "knowledge",
            Some(&knowledge_unit_id),
            Some(json!({ "tagId": tag_id, "name": tag.name })),
        )?;
        Ok(tag)
    })
}

#[tauri::command]
pub fn tag_remove(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
    tag_id: String,
) -> Result<(), String> {
    state.with_connection(|connection| {
        connection
            .execute(
                "DELETE FROM knowledge_tags WHERE knowledge_unit_id = ?1 AND tag_id = ?2",
                params![knowledge_unit_id.trim(), tag_id.trim()],
            )
            .map_err(|error| format!("failed to remove tag: {error}"))?;
        audit_event(
            connection,
            "knowledge.tag.remove",
            "knowledge",
            Some(knowledge_unit_id.trim()),
            Some(json!({ "tagId": tag_id.trim() })),
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn tag_list(state: State<'_, DatabaseState>) -> Result<Vec<TagRecord>, String> {
    state.with_connection(|connection| list_tags(connection))
}

#[tauri::command]
pub fn tag_update(
    state: State<'_, DatabaseState>,
    tag_id: String,
    name: String,
) -> Result<TagRecord, String> {
    let tag_id = required_id(&tag_id, "tag")?;
    let name = normalized_name(&name, "tag", 80)?;
    state.with_connection(|connection| update_tag_connection(connection, &tag_id, &name))
}

#[tauri::command]
pub fn tag_delete_empty(
    state: State<'_, DatabaseState>,
    tag_id: String,
) -> Result<(), String> {
    let tag_id = required_id(&tag_id, "tag")?;
    state.with_connection(|connection| delete_empty_tag_connection(connection, &tag_id))
}

#[tauri::command]
pub fn knowledge_relation_create(
    state: State<'_, DatabaseState>,
    source_knowledge_id: String,
    target_knowledge_id: String,
    relation_type: String,
) -> Result<KnowledgeRelationRecord, String> {
    let source = required_id(&source_knowledge_id, "source knowledge")?;
    let target = required_id(&target_knowledge_id, "target knowledge")?;
    if source == target {
        return Err("a knowledge unit cannot relate to itself".into());
    }
    let relation_type = relation_type.trim().to_ascii_lowercase();
    if !RELATION_TYPES.contains(&relation_type.as_str()) {
        return Err("unsupported relation type".into());
    }
    let id = Uuid::new_v4().to_string();
    state.with_connection(|connection| {
        ensure_active_knowledge(connection, &source)?;
        ensure_active_knowledge(connection, &target)?;
        connection
            .execute(
                "INSERT INTO knowledge_relations(
                    id, source_knowledge_id, target_knowledge_id, relation_type,
                    confidence, created_by, confirmed, created_at
                 ) VALUES (?1, ?2, ?3, ?4, NULL, 'user', 1, ?5)",
                params![id, source, target, relation_type, now_ms()],
            )
            .map_err(|error| format!("failed to create knowledge relation: {error}"))?;
        let relation = list_relations_for(connection, &source)?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "relation was created but could not be reloaded".to_string())?;
        audit_event(
            connection,
            "knowledge.relation.create",
            "knowledge_relation",
            Some(&id),
            Some(json!({ "sourceId": source, "targetId": target, "relationType": relation_type })),
        )?;
        Ok(relation)
    })
}

#[tauri::command]
pub fn knowledge_relation_list(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
) -> Result<Vec<KnowledgeRelationRecord>, String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    state.with_connection(|connection| list_relations_for(connection, &knowledge_unit_id))
}

#[tauri::command]
pub fn knowledge_relation_remove(
    state: State<'_, DatabaseState>,
    relation_id: String,
) -> Result<(), String> {
    state.with_connection(|connection| {
        connection
            .execute(
                "DELETE FROM knowledge_relations WHERE id = ?1",
                [relation_id.trim()],
            )
            .map_err(|error| format!("failed to remove knowledge relation: {error}"))?;
        audit_event(
            connection,
            "knowledge.relation.remove",
            "knowledge_relation",
            Some(relation_id.trim()),
            None,
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn knowledge_archive(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
    archived: bool,
) -> Result<(), String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    state.with_connection(|connection| {
        let changed = connection
            .execute(
                "UPDATE knowledge_units SET archived_at = ?2, updated_at = ?3
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![
                    knowledge_unit_id,
                    if archived { Some(now_ms()) } else { None },
                    now_ms()
                ],
            )
            .map_err(|error| format!("failed to update archive state: {error}"))?;
        if changed == 0 {
            return Err("knowledge unit not found".into());
        }
        audit_event(
            connection,
            if archived {
                "knowledge.archive"
            } else {
                "knowledge.unarchive"
            },
            "knowledge",
            Some(&knowledge_unit_id),
            None,
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn knowledge_trash(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
) -> Result<(), String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    let deleted_at = now_ms();
    let expires_at = deleted_at.saturating_add(TRASH_RETENTION_MS);
    state.with_connection(|connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start trash transaction: {error}"))?;
        let changed = transaction
            .execute(
                "UPDATE knowledge_units SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                params![knowledge_unit_id, deleted_at],
            )
            .map_err(|error| format!("failed to move knowledge to trash: {error}"))?;
        if changed == 0 {
            return Err("knowledge unit not found or already in trash".into());
        }
        transaction
            .execute(
                "INSERT INTO trash_records(id, entity_type, entity_id, deleted_at, expires_at)
                 VALUES (?1, 'knowledge', ?2, ?3, ?4)
                 ON CONFLICT(entity_type, entity_id) DO UPDATE SET
                    deleted_at = excluded.deleted_at, expires_at = excluded.expires_at",
                params![Uuid::new_v4().to_string(), knowledge_unit_id, deleted_at, expires_at],
            )
            .map_err(|error| format!("failed to record trash entry: {error}"))?;
        audit_event(
            &transaction,
            "knowledge.trash",
            "knowledge",
            Some(&knowledge_unit_id),
            Some(json!({ "expiresAt": expires_at })),
        )?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit trash operation: {error}"))?;
        Ok(())
    })
}

#[tauri::command]
pub fn knowledge_restore(
    state: State<'_, DatabaseState>,
    knowledge_unit_id: String,
) -> Result<(), String> {
    let knowledge_unit_id = required_id(&knowledge_unit_id, "knowledge unit")?;
    state.with_connection(|connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start restore transaction: {error}"))?;
        let changed = transaction
            .execute(
                "UPDATE knowledge_units SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![knowledge_unit_id, now_ms()],
            )
            .map_err(|error| format!("failed to restore knowledge: {error}"))?;
        if changed == 0 {
            return Err("knowledge unit is not in trash".into());
        }
        transaction
            .execute(
                "DELETE FROM trash_records WHERE entity_type = 'knowledge' AND entity_id = ?1",
                [&knowledge_unit_id],
            )
            .map_err(|error| format!("failed to clear trash entry: {error}"))?;
        audit_event(
            &transaction,
            "knowledge.restore",
            "knowledge",
            Some(&knowledge_unit_id),
            None,
        )?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit restore: {error}"))?;
        Ok(())
    })
}

#[tauri::command]
pub fn knowledge_trash_list(
    state: State<'_, DatabaseState>,
) -> Result<Vec<TrashKnowledgeItem>, String> {
    state.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT k.id, k.core_claim, s.selected_text, s.platform, s.title, tr.deleted_at, tr.expires_at
                 FROM trash_records tr
                 JOIN knowledge_units k ON k.id = tr.entity_id AND tr.entity_type = 'knowledge'
                 JOIN sources s ON s.id = k.source_id
                 WHERE k.deleted_at IS NOT NULL
                 ORDER BY tr.deleted_at DESC",
            )
            .map_err(|error| format!("failed to prepare trash query: {error}"))?;
        let items = statement
            .query_map([], |row| {
                Ok(TrashKnowledgeItem {
                    knowledge_unit_id: row.get(0)?,
                    core_claim: row.get(1)?,
                    selected_text: row.get(2)?,
                    platform: row.get(3)?,
                    title: row.get(4)?,
                    deleted_at: row.get(5)?,
                    expires_at: row.get(6)?,
                })
            })
            .map_err(|error| format!("failed to query trash: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read trash: {error}"))?;
        Ok(items)
    })
}

fn list_topics(
    connection: &rusqlite::Connection,
    include_archived: bool,
) -> Result<Vec<TopicRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.name, t.description, t.created_by, t.locked,
                    COUNT(k.id), t.created_at, t.updated_at, t.archived_at
             FROM topics t
             LEFT JOIN knowledge_topics kt ON kt.topic_id = t.id
             LEFT JOIN knowledge_units k ON k.id = kt.knowledge_unit_id AND k.deleted_at IS NULL
             WHERE t.deleted_at IS NULL AND (?1 = 1 OR t.archived_at IS NULL)
             GROUP BY t.id
             ORDER BY CASE WHEN t.archived_at IS NULL THEN 0 ELSE 1 END, lower(t.name) ASC",
        )
        .map_err(|error| format!("failed to prepare topic list: {error}"))?;
    let items = statement
        .query_map([if include_archived { 1 } else { 0 }], map_topic)
        .map_err(|error| format!("failed to query topics: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read topics: {error}"))?;
    Ok(items)
}

fn list_topics_for(
    connection: &rusqlite::Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<TopicRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.name, t.description, t.created_by, t.locked,
                    (SELECT COUNT(*) FROM knowledge_topics all_kt
                     JOIN knowledge_units all_k ON all_k.id = all_kt.knowledge_unit_id
                     WHERE all_kt.topic_id = t.id AND all_k.deleted_at IS NULL),
                    t.created_at, t.updated_at, t.archived_at
             FROM topics t
             JOIN knowledge_topics kt ON kt.topic_id = t.id
             WHERE kt.knowledge_unit_id = ?1 AND t.deleted_at IS NULL
             ORDER BY lower(t.name) ASC",
        )
        .map_err(|error| format!("failed to prepare knowledge topics: {error}"))?;
    let items = statement
        .query_map([knowledge_unit_id], map_topic)
        .map_err(|error| format!("failed to query knowledge topics: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read knowledge topics: {error}"))?;
    Ok(items)
}

fn load_topic(
    connection: &rusqlite::Connection,
    topic_id: &str,
) -> Result<Option<TopicRecord>, String> {
    connection
        .query_row(
            "SELECT t.id, t.name, t.description, t.created_by, t.locked,
                    (SELECT COUNT(*) FROM knowledge_topics kt
                     JOIN knowledge_units k ON k.id = kt.knowledge_unit_id
                     WHERE kt.topic_id = t.id AND k.deleted_at IS NULL),
                    t.created_at, t.updated_at, t.archived_at
             FROM topics t WHERE t.id = ?1 AND t.deleted_at IS NULL",
            [topic_id],
            map_topic,
        )
        .optional()
        .map_err(|error| format!("failed to load topic: {error}"))
}

fn map_topic(row: &rusqlite::Row<'_>) -> rusqlite::Result<TopicRecord> {
    Ok(TopicRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        created_by: row.get(3)?,
        locked: row.get::<_, i64>(4)? != 0,
        knowledge_count: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        archived_at: row.get(8)?,
    })
}

fn update_topic_connection(
    connection: &rusqlite::Connection,
    topic_id: &str,
    name: &str,
    description: &str,
) -> Result<TopicRecord, String> {
    let (locked, archived_at) = connection
        .query_row(
            "SELECT locked, archived_at FROM topics WHERE id = ?1 AND deleted_at IS NULL",
            [topic_id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .map_err(|error| format!("failed to inspect topic: {error}"))?
        .ok_or_else(|| "topic not found".to_string())?;
    if locked {
        return Err("locked topic cannot be edited".into());
    }
    if archived_at.is_some() {
        return Err("archived topic must be restored before editing".into());
    }
    let now = now_ms();
    connection
        .execute(
            "UPDATE topics SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
            params![topic_id, name, description, now],
        )
        .map_err(|error| format!("failed to update topic: {error}"))?;
    audit_event(
        connection,
        "topic.update",
        "topic",
        Some(topic_id),
        Some(json!({ "name": name })),
    )?;
    load_topic(connection, topic_id)?.ok_or_else(|| "topic could not be reloaded".to_string())
}

fn set_topic_archived_connection(
    connection: &rusqlite::Connection,
    topic_id: &str,
    archived: bool,
) -> Result<TopicRecord, String> {
    let locked = connection
        .query_row(
            "SELECT locked FROM topics WHERE id = ?1 AND deleted_at IS NULL",
            [topic_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )
        .optional()
        .map_err(|error| format!("failed to inspect topic: {error}"))?
        .ok_or_else(|| "topic not found".to_string())?;
    if locked {
        return Err("locked topic cannot be archived".into());
    }
    let now = now_ms();
    connection
        .execute(
            "UPDATE topics SET archived_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![topic_id, if archived { Some(now) } else { None }, now],
        )
        .map_err(|error| format!("failed to update topic archive state: {error}"))?;
    audit_event(
        connection,
        if archived { "topic.archive" } else { "topic.restore" },
        "topic",
        Some(topic_id),
        None,
    )?;
    load_topic(connection, topic_id)?.ok_or_else(|| "topic could not be reloaded".to_string())
}

fn update_tag_connection(
    connection: &rusqlite::Connection,
    tag_id: &str,
    name: &str,
) -> Result<TagRecord, String> {
    load_tag(connection, tag_id)?.ok_or_else(|| "tag not found".to_string())?;
    connection
        .execute(
            "UPDATE tags SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![tag_id, name, now_ms()],
        )
        .map_err(|error| format!("failed to update tag: {error}"))?;
    audit_event(
        connection,
        "tag.update",
        "tag",
        Some(tag_id),
        Some(json!({ "name": name })),
    )?;
    load_tag(connection, tag_id)?.ok_or_else(|| "tag could not be reloaded".to_string())
}

fn delete_empty_tag_connection(
    connection: &rusqlite::Connection,
    tag_id: &str,
) -> Result<(), String> {
    let tag = load_tag(connection, tag_id)?.ok_or_else(|| "tag not found".to_string())?;
    let assignments: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_tags WHERE tag_id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect tag assignments: {error}"))?;
    if assignments > 0 {
        return Err("tag is still assigned to knowledge and cannot be deleted".into());
    }
    connection
        .execute("DELETE FROM tags WHERE id = ?1", [tag_id])
        .map_err(|error| format!("failed to delete empty tag: {error}"))?;
    audit_event(
        connection,
        "tag.delete_empty",
        "tag",
        Some(tag_id),
        Some(json!({ "name": tag.name })),
    )?;
    Ok(())
}

fn list_tags(connection: &rusqlite::Connection) -> Result<Vec<TagRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.name, t.created_by,
                    COUNT(CASE WHEN k.id IS NOT NULL AND k.deleted_at IS NULL THEN 1 END), t.created_at, t.updated_at
             FROM tags t
             LEFT JOIN knowledge_tags kt ON kt.tag_id = t.id
             LEFT JOIN knowledge_units k ON k.id = kt.knowledge_unit_id
             GROUP BY t.id ORDER BY lower(t.name) ASC",
        )
        .map_err(|error| format!("failed to prepare tag list: {error}"))?;
    let items = statement
        .query_map([], map_tag)
        .map_err(|error| format!("failed to query tags: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read tags: {error}"))?;
    Ok(items)
}

fn list_tags_for(
    connection: &rusqlite::Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<TagRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.name, t.created_by,
                    (SELECT COUNT(*) FROM knowledge_tags all_kt
                     JOIN knowledge_units all_k ON all_k.id = all_kt.knowledge_unit_id
                     WHERE all_kt.tag_id = t.id AND all_k.deleted_at IS NULL),
                    t.created_at, t.updated_at
             FROM tags t JOIN knowledge_tags kt ON kt.tag_id = t.id
             WHERE kt.knowledge_unit_id = ?1 ORDER BY lower(t.name) ASC",
        )
        .map_err(|error| format!("failed to prepare knowledge tags: {error}"))?;
    let items = statement
        .query_map([knowledge_unit_id], map_tag)
        .map_err(|error| format!("failed to query knowledge tags: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read knowledge tags: {error}"))?;
    Ok(items)
}

fn load_tag(connection: &rusqlite::Connection, tag_id: &str) -> Result<Option<TagRecord>, String> {
    connection
        .query_row(
            "SELECT t.id, t.name, t.created_by,
                    (SELECT COUNT(*) FROM knowledge_tags kt
                     JOIN knowledge_units k ON k.id = kt.knowledge_unit_id
                     WHERE kt.tag_id = t.id AND k.deleted_at IS NULL),
                    t.created_at, t.updated_at FROM tags t WHERE t.id = ?1",
            [tag_id],
            map_tag,
        )
        .optional()
        .map_err(|error| format!("failed to load tag: {error}"))
}

fn map_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRecord> {
    Ok(TagRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        created_by: row.get(2)?,
        knowledge_count: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn list_relations_for(
    connection: &rusqlite::Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<KnowledgeRelationRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT r.id, r.source_knowledge_id, r.target_knowledge_id, r.relation_type,
                    r.confidence, r.created_by, r.confirmed, r.created_at,
                    CASE WHEN r.source_knowledge_id = ?1 THEN r.target_knowledge_id ELSE r.source_knowledge_id END,
                    COALESCE(NULLIF(other.core_claim, ''), other_source.selected_text),
                    CASE WHEN r.source_knowledge_id = ?1 THEN 'outgoing' ELSE 'incoming' END
             FROM knowledge_relations r
             JOIN knowledge_units other ON other.id = CASE
                WHEN r.source_knowledge_id = ?1 THEN r.target_knowledge_id ELSE r.source_knowledge_id END
             JOIN sources other_source ON other_source.id = other.source_id
             WHERE (r.source_knowledge_id = ?1 OR r.target_knowledge_id = ?1)
               AND other.deleted_at IS NULL
             ORDER BY r.created_at DESC",
        )
        .map_err(|error| format!("failed to prepare relation list: {error}"))?;
    let items = statement
        .query_map([knowledge_unit_id], |row| {
            Ok(KnowledgeRelationRecord {
                id: row.get(0)?,
                source_knowledge_id: row.get(1)?,
                target_knowledge_id: row.get(2)?,
                relation_type: row.get(3)?,
                confidence: row.get(4)?,
                created_by: row.get(5)?,
                confirmed: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
                other_knowledge_id: row.get(8)?,
                other_title: row.get(9)?,
                direction: row.get(10)?,
            })
        })
        .map_err(|error| format!("failed to query relations: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read relations: {error}"))?;
    Ok(items)
}

fn ensure_active_knowledge(connection: &rusqlite::Connection, id: &str) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM knowledge_units WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| format!("failed to validate knowledge unit: {error}"))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("knowledge unit not found".into())
    }
}

fn ensure_active_topic(connection: &rusqlite::Connection, id: &str) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM topics WHERE id = ?1 AND deleted_at IS NULL AND archived_at IS NULL",
            [id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| format!("failed to validate topic: {error}"))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("topic not found".into())
    }
}

fn required_id(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} id is empty"))
    } else {
        Ok(value.to_string())
    }
}

fn normalized_name(value: &str, label: &str, max_chars: usize) -> Result<String, String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return Err(format!("{label} name is empty"));
    }
    if value.chars().count() > max_chars {
        return Err(format!("{label} name is too long"));
    }
    Ok(value)
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

fn fts_expression(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(|part| format!("\"{}\"*", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn management_database() -> rusqlite::Connection {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::database::apply_migrations(&mut connection).unwrap();
        connection
    }

    #[test]
    fn topic_lifecycle_preserves_data_and_enforces_active_state() {
        let connection = management_database();
        connection
            .execute_batch(
                "INSERT INTO topics(id, name, description, created_by, locked, created_at, updated_at)
                    VALUES ('topic-1', 'Original', 'before', 'user', 0, 1, 1),
                           ('topic-locked', 'Locked', '', 'user', 1, 1, 1);",
            )
            .unwrap();

        let updated = update_topic_connection(&connection, "topic-1", "Renamed", "after").unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.description, "after");
        assert!(updated.archived_at.is_none());

        let archived = set_topic_archived_connection(&connection, "topic-1", true).unwrap();
        assert!(archived.archived_at.is_some());
        assert!(ensure_active_topic(&connection, "topic-1").is_err());
        assert!(list_topics(&connection, false).unwrap().iter().all(|item| item.id != "topic-1"));
        assert!(list_topics(&connection, true).unwrap().iter().any(|item| item.id == "topic-1" && item.archived_at.is_some()));
        assert!(update_topic_connection(&connection, "topic-1", "Nope", "").is_err());

        let restored = set_topic_archived_connection(&connection, "topic-1", false).unwrap();
        assert!(restored.archived_at.is_none());
        assert!(ensure_active_topic(&connection, "topic-1").is_ok());
        assert!(update_topic_connection(&connection, "topic-locked", "Nope", "").is_err());
        assert!(set_topic_archived_connection(&connection, "topic-locked", true).is_err());
    }

    #[test]
    fn empty_tag_deletion_refuses_any_existing_assignment() {
        let connection = management_database();
        connection
            .execute_batch(
                "INSERT INTO sources(id, platform, selected_text, content_hash, captured_at)
                    VALUES ('source-1', 'web', 'source text', 'management-tag-source', 1);
                 INSERT INTO knowledge_units(id, source_id, core_claim, status, created_at, updated_at)
                    VALUES ('knowledge-1', 'source-1', 'claim', 'learning', 2, 2);
                 INSERT INTO tags(id, name, created_by, created_at, updated_at)
                    VALUES ('tag-empty', 'Empty', 'user', 3, 3),
                           ('tag-used', 'Used', 'user', 3, 3);
                 INSERT INTO knowledge_tags(knowledge_unit_id, tag_id, created_by, created_at)
                    VALUES ('knowledge-1', 'tag-used', 'user', 4);",
            )
            .unwrap();

        let renamed = update_tag_connection(&connection, "tag-used", "Corrected").unwrap();
        assert_eq!(renamed.name, "Corrected");
        assert_eq!(renamed.knowledge_count, 1);
        let assignment_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_tags WHERE tag_id = 'tag-used'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(assignment_count, 1);

        delete_empty_tag_connection(&connection, "tag-empty").unwrap();
        assert!(load_tag(&connection, "tag-empty").unwrap().is_none());
        assert!(delete_empty_tag_connection(&connection, "tag-used").is_err());
        assert!(load_tag(&connection, "tag-used").unwrap().is_some());
    }

    #[test]
    fn names_are_normalized_without_becoming_folders() {
        assert_eq!(
            normalized_name("  Agent   Reliability ", "topic", 120).unwrap(),
            "Agent Reliability"
        );
        assert!(normalized_name("   ", "topic", 120).is_err());
    }

    #[test]
    fn relation_types_are_explicit_semantics() {
        assert!(RELATION_TYPES.contains(&"contradicts"));
        assert!(RELATION_TYPES.contains(&"prerequisite_of"));
        assert!(!RELATION_TYPES.contains(&"similarity"));
    }

    #[test]
    fn search_escapes_like_wildcards_and_fts_quotes() {
        assert_eq!(escape_like("50%_done"), "50\\%\\_done");
        assert_eq!(fts_expression("agent retry"), "\"agent\"* AND \"retry\"*");
    }

    #[test]
    fn sqlite_search_escape_and_fts_are_executable() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        let pattern = format!("%{}%", escape_like("50%_done"));
        let like_match: i64 = connection
            .query_row(
                "SELECT CASE WHEN ?1 LIKE ?2 ESCAPE '\\' THEN 1 ELSE 0 END",
                params!["prefix 50%_done suffix", pattern],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(like_match, 1);

        connection
            .execute_batch(
                "CREATE VIRTUAL TABLE search_test USING fts5(body, tokenize = 'unicode61');
                 INSERT INTO search_test(body) VALUES ('agent retry strategy');",
            )
            .unwrap();
        let fts_match: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM search_test WHERE search_test MATCH ?1",
                [fts_expression("agent retry")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_match, 1);
    }
}
