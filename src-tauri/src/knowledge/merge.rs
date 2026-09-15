use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::database::{safety::audit_event, DatabaseState};

use super::now_ms;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSideSummary {
    pub id: String,
    pub title: String,
    pub source_count: i64,
    pub evidence_count: i64,
    pub topic_count: i64,
    pub tag_count: i64,
    pub relation_count: i64,
    pub review_count: i64,
    pub mastery_score: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMergeRecord {
    pub id: String,
    pub curation_candidate_id: String,
    pub target_knowledge_id: String,
    pub source_knowledge_id: String,
    pub status: String,
    pub created_at: i64,
    pub reverted_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMergePreview {
    pub candidate_id: String,
    pub left: MergeSideSummary,
    pub right: MergeSideSummary,
    pub recommended_target_id: String,
    pub recommendation_reason: String,
    pub existing_merge: Option<DuplicateMergeRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeSourceLink {
    source_id: String,
    role: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeSnapshot {
    source_archived_at: Option<i64>,
    source_updated_at: i64,
    source_links: Vec<MergeSourceLink>,
    evidence_ids: Vec<String>,
    topic_ids: Vec<String>,
    tag_ids: Vec<String>,
    relation_ids: Vec<String>,
}

#[derive(Debug)]
struct DuplicateCandidate {
    id: String,
    left_id: String,
    right_id: String,
}

#[tauri::command]
pub fn duplicate_merge_preview(
    state: State<'_, DatabaseState>,
    candidate_id: String,
) -> Result<DuplicateMergePreview, String> {
    let candidate_id = required_id(&candidate_id, "candidate")?;
    state.with_connection(|connection| build_preview(connection, &candidate_id))
}

#[tauri::command]
pub fn duplicate_merge_apply(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    candidate_id: String,
    target_knowledge_id: String,
    expected_left_updated_at: i64,
    expected_right_updated_at: i64,
) -> Result<DuplicateMergeRecord, String> {
    let candidate_id = required_id(&candidate_id, "candidate")?;
    let target_knowledge_id = required_id(&target_knowledge_id, "target knowledge")?;
    let record = state.with_connection(|connection| {
        apply_merge_connection(
            connection,
            &candidate_id,
            &target_knowledge_id,
            expected_left_updated_at,
            expected_right_updated_at,
        )
    })?;

    let _ = app.emit("knowledge://changed", json!({ "reason": "merge-apply" }));
    let _ = app.emit("curation://changed", json!({ "candidateId": candidate_id }));
    Ok(record)
}

fn apply_merge_connection(
    connection: &mut Connection,
    candidate_id: &str,
    target_knowledge_id: &str,
    expected_left_updated_at: i64,
    expected_right_updated_at: i64,
) -> Result<DuplicateMergeRecord, String> {
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to start merge transaction: {error}"))?;
    let candidate = load_confirmed_duplicate(&transaction, candidate_id)?;
    if target_knowledge_id != candidate.left_id && target_knowledge_id != candidate.right_id {
        return Err("merge target is not part of this duplicate pair".into());
    }
    let source_knowledge_id = if target_knowledge_id == candidate.left_id {
        candidate.right_id.clone()
    } else {
        candidate.left_id.clone()
    };

    if let Some(existing) = load_active_merge_for_candidate(&transaction, candidate_id)? {
        if existing.target_knowledge_id == target_knowledge_id {
            transaction
                .commit()
                .map_err(|error| format!("failed to finish idempotent merge: {error}"))?;
            return Ok(existing);
        }
        return Err("this duplicate pair is already merged with a different target".into());
    }

    validate_merge_revisions(
        &transaction,
        &candidate,
        expected_left_updated_at,
        expected_right_updated_at,
    )?;
    ensure_active_knowledge(&transaction, target_knowledge_id)?;
    ensure_active_knowledge(&transaction, &source_knowledge_id)?;

    let source_archived_at = transaction
        .query_row(
            "SELECT archived_at FROM knowledge_units WHERE id = ?1",
            [&source_knowledge_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(|error| format!("failed to read source archive state: {error}"))?;
    let source_updated_at = transaction
        .query_row(
            "SELECT updated_at FROM knowledge_units WHERE id = ?1",
            [&source_knowledge_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("failed to read source revision: {error}"))?;

    let merge_id = Uuid::new_v4().to_string();
    let marker = format!("merge:{merge_id}");
    let created_at = now_ms();
    let source_links = copy_source_links(
        &transaction,
        target_knowledge_id,
        &source_knowledge_id,
        &marker,
        created_at,
    )?;
    let evidence_ids = copy_supporting_evidence(
        &transaction,
        target_knowledge_id,
        &source_knowledge_id,
        &marker,
        created_at,
    )?;
    let topic_ids = copy_topics(
        &transaction,
        target_knowledge_id,
        &source_knowledge_id,
        &marker,
        created_at,
    )?;
    let tag_ids = copy_tags(
        &transaction,
        target_knowledge_id,
        &source_knowledge_id,
        &marker,
        created_at,
    )?;
    let relation_ids = copy_relations(
        &transaction,
        target_knowledge_id,
        &source_knowledge_id,
        &marker,
        created_at,
    )?;

    let snapshot = MergeSnapshot {
        source_archived_at,
        source_updated_at,
        source_links,
        evidence_ids,
        topic_ids,
        tag_ids,
        relation_ids,
    };
    let snapshot_json = serde_json::to_string(&snapshot)
        .map_err(|error| format!("failed to serialize merge snapshot: {error}"))?;

    let changed = transaction
        .execute(
            "UPDATE knowledge_units
             SET archived_at = ?2, updated_at = ?2
             WHERE id = ?1 AND deleted_at IS NULL AND archived_at IS NULL AND updated_at = ?3",
            params![source_knowledge_id, created_at, source_updated_at],
        )
        .map_err(|error| format!("failed to archive merged knowledge: {error}"))?;
    if changed == 0 {
        return Err("duplicate knowledge changed before merge could be applied".into());
    }

    transaction
        .execute(
            "INSERT INTO knowledge_merges(
                id, curation_candidate_id, target_knowledge_id, source_knowledge_id,
                status, snapshot_json, created_at, reverted_at
             ) VALUES (?1, ?2, ?3, ?4, 'applied', ?5, ?6, NULL)",
            params![
                merge_id,
                candidate_id,
                target_knowledge_id,
                source_knowledge_id,
                snapshot_json,
                created_at
            ],
        )
        .map_err(|error| format!("failed to record knowledge merge: {error}"))?;
    audit_event(
        &transaction,
        "knowledge.merge.apply",
        "knowledge_merge",
        Some(&merge_id),
        Some(json!({
            "candidateId": candidate_id,
            "targetKnowledgeId": target_knowledge_id,
            "sourceKnowledgeId": source_knowledge_id,
            "copiedSources": snapshot.source_links.len(),
            "copiedEvidence": snapshot.evidence_ids.len(),
            "copiedTopics": snapshot.topic_ids.len(),
            "copiedTags": snapshot.tag_ids.len(),
            "copiedRelations": snapshot.relation_ids.len()
        })),
    )?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit knowledge merge: {error}"))?;
    load_merge(connection, &merge_id)?
        .ok_or_else(|| "knowledge merge could not be reloaded".to_string())
}

#[tauri::command]
pub fn duplicate_merge_revert(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    merge_id: String,
) -> Result<DuplicateMergeRecord, String> {
    let merge_id = required_id(&merge_id, "merge")?;
    let record =
        state.with_connection(|connection| revert_merge_connection(connection, &merge_id))?;

    let _ = app.emit("knowledge://changed", json!({ "reason": "merge-revert" }));
    let _ = app.emit("curation://changed", json!({ "mergeId": merge_id }));
    Ok(record)
}

fn revert_merge_connection(
    connection: &mut Connection,
    merge_id: &str,
) -> Result<DuplicateMergeRecord, String> {
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to start merge revert transaction: {error}"))?;
    let merge = load_merge(&transaction, merge_id)?
        .ok_or_else(|| "knowledge merge not found".to_string())?;
    if merge.status == "reverted" {
        transaction
            .commit()
            .map_err(|error| format!("failed to finish idempotent merge revert: {error}"))?;
        return Ok(merge);
    }
    if merge.status != "applied" {
        return Err("knowledge merge is not reversible in its current state".into());
    }
    let snapshot_json = transaction
        .query_row(
            "SELECT snapshot_json FROM knowledge_merges WHERE id = ?1",
            [merge_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| format!("failed to load merge snapshot: {error}"))?;
    let snapshot: MergeSnapshot = serde_json::from_str(&snapshot_json)
        .map_err(|error| format!("invalid merge snapshot: {error}"))?;
    let current_source_state = transaction
        .query_row(
            "SELECT archived_at, deleted_at FROM knowledge_units WHERE id = ?1",
            [&merge.source_knowledge_id],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .map_err(|error| format!("failed to inspect merged source state: {error}"))?
        .ok_or_else(|| "merged source knowledge no longer exists".to_string())?;
    if current_source_state.1.is_some() || current_source_state.0 != Some(merge.created_at) {
        return Err("merged source changed after merge; automatic revert was blocked".into());
    }

    let marker = format!("merge:{merge_id}");
    transaction
        .execute(
            "DELETE FROM claim_evidence WHERE created_by = ?1",
            [&marker],
        )
        .map_err(|error| format!("failed to remove merged evidence links: {error}"))?;
    transaction
        .execute(
            "DELETE FROM knowledge_source_links WHERE created_by = ?1",
            [&marker],
        )
        .map_err(|error| format!("failed to remove merged source links: {error}"))?;
    transaction
        .execute(
            "DELETE FROM knowledge_topics WHERE created_by = ?1",
            [&marker],
        )
        .map_err(|error| format!("failed to remove merged topics: {error}"))?;
    transaction
        .execute(
            "DELETE FROM knowledge_tags WHERE created_by = ?1",
            [&marker],
        )
        .map_err(|error| format!("failed to remove merged tags: {error}"))?;
    transaction
        .execute(
            "DELETE FROM knowledge_relations WHERE created_by = ?1",
            [&marker],
        )
        .map_err(|error| format!("failed to remove merged relations: {error}"))?;

    let reverted_at = now_ms();
    let changed = transaction
        .execute(
            "UPDATE knowledge_units
             SET archived_at = ?2, updated_at = ?3
             WHERE id = ?1 AND archived_at = ?4 AND deleted_at IS NULL",
            params![
                merge.source_knowledge_id,
                snapshot.source_archived_at,
                reverted_at,
                merge.created_at
            ],
        )
        .map_err(|error| format!("failed to restore merged knowledge: {error}"))?;
    if changed == 0 {
        return Err("merged source could not be restored safely".into());
    }
    transaction
        .execute(
            "UPDATE knowledge_merges SET status = 'reverted', reverted_at = ?2 WHERE id = ?1",
            params![merge_id, reverted_at],
        )
        .map_err(|error| format!("failed to mark merge reverted: {error}"))?;
    audit_event(
        &transaction,
        "knowledge.merge.revert",
        "knowledge_merge",
        Some(merge_id),
        Some(json!({
            "targetKnowledgeId": merge.target_knowledge_id,
            "sourceKnowledgeId": merge.source_knowledge_id
        })),
    )?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit merge revert: {error}"))?;
    load_merge(connection, merge_id)?
        .ok_or_else(|| "reverted merge could not be reloaded".to_string())
}

fn build_preview(
    connection: &Connection,
    candidate_id: &str,
) -> Result<DuplicateMergePreview, String> {
    let candidate = load_confirmed_duplicate(connection, candidate_id)?;
    let existing_merge = load_active_merge_for_candidate(connection, candidate_id)?;
    let left = load_side(connection, &candidate.left_id)?;
    let right = load_side(connection, &candidate.right_id)?;
    if existing_merge.is_none() && (left.archived_at.is_some() || right.archived_at.is_some()) {
        return Err("one of the duplicate items is archived; restore it before merging".into());
    }
    let (recommended_target_id, recommendation_reason) = if let Some(existing) = &existing_merge {
        (
            existing.target_knowledge_id.clone(),
            "这组重复知识已经完成合并。".to_string(),
        )
    } else {
        recommend_target(&left, &right)
    };
    Ok(DuplicateMergePreview {
        candidate_id: candidate.id,
        left,
        right,
        recommended_target_id,
        recommendation_reason,
        existing_merge,
    })
}

fn recommend_target(left: &MergeSideSummary, right: &MergeSideSummary) -> (String, String) {
    if left.review_count != right.review_count {
        let chosen = if left.review_count > right.review_count {
            left
        } else {
            right
        };
        return (
            chosen.id.clone(),
            "优先保留复习记录更完整的一条，避免人为迁移 mastery。".into(),
        );
    }
    if left.evidence_count != right.evidence_count {
        let chosen = if left.evidence_count > right.evidence_count {
            left
        } else {
            right
        };
        return (
            chosen.id.clone(),
            "复习记录相同，优先保留已有支持证据更多的一条。".into(),
        );
    }
    if left.source_count != right.source_count {
        let chosen = if left.source_count > right.source_count {
            left
        } else {
            right
        };
        return (
            chosen.id.clone(),
            "复习与证据数量相同，优先保留来源更丰富的一条。".into(),
        );
    }
    let chosen = if left.updated_at >= right.updated_at {
        left
    } else {
        right
    };
    (
        chosen.id.clone(),
        "两条知识信息量接近，默认保留最近更新的一条。".into(),
    )
}

fn load_side(connection: &Connection, knowledge_id: &str) -> Result<MergeSideSummary, String> {
    connection
        .query_row(
            "SELECT k.id, COALESCE(NULLIF(k.core_claim, ''), s.selected_text),
                    (SELECT COUNT(*) FROM knowledge_source_links l WHERE l.knowledge_unit_id = k.id),
                    (SELECT COUNT(*) FROM claim_evidence ce WHERE ce.claim_id = 'primary:' || k.id AND ce.stance = 'supports'),
                    (SELECT COUNT(*) FROM knowledge_topics kt WHERE kt.knowledge_unit_id = k.id),
                    (SELECT COUNT(*) FROM knowledge_tags kg WHERE kg.knowledge_unit_id = k.id),
                    (SELECT COUNT(*) FROM knowledge_relations kr WHERE kr.source_knowledge_id = k.id OR kr.target_knowledge_id = k.id),
                    COALESCE(r.review_count, 0), COALESCE(r.mastery_score, 0), k.updated_at, k.archived_at
             FROM knowledge_units k
             JOIN sources s ON s.id = k.source_id
             LEFT JOIN review_states r ON r.knowledge_unit_id = k.id
             WHERE k.id = ?1 AND k.deleted_at IS NULL",
            [knowledge_id],
            |row| {
                Ok(MergeSideSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    source_count: row.get(2)?,
                    evidence_count: row.get(3)?,
                    topic_count: row.get(4)?,
                    tag_count: row.get(5)?,
                    relation_count: row.get(6)?,
                    review_count: row.get(7)?,
                    mastery_score: row.get(8)?,
                    updated_at: row.get(9)?,
                    archived_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to load merge knowledge summary: {error}"))?
        .ok_or_else(|| "duplicate knowledge not found".to_string())
}

fn load_confirmed_duplicate(
    connection: &Connection,
    candidate_id: &str,
) -> Result<DuplicateCandidate, String> {
    connection
        .query_row(
            "SELECT id, left_knowledge_id, right_knowledge_id
             FROM curation_candidates
             WHERE id = ?1 AND classification = 'duplicate' AND status = 'applied'",
            [candidate_id],
            |row| {
                Ok(DuplicateCandidate {
                    id: row.get(0)?,
                    left_id: row.get(1)?,
                    right_id: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to load confirmed duplicate: {error}"))?
        .ok_or_else(|| "duplicate must be confirmed before merge preview".to_string())
}

fn validate_merge_revisions(
    connection: &Connection,
    candidate: &DuplicateCandidate,
    expected_left_updated_at: i64,
    expected_right_updated_at: i64,
) -> Result<(), String> {
    let left_updated_at = knowledge_updated_at(connection, &candidate.left_id)?;
    let right_updated_at = knowledge_updated_at(connection, &candidate.right_id)?;
    if left_updated_at != expected_left_updated_at || right_updated_at != expected_right_updated_at
    {
        return Err("duplicate knowledge changed after preview; refresh the merge preview".into());
    }
    Ok(())
}

fn knowledge_updated_at(connection: &Connection, knowledge_id: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT updated_at FROM knowledge_units WHERE id = ?1 AND deleted_at IS NULL",
            [knowledge_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| format!("failed to read knowledge revision: {error}"))?
        .ok_or_else(|| "knowledge unit not found".to_string())
}

fn ensure_active_knowledge(connection: &Connection, knowledge_id: &str) -> Result<(), String> {
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_units
             WHERE id = ?1 AND deleted_at IS NULL AND archived_at IS NULL",
            [knowledge_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to validate merge knowledge: {error}"))?;
    if exists > 0 {
        Ok(())
    } else {
        Err("knowledge is no longer active".into())
    }
}

fn copy_source_links(
    connection: &Connection,
    target_id: &str,
    source_id: &str,
    marker: &str,
    created_at: i64,
) -> Result<Vec<MergeSourceLink>, String> {
    let mut statement = connection
        .prepare("SELECT source_id, role FROM knowledge_source_links WHERE knowledge_unit_id = ?1")
        .map_err(|error| format!("failed to prepare merge source query: {error}"))?;
    let rows = statement
        .query_map([source_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("failed to query merge sources: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read merge source: {error}"))?;
    let mut inserted = Vec::new();
    for (linked_source_id, role) in rows {
        let already_linked: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_source_links WHERE knowledge_unit_id = ?1 AND source_id = ?2",
                params![target_id, linked_source_id],
                |row| row.get(0),
            )
            .map_err(|error| format!("failed to inspect merge source: {error}"))?;
        if already_linked > 0 {
            continue;
        }
        let merged_role = if role == "origin" {
            "supporting"
        } else {
            role.as_str()
        };
        let changed = connection
            .execute(
                "INSERT OR IGNORE INTO knowledge_source_links(
                    knowledge_unit_id, source_id, role, created_by, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![target_id, linked_source_id, merged_role, marker, created_at],
            )
            .map_err(|error| format!("failed to copy merge source: {error}"))?;
        if changed > 0 {
            inserted.push(MergeSourceLink {
                source_id: linked_source_id,
                role: merged_role.to_string(),
            });
        }
    }
    Ok(inserted)
}

fn copy_supporting_evidence(
    connection: &Connection,
    target_id: &str,
    source_id: &str,
    marker: &str,
    created_at: i64,
) -> Result<Vec<String>, String> {
    let target_claim = format!("primary:{target_id}");
    let source_claim = format!("primary:{source_id}");
    let target_exists = connection
        .query_row(
            "SELECT 1 FROM claims WHERE id = ?1 AND retired_at IS NULL",
            [&target_claim],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| format!("failed to inspect merge target claim: {error}"))?
        .is_some();
    if !target_exists {
        return Err("merge target has no active primary claim".into());
    }
    let mut statement = connection
        .prepare(
            "SELECT evidence_id, confidence FROM claim_evidence
             WHERE claim_id = ?1 AND stance = 'supports'",
        )
        .map_err(|error| format!("failed to prepare merge evidence query: {error}"))?;
    let evidence = statement
        .query_map([source_claim], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?))
        })
        .map_err(|error| format!("failed to query merge evidence: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read merge evidence: {error}"))?;
    let mut inserted = Vec::new();
    for (evidence_id, confidence) in evidence {
        let changed = connection
            .execute(
                "INSERT OR IGNORE INTO claim_evidence(
                    claim_id, evidence_id, stance, confidence, created_by, created_at
                 ) VALUES (?1, ?2, 'supports', ?3, ?4, ?5)",
                params![target_claim, evidence_id, confidence, marker, created_at],
            )
            .map_err(|error| format!("failed to copy merge evidence: {error}"))?;
        if changed > 0 {
            inserted.push(evidence_id);
        }
    }
    Ok(inserted)
}

fn copy_topics(
    connection: &Connection,
    target_id: &str,
    source_id: &str,
    marker: &str,
    created_at: i64,
) -> Result<Vec<String>, String> {
    copy_named_links(
        connection,
        "knowledge_topics",
        "topic_id",
        target_id,
        source_id,
        marker,
        created_at,
    )
}

fn copy_tags(
    connection: &Connection,
    target_id: &str,
    source_id: &str,
    marker: &str,
    created_at: i64,
) -> Result<Vec<String>, String> {
    copy_named_links(
        connection,
        "knowledge_tags",
        "tag_id",
        target_id,
        source_id,
        marker,
        created_at,
    )
}

fn copy_named_links(
    connection: &Connection,
    table: &str,
    id_column: &str,
    target_id: &str,
    source_id: &str,
    marker: &str,
    created_at: i64,
) -> Result<Vec<String>, String> {
    let select_sql = format!("SELECT {id_column} FROM {table} WHERE knowledge_unit_id = ?1");
    let mut statement = connection
        .prepare(&select_sql)
        .map_err(|error| format!("failed to prepare merge metadata query: {error}"))?;
    let ids = statement
        .query_map([source_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query merge metadata: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read merge metadata: {error}"))?;
    let insert_sql = format!(
        "INSERT OR IGNORE INTO {table}(knowledge_unit_id, {id_column}, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4)"
    );
    let mut inserted = Vec::new();
    for id in ids {
        let changed = connection
            .execute(&insert_sql, params![target_id, id, marker, created_at])
            .map_err(|error| format!("failed to copy merge metadata: {error}"))?;
        if changed > 0 {
            inserted.push(id);
        }
    }
    Ok(inserted)
}

fn copy_relations(
    connection: &Connection,
    target_id: &str,
    source_id: &str,
    marker: &str,
    created_at: i64,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT source_knowledge_id, target_knowledge_id, relation_type, confidence, confirmed
             FROM knowledge_relations
             WHERE source_knowledge_id = ?1 OR target_knowledge_id = ?1",
        )
        .map_err(|error| format!("failed to prepare merge relation query: {error}"))?;
    let relations = statement
        .query_map([source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| format!("failed to query merge relations: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read merge relation: {error}"))?;
    let mut inserted = Vec::new();
    for (relation_source, relation_target, relation_type, confidence, confirmed) in relations {
        let new_source = if relation_source == source_id {
            target_id
        } else {
            relation_source.as_str()
        };
        let new_target = if relation_target == source_id {
            target_id
        } else {
            relation_target.as_str()
        };
        if new_source == new_target
            || relation_exists(connection, new_source, new_target, &relation_type)?
        {
            continue;
        }
        let relation_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO knowledge_relations(
                    id, source_knowledge_id, target_knowledge_id, relation_type,
                    confidence, created_by, confirmed, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    relation_id,
                    new_source,
                    new_target,
                    relation_type,
                    confidence,
                    marker,
                    confirmed,
                    created_at
                ],
            )
            .map_err(|error| format!("failed to copy merge relation: {error}"))?;
        inserted.push(relation_id);
    }
    Ok(inserted)
}

fn relation_exists(
    connection: &Connection,
    source_id: &str,
    target_id: &str,
    relation_type: &str,
) -> Result<bool, String> {
    let exact: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_relations
             WHERE source_knowledge_id = ?1 AND target_knowledge_id = ?2 AND relation_type = ?3",
            params![source_id, target_id, relation_type],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect merge relation: {error}"))?;
    if exact > 0 {
        return Ok(true);
    }
    if matches!(relation_type, "related_to" | "contradicts") {
        let reverse: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relations
                 WHERE source_knowledge_id = ?1 AND target_knowledge_id = ?2 AND relation_type = ?3",
                params![target_id, source_id, relation_type],
                |row| row.get(0),
            )
            .map_err(|error| format!("failed to inspect reverse merge relation: {error}"))?;
        return Ok(reverse > 0);
    }
    Ok(false)
}

fn load_active_merge_for_candidate(
    connection: &Connection,
    candidate_id: &str,
) -> Result<Option<DuplicateMergeRecord>, String> {
    connection
        .query_row(
            "SELECT id, curation_candidate_id, target_knowledge_id, source_knowledge_id,
                    status, created_at, reverted_at
             FROM knowledge_merges
             WHERE curation_candidate_id = ?1 AND status = 'applied'
             ORDER BY created_at DESC LIMIT 1",
            [candidate_id],
            map_merge,
        )
        .optional()
        .map_err(|error| format!("failed to load active merge: {error}"))
}

fn load_merge(
    connection: &Connection,
    merge_id: &str,
) -> Result<Option<DuplicateMergeRecord>, String> {
    connection
        .query_row(
            "SELECT id, curation_candidate_id, target_knowledge_id, source_knowledge_id,
                    status, created_at, reverted_at
             FROM knowledge_merges WHERE id = ?1",
            [merge_id],
            map_merge,
        )
        .optional()
        .map_err(|error| format!("failed to load knowledge merge: {error}"))
}

fn map_merge(row: &rusqlite::Row<'_>) -> rusqlite::Result<DuplicateMergeRecord> {
    Ok(DuplicateMergeRecord {
        id: row.get(0)?,
        curation_candidate_id: row.get(1)?,
        target_knowledge_id: row.get(2)?,
        source_knowledge_id: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        reverted_at: row.get(6)?,
    })
}

fn required_id(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} id is empty"))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge_database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE sources(
                    id TEXT PRIMARY KEY, selected_text TEXT NOT NULL
                 );
                 CREATE TABLE knowledge_units(
                    id TEXT PRIMARY KEY, source_id TEXT NOT NULL, core_claim TEXT NOT NULL,
                    updated_at INTEGER NOT NULL, archived_at INTEGER, deleted_at INTEGER
                 );
                 CREATE TABLE review_states(
                    knowledge_unit_id TEXT PRIMARY KEY, mastery_score INTEGER NOT NULL,
                    review_count INTEGER NOT NULL
                 );
                 CREATE TABLE curation_candidates(
                    id TEXT PRIMARY KEY, left_knowledge_id TEXT NOT NULL,
                    right_knowledge_id TEXT NOT NULL, classification TEXT NOT NULL,
                    status TEXT NOT NULL
                 );
                 CREATE TABLE knowledge_merges(
                    id TEXT PRIMARY KEY, curation_candidate_id TEXT NOT NULL,
                    target_knowledge_id TEXT NOT NULL, source_knowledge_id TEXT NOT NULL,
                    status TEXT NOT NULL, snapshot_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL, reverted_at INTEGER
                 );
                 CREATE TABLE knowledge_source_links(
                    knowledge_unit_id TEXT NOT NULL, source_id TEXT NOT NULL, role TEXT NOT NULL,
                    created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, source_id, role)
                 );
                 CREATE TABLE claims(
                    id TEXT PRIMARY KEY, knowledge_unit_id TEXT NOT NULL, retired_at INTEGER
                 );
                 CREATE TABLE claim_evidence(
                    claim_id TEXT NOT NULL, evidence_id TEXT NOT NULL, stance TEXT NOT NULL,
                    confidence REAL, created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
                    PRIMARY KEY(claim_id, evidence_id, stance)
                 );
                 CREATE TABLE knowledge_topics(
                    knowledge_unit_id TEXT NOT NULL, topic_id TEXT NOT NULL,
                    created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, topic_id)
                 );
                 CREATE TABLE knowledge_tags(
                    knowledge_unit_id TEXT NOT NULL, tag_id TEXT NOT NULL,
                    created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, tag_id)
                 );
                 CREATE TABLE knowledge_relations(
                    id TEXT PRIMARY KEY, source_knowledge_id TEXT NOT NULL,
                    target_knowledge_id TEXT NOT NULL, relation_type TEXT NOT NULL,
                    confidence REAL, created_by TEXT NOT NULL, confirmed INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    UNIQUE(source_knowledge_id, target_knowledge_id, relation_type)
                 );
                 CREATE TABLE audit_log(
                    id TEXT PRIMARY KEY, action TEXT NOT NULL, entity_type TEXT NOT NULL,
                    entity_id TEXT, detail_json TEXT, created_at INTEGER NOT NULL
                 );

                 INSERT INTO sources VALUES
                    ('s1', 'Source one'), ('s2', 'Source two'), ('s3', 'Source three');
                 INSERT INTO knowledge_units VALUES
                    ('k1', 's1', 'Keep claim', 100, NULL, NULL),
                    ('k2', 's2', 'Duplicate claim', 200, NULL, NULL),
                    ('k3', 's3', 'Related claim', 300, NULL, NULL);
                 INSERT INTO review_states VALUES
                    ('k1', 82, 5), ('k2', 31, 1);
                 INSERT INTO curation_candidates VALUES
                    ('c1', 'k1', 'k2', 'duplicate', 'applied');
                 INSERT INTO knowledge_source_links VALUES
                    ('k1', 's1', 'origin', 'system', 1),
                    ('k2', 's2', 'origin', 'system', 1);
                 INSERT INTO claims VALUES
                    ('primary:k1', 'k1', NULL), ('primary:k2', 'k2', NULL);
                 INSERT INTO claim_evidence VALUES
                    ('primary:k1', 'e1', 'supports', 1.0, 'system', 1),
                    ('primary:k2', 'e2', 'supports', 0.9, 'system', 1);
                 INSERT INTO knowledge_topics VALUES
                    ('k1', 't1', 'user', 1), ('k2', 't2', 'user', 1);
                 INSERT INTO knowledge_tags VALUES
                    ('k2', 'g2', 'user', 1);
                 INSERT INTO knowledge_relations VALUES
                    ('r1', 'k2', 'k3', 'related_to', 0.8, 'user', 1, 1);",
            )
            .unwrap();
        connection
    }

    #[test]
    fn preview_recommends_reviewed_item_and_merge_round_trip_is_reversible() {
        let mut connection = merge_database();
        let preview = build_preview(&connection, "c1").unwrap();
        assert_eq!(preview.recommended_target_id, "k1");
        assert_eq!(preview.left.review_count, 5);
        assert_eq!(preview.right.review_count, 1);

        let merge = apply_merge_connection(&mut connection, "c1", "k1", 100, 200).unwrap();
        assert_eq!(merge.status, "applied");
        assert_eq!(merge.target_knowledge_id, "k1");
        assert_eq!(merge.source_knowledge_id, "k2");

        let archived_at: Option<i64> = connection
            .query_row(
                "SELECT archived_at FROM knowledge_units WHERE id = 'k2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(archived_at, Some(merge.created_at));

        let merged_source: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_source_links
                 WHERE knowledge_unit_id = 'k1' AND source_id = 's2' AND role = 'supporting'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let merged_evidence: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM claim_evidence
                 WHERE claim_id = 'primary:k1' AND evidence_id = 'e2' AND stance = 'supports'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let merged_topic: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_topics WHERE knowledge_unit_id = 'k1' AND topic_id = 't2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let merged_tag: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_tags WHERE knowledge_unit_id = 'k1' AND tag_id = 'g2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let merged_relation: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relations
                 WHERE source_knowledge_id = 'k1' AND target_knowledge_id = 'k3' AND relation_type = 'related_to'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(merged_source, 1);
        assert_eq!(merged_evidence, 1);
        assert_eq!(merged_topic, 1);
        assert_eq!(merged_tag, 1);
        assert_eq!(merged_relation, 1);

        let review_before_revert: (i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'k1'),
                    (SELECT review_count FROM review_states WHERE knowledge_unit_id = 'k1'),
                    (SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'k2'),
                    (SELECT review_count FROM review_states WHERE knowledge_unit_id = 'k2')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(review_before_revert, (82, 5, 31, 1));

        let reverted = revert_merge_connection(&mut connection, &merge.id).unwrap();
        assert_eq!(reverted.status, "reverted");
        let restored_archived_at: Option<i64> = connection
            .query_row(
                "SELECT archived_at FROM knowledge_units WHERE id = 'k2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(restored_archived_at, None);

        for sql in [
            "SELECT COUNT(*) FROM knowledge_source_links WHERE knowledge_unit_id = 'k1' AND source_id = 's2'",
            "SELECT COUNT(*) FROM claim_evidence WHERE claim_id = 'primary:k1' AND evidence_id = 'e2'",
            "SELECT COUNT(*) FROM knowledge_topics WHERE knowledge_unit_id = 'k1' AND topic_id = 't2'",
            "SELECT COUNT(*) FROM knowledge_tags WHERE knowledge_unit_id = 'k1' AND tag_id = 'g2'",
            "SELECT COUNT(*) FROM knowledge_relations WHERE source_knowledge_id = 'k1' AND target_knowledge_id = 'k3' AND relation_type = 'related_to'",
        ] {
            let count: i64 = connection.query_row(sql, [], |row| row.get(0)).unwrap();
            assert_eq!(count, 0, "merge-created row should be removed: {sql}");
        }

        let originals: (i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM knowledge_source_links WHERE knowledge_unit_id = 'k2' AND source_id = 's2'),
                    (SELECT COUNT(*) FROM claim_evidence WHERE claim_id = 'primary:k2' AND evidence_id = 'e2'),
                    (SELECT COUNT(*) FROM knowledge_relations WHERE source_knowledge_id = 'k2' AND target_knowledge_id = 'k3')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(originals, (1, 1, 1));

        let review_after_revert: (i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                    (SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'k1'),
                    (SELECT review_count FROM review_states WHERE knowledge_unit_id = 'k1'),
                    (SELECT mastery_score FROM review_states WHERE knowledge_unit_id = 'k2'),
                    (SELECT review_count FROM review_states WHERE knowledge_unit_id = 'k2')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(review_after_revert, (82, 5, 31, 1));
    }

    #[test]
    fn merge_rejects_stale_preview_revision() {
        let mut connection = merge_database();
        connection
            .execute(
                "UPDATE knowledge_units SET updated_at = 201 WHERE id = 'k2'",
                [],
            )
            .unwrap();
        let error = apply_merge_connection(&mut connection, "c1", "k1", 100, 200).unwrap_err();
        assert!(error.contains("changed after preview"));
        let merges: i64 = connection
            .query_row("SELECT COUNT(*) FROM knowledge_merges", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(merges, 0);
    }
}
