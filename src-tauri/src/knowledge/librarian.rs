use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::database::{safety::audit_event, DatabaseState};

use super::{
    internalization::run_structured_with_repair, librarian_prompt::LIBRARIAN_SYSTEM_PROMPT, now_ms,
};

const MAX_CATALOG_ITEMS: i64 = 80;
const MAX_PROPOSALS: usize = 24;
const MIN_CONFIDENCE: f64 = 0.65;
const STALE_RUN_MS: i64 = 10 * 60 * 1000;
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
pub struct LibrarianProposalRecord {
    pub id: String,
    pub run_id: String,
    pub action_type: String,
    pub description: String,
    pub rationale: String,
    pub confidence: f64,
    pub status: String,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarianRunDetail {
    pub id: String,
    pub status: String,
    pub knowledge_count: i64,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub proposals: Vec<LibrarianProposalRecord>,
}

#[derive(Clone, Debug, Serialize)]
struct CatalogKnowledge {
    id: String,
    claim: String,
    concepts: Vec<String>,
    topics: Vec<String>,
    tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct CatalogRelation {
    source_id: String,
    target_id: String,
    relation_type: String,
}

#[derive(Clone, Debug, Serialize)]
struct LibrarianCatalog {
    knowledge: Vec<CatalogKnowledge>,
    existing_topics: Vec<String>,
    existing_tags: Vec<String>,
    existing_relations: Vec<CatalogRelation>,
}

#[derive(Clone, Debug, Deserialize)]
struct LibrarianOutput {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    proposals: Vec<ProposalSchema>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ProposalSchema {
    TopicAssign {
        knowledge_id: String,
        topic_name: String,
        rationale: String,
        confidence: f64,
    },
    TagAdd {
        knowledge_id: String,
        tag_name: String,
        rationale: String,
        confidence: f64,
    },
    RelationCreate {
        source_id: String,
        target_id: String,
        relation_type: String,
        rationale: String,
        confidence: f64,
    },
}

#[derive(Debug, Deserialize)]
struct TopicPayload {
    knowledge_id: String,
    topic_name: String,
}

#[derive(Debug, Deserialize)]
struct TagPayload {
    knowledge_id: String,
    tag_name: String,
}

#[derive(Debug, Deserialize)]
struct RelationPayload {
    source_id: String,
    target_id: String,
    relation_type: String,
}

#[derive(Debug)]
struct RawProposal {
    id: String,
    run_id: String,
    action_type: String,
    payload: Value,
    rationale: String,
    confidence: f64,
    status: String,
    created_at: i64,
    decided_at: Option<i64>,
}

#[tauri::command]
pub async fn librarian_analyze(app: AppHandle) -> Result<LibrarianRunDetail, String> {
    let (run_id, catalog) = {
        let state = app.state::<DatabaseState>();
        state.with_connection(|connection| {
            let now = now_ms();
            connection
                .execute(
                    "UPDATE librarian_runs SET status = 'failed', error = 'analysis interrupted', updated_at = ?1
                     WHERE status = 'running' AND updated_at < ?2",
                    params![now, now.saturating_sub(STALE_RUN_MS)],
                )
                .map_err(|error| format!("failed to recover stale librarian run: {error}"))?;
            let running: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM librarian_runs WHERE status = 'running'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| format!("failed to inspect librarian runs: {error}"))?;
            if running > 0 {
                return Err("knowledge organization is already running".into());
            }
            let catalog = load_catalog(connection)?;
            let run_id = Uuid::new_v4().to_string();
            connection
                .execute(
                    "INSERT INTO librarian_runs(id, status, knowledge_count, summary, error, created_at, updated_at)
                     VALUES (?1, 'running', ?2, NULL, NULL, ?3, ?3)",
                    params![run_id, catalog.knowledge.len() as i64, now],
                )
                .map_err(|error| format!("failed to create librarian run: {error}"))?;
            Ok((run_id, catalog))
        })?
    };

    if catalog.knowledge.is_empty() {
        return finish_empty_run(&app, &run_id, "暂无可整理的已内化知识。".into());
    }

    let system = LIBRARIAN_SYSTEM_PROMPT;
    let user = serde_json::to_string_pretty(&catalog)
        .map_err(|error| format!("failed to serialize librarian catalog: {error}"))?;

    let analyzed =
        run_structured_with_repair(&app, "knowledge_librarian", system, &user, |value| {
            validate_output(value, &catalog)
        })
        .await;

    match analyzed {
        Ok(output) => persist_output(&app, &run_id, output),
        Err(error) => {
            let state = app.state::<DatabaseState>();
            state.with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE librarian_runs SET status = 'failed', error = ?2, updated_at = ?3 WHERE id = ?1",
                        params![run_id, bounded_text(&error, 2000), now_ms()],
                    )
                    .map_err(|db_error| format!("failed to mark librarian run failed: {db_error}"))?;
                audit_event(connection, "librarian.run.failed", "librarian_run", Some(&run_id), None)?;
                Ok(())
            })?;
            let _ = app.emit(
                "librarian://changed",
                json!({ "runId": run_id, "status": "failed" }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub fn librarian_latest(
    state: State<'_, DatabaseState>,
) -> Result<Option<LibrarianRunDetail>, String> {
    state.with_connection(|connection| {
        let run_id = connection
            .query_row(
                "SELECT id FROM librarian_runs ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("failed to find latest librarian run: {error}"))?;
        run_id
            .map(|id| load_run_detail(connection, &id))
            .transpose()
    })
}

#[tauri::command]
pub fn librarian_proposal_apply(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    proposal_id: String,
) -> Result<LibrarianProposalRecord, String> {
    let proposal_id = required_id(&proposal_id, "proposal")?;
    let record = state.with_connection(|connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start librarian apply transaction: {error}"))?;
        let proposal = load_raw_proposal(&transaction, &proposal_id)?
            .ok_or_else(|| "librarian proposal not found".to_string())?;
        if proposal.status == "applied" {
            transaction
                .commit()
                .map_err(|error| format!("failed to finish idempotent librarian apply: {error}"))?;
            return load_proposal_record(connection, &proposal_id)?
                .ok_or_else(|| "applied librarian proposal could not be reloaded".to_string());
        }
        if proposal.status != "pending" {
            return Err("librarian proposal is no longer pending".into());
        }

        let outcome = match proposal.action_type.as_str() {
            "topic_assign" => apply_topic_proposal(&transaction, &proposal)?,
            "tag_add" => apply_tag_proposal(&transaction, &proposal)?,
            "relation_create" => apply_relation_proposal(&transaction, &proposal)?,
            _ => return Err("unsupported librarian action".into()),
        };
        let decided_at = now_ms();
        match outcome {
            ApplyOutcome::Applied(result) => {
                transaction
                    .execute(
                        "UPDATE librarian_proposals
                         SET status = 'applied', result_json = ?2, decided_at = ?3 WHERE id = ?1",
                        params![proposal.id, result.to_string(), decided_at],
                    )
                    .map_err(|error| {
                        format!("failed to mark librarian proposal applied: {error}")
                    })?;
                audit_event(
                    &transaction,
                    "librarian.proposal.apply",
                    "librarian_proposal",
                    Some(&proposal.id),
                    Some(json!({ "runId": proposal.run_id, "actionType": proposal.action_type })),
                )?;
            }
            ApplyOutcome::Stale(reason) => {
                transaction
                    .execute(
                        "UPDATE librarian_proposals
                         SET status = 'stale', result_json = ?2, decided_at = ?3 WHERE id = ?1",
                        params![
                            proposal.id,
                            json!({ "reason": reason }).to_string(),
                            decided_at
                        ],
                    )
                    .map_err(|error| format!("failed to mark librarian proposal stale: {error}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("failed to commit librarian proposal: {error}"))?;
        load_proposal_record(connection, &proposal_id)?
            .ok_or_else(|| "librarian proposal could not be reloaded".to_string())
    })?;

    let _ = app.emit("librarian://changed", json!({ "proposalId": proposal_id }));
    if record.status == "applied" {
        let _ = app.emit(
            "knowledge://changed",
            json!({ "reason": "librarian-apply" }),
        );
    }
    Ok(record)
}

#[tauri::command]
pub fn librarian_proposal_dismiss(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    proposal_id: String,
) -> Result<LibrarianProposalRecord, String> {
    let proposal_id = required_id(&proposal_id, "proposal")?;
    let record = state.with_connection(|connection| {
        let proposal = load_raw_proposal(connection, &proposal_id)?
            .ok_or_else(|| "librarian proposal not found".to_string())?;
        if proposal.status == "dismissed" {
            return load_proposal_record(connection, &proposal_id)?
                .ok_or_else(|| "dismissed librarian proposal could not be reloaded".to_string());
        }
        if proposal.status != "pending" {
            return Err("librarian proposal is no longer pending".into());
        }
        connection
            .execute(
                "UPDATE librarian_proposals SET status = 'dismissed', decided_at = ?2 WHERE id = ?1",
                params![proposal_id, now_ms()],
            )
            .map_err(|error| format!("failed to dismiss librarian proposal: {error}"))?;
        audit_event(
            connection,
            "librarian.proposal.dismiss",
            "librarian_proposal",
            Some(&proposal_id),
            Some(json!({ "runId": proposal.run_id, "actionType": proposal.action_type })),
        )?;
        load_proposal_record(connection, &proposal_id)?
            .ok_or_else(|| "dismissed librarian proposal could not be reloaded".to_string())
    })?;
    let _ = app.emit("librarian://changed", json!({ "proposalId": proposal_id }));
    Ok(record)
}

enum ApplyOutcome {
    Applied(Value),
    Stale(String),
}

fn apply_topic_proposal(
    connection: &Connection,
    proposal: &RawProposal,
) -> Result<ApplyOutcome, String> {
    let payload: TopicPayload = serde_json::from_value(proposal.payload.clone())
        .map_err(|error| format!("invalid topic proposal payload: {error}"))?;
    if !active_knowledge_exists(connection, &payload.knowledge_id)? {
        return Ok(ApplyOutcome::Stale("knowledge is no longer active".into()));
    }
    let topic_name = normalize_name(&payload.topic_name, 120)?;
    let existing = connection
        .query_row(
            "SELECT id, archived_at, deleted_at FROM topics WHERE name = ?1 COLLATE NOCASE",
            [&topic_name],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("failed to resolve librarian topic: {error}"))?;
    let (topic_id, created) = match existing {
        Some((id, None, None)) => (id, false),
        Some(_) => return Ok(ApplyOutcome::Stale("topic name is no longer active".into())),
        None => {
            let id = Uuid::new_v4().to_string();
            let now = now_ms();
            connection
                .execute(
                    "INSERT INTO topics(id, name, description, created_by, locked, created_at, updated_at)
                     VALUES (?1, ?2, '', 'agent', 0, ?3, ?3)",
                    params![id, topic_name, now],
                )
                .map_err(|error| format!("failed to create librarian topic: {error}"))?;
            (id, true)
        }
    };
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_topics WHERE knowledge_unit_id = ?1 AND topic_id = ?2",
            params![payload.knowledge_id, topic_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect topic assignment: {error}"))?;
    if exists > 0 {
        return Ok(ApplyOutcome::Stale("topic is already assigned".into()));
    }
    connection
        .execute(
            "INSERT INTO knowledge_topics(knowledge_unit_id, topic_id, created_by, created_at)
             VALUES (?1, ?2, 'agent', ?3)",
            params![payload.knowledge_id, topic_id, now_ms()],
        )
        .map_err(|error| format!("failed to apply librarian topic: {error}"))?;
    Ok(ApplyOutcome::Applied(json!({
        "knowledgeId": payload.knowledge_id,
        "topicId": topic_id,
        "topicName": topic_name,
        "createdTopic": created
    })))
}

fn apply_tag_proposal(
    connection: &Connection,
    proposal: &RawProposal,
) -> Result<ApplyOutcome, String> {
    let payload: TagPayload = serde_json::from_value(proposal.payload.clone())
        .map_err(|error| format!("invalid tag proposal payload: {error}"))?;
    if !active_knowledge_exists(connection, &payload.knowledge_id)? {
        return Ok(ApplyOutcome::Stale("knowledge is no longer active".into()));
    }
    let tag_name = normalize_name(&payload.tag_name, 80)?;
    let existing = connection
        .query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            [&tag_name],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to resolve librarian tag: {error}"))?;
    let (tag_id, created) = match existing {
        Some(id) => (id, false),
        None => {
            let id = Uuid::new_v4().to_string();
            let now = now_ms();
            connection
                .execute(
                    "INSERT INTO tags(id, name, created_by, created_at, updated_at)
                     VALUES (?1, ?2, 'agent', ?3, ?3)",
                    params![id, tag_name, now],
                )
                .map_err(|error| format!("failed to create librarian tag: {error}"))?;
            (id, true)
        }
    };
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_tags WHERE knowledge_unit_id = ?1 AND tag_id = ?2",
            params![payload.knowledge_id, tag_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect tag assignment: {error}"))?;
    if exists > 0 {
        return Ok(ApplyOutcome::Stale("tag is already assigned".into()));
    }
    connection
        .execute(
            "INSERT INTO knowledge_tags(knowledge_unit_id, tag_id, created_by, created_at)
             VALUES (?1, ?2, 'agent', ?3)",
            params![payload.knowledge_id, tag_id, now_ms()],
        )
        .map_err(|error| format!("failed to apply librarian tag: {error}"))?;
    Ok(ApplyOutcome::Applied(json!({
        "knowledgeId": payload.knowledge_id,
        "tagId": tag_id,
        "tagName": tag_name,
        "createdTag": created
    })))
}

fn apply_relation_proposal(
    connection: &Connection,
    proposal: &RawProposal,
) -> Result<ApplyOutcome, String> {
    let payload: RelationPayload = serde_json::from_value(proposal.payload.clone())
        .map_err(|error| format!("invalid relation proposal payload: {error}"))?;
    if !active_knowledge_exists(connection, &payload.source_id)?
        || !active_knowledge_exists(connection, &payload.target_id)?
    {
        return Ok(ApplyOutcome::Stale("knowledge is no longer active".into()));
    }
    if payload.source_id == payload.target_id {
        return Ok(ApplyOutcome::Stale(
            "relation endpoints are identical".into(),
        ));
    }
    let relation_type = payload.relation_type.trim().to_ascii_lowercase();
    if !RELATION_TYPES.contains(&relation_type.as_str()) {
        return Err("unsupported librarian relation type".into());
    }
    if relation_exists(
        connection,
        &payload.source_id,
        &payload.target_id,
        &relation_type,
    )? {
        return Ok(ApplyOutcome::Stale("relation already exists".into()));
    }
    let relation_id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO knowledge_relations(
                id, source_knowledge_id, target_knowledge_id, relation_type,
                confidence, created_by, confirmed, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'agent', 1, ?6)",
            params![
                relation_id,
                payload.source_id,
                payload.target_id,
                relation_type,
                proposal.confidence,
                now_ms()
            ],
        )
        .map_err(|error| format!("failed to apply librarian relation: {error}"))?;
    Ok(ApplyOutcome::Applied(json!({
        "relationId": relation_id,
        "sourceId": payload.source_id,
        "targetId": payload.target_id,
        "relationType": relation_type
    })))
}

fn persist_output(
    app: &AppHandle,
    run_id: &str,
    output: LibrarianOutput,
) -> Result<LibrarianRunDetail, String> {
    let state = app.state::<DatabaseState>();
    let detail = state.with_connection(|connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start librarian persistence transaction: {error}"))?;
        let now = now_ms();
        transaction
            .execute(
                "UPDATE librarian_proposals
                 SET status = 'stale', result_json = ?2, decided_at = ?3
                 WHERE status = 'pending' AND run_id <> ?1",
                params![
                    run_id,
                    json!({ "reason": "superseded by newer analysis" }).to_string(),
                    now
                ],
            )
            .map_err(|error| format!("failed to supersede older librarian proposals: {error}"))?;
        for proposal in output.proposals {
            let (action_type, payload, rationale, confidence) = proposal_parts(proposal);
            transaction
                .execute(
                    "INSERT INTO librarian_proposals(
                        id, run_id, action_type, payload_json, rationale, confidence, status, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)",
                    params![Uuid::new_v4().to_string(), run_id, action_type, payload.to_string(), rationale, confidence, now],
                )
                .map_err(|error| format!("failed to save librarian proposal: {error}"))?;
        }
        let summary = bounded_text(&output.summary, 600);
        transaction
            .execute(
                "UPDATE librarian_runs SET status = 'ready', summary = ?2, error = NULL, updated_at = ?3 WHERE id = ?1",
                params![run_id, summary, now],
            )
            .map_err(|error| format!("failed to complete librarian run: {error}"))?;
        let proposal_count: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM librarian_proposals WHERE run_id = ?1",
                [run_id],
                |row| row.get(0),
            )
            .map_err(|error| format!("failed to count librarian proposals: {error}"))?;
        audit_event(
            &transaction,
            "librarian.run.ready",
            "librarian_run",
            Some(run_id),
            Some(json!({ "proposalCount": proposal_count })),
        )?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit librarian run: {error}"))?;
        load_run_detail(connection, run_id)
    })?;
    let _ = app.emit(
        "librarian://changed",
        json!({ "runId": run_id, "status": "ready" }),
    );
    Ok(detail)
}

fn finish_empty_run(
    app: &AppHandle,
    run_id: &str,
    summary: String,
) -> Result<LibrarianRunDetail, String> {
    let state = app.state::<DatabaseState>();
    let detail = state.with_connection(|connection| {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start empty librarian transaction: {error}"))?;
        let now = now_ms();
        transaction
            .execute(
                "UPDATE librarian_proposals
                 SET status = 'stale', result_json = ?2, decided_at = ?3
                 WHERE status = 'pending' AND run_id <> ?1",
                params![run_id, json!({ "reason": "superseded by newer analysis" }).to_string(), now],
            )
            .map_err(|error| format!("failed to supersede older librarian proposals: {error}"))?;
        transaction
            .execute(
                "UPDATE librarian_runs SET status = 'ready', summary = ?2, updated_at = ?3 WHERE id = ?1",
                params![run_id, summary, now],
            )
            .map_err(|error| format!("failed to finish empty librarian run: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit empty librarian run: {error}"))?;
        load_run_detail(connection, run_id)
    })?;
    let _ = app.emit(
        "librarian://changed",
        json!({ "runId": run_id, "status": "ready" }),
    );
    Ok(detail)
}

fn validate_output(
    mut output: LibrarianOutput,
    catalog: &LibrarianCatalog,
) -> Result<LibrarianOutput, String> {
    output.summary = bounded_text(&output.summary, 600);
    if output.proposals.len() > MAX_PROPOSALS {
        return Err(format!(
            "librarian returned more than {MAX_PROPOSALS} proposals"
        ));
    }
    let ids: HashSet<&str> = catalog
        .knowledge
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let existing_relations: HashSet<String> = catalog
        .existing_relations
        .iter()
        .map(|item| relation_key(&item.source_id, &item.target_id, &item.relation_type))
        .collect();
    let mut seen = HashSet::new();
    let mut validated = Vec::new();
    for proposal in output.proposals {
        let normalized = match proposal {
            ProposalSchema::TopicAssign {
                knowledge_id,
                topic_name,
                rationale,
                confidence,
            } => {
                require_known_id(&ids, &knowledge_id)?;
                validate_confidence(confidence)?;
                if confidence < MIN_CONFIDENCE {
                    continue;
                }
                let topic_name = normalize_name(&topic_name, 120)?;
                let item = catalog
                    .knowledge
                    .iter()
                    .find(|item| item.id == knowledge_id)
                    .unwrap();
                if item
                    .topics
                    .iter()
                    .any(|name| name.to_lowercase() == topic_name.to_lowercase())
                {
                    continue;
                }
                ProposalSchema::TopicAssign {
                    knowledge_id,
                    topic_name,
                    rationale: normalize_rationale(&rationale)?,
                    confidence,
                }
            }
            ProposalSchema::TagAdd {
                knowledge_id,
                tag_name,
                rationale,
                confidence,
            } => {
                require_known_id(&ids, &knowledge_id)?;
                validate_confidence(confidence)?;
                if confidence < MIN_CONFIDENCE {
                    continue;
                }
                let tag_name = normalize_name(&tag_name, 80)?;
                let item = catalog
                    .knowledge
                    .iter()
                    .find(|item| item.id == knowledge_id)
                    .unwrap();
                if item
                    .tags
                    .iter()
                    .any(|name| name.to_lowercase() == tag_name.to_lowercase())
                {
                    continue;
                }
                ProposalSchema::TagAdd {
                    knowledge_id,
                    tag_name,
                    rationale: normalize_rationale(&rationale)?,
                    confidence,
                }
            }
            ProposalSchema::RelationCreate {
                source_id,
                target_id,
                relation_type,
                rationale,
                confidence,
            } => {
                require_known_id(&ids, &source_id)?;
                require_known_id(&ids, &target_id)?;
                if source_id == target_id {
                    return Err("relation endpoints must differ".into());
                }
                validate_confidence(confidence)?;
                if confidence < MIN_CONFIDENCE {
                    continue;
                }
                let relation_type = relation_type.trim().to_ascii_lowercase();
                if !RELATION_TYPES.contains(&relation_type.as_str()) {
                    return Err(format!("unsupported relation type '{relation_type}'"));
                }
                if existing_relations.contains(&relation_key(
                    &source_id,
                    &target_id,
                    &relation_type,
                )) || is_symmetric_relation(&relation_type)
                    && existing_relations.contains(&relation_key(
                        &target_id,
                        &source_id,
                        &relation_type,
                    ))
                {
                    continue;
                }
                ProposalSchema::RelationCreate {
                    source_id,
                    target_id,
                    relation_type,
                    rationale: normalize_rationale(&rationale)?,
                    confidence,
                }
            }
        };
        let key = proposal_identity(&normalized);
        if seen.insert(key) {
            validated.push(normalized);
        }
    }
    output.proposals = validated;
    Ok(output)
}

fn proposal_identity(proposal: &ProposalSchema) -> String {
    match proposal {
        ProposalSchema::TopicAssign {
            knowledge_id,
            topic_name,
            ..
        } => format!(
            "topic\u{1f}{knowledge_id}\u{1f}{}",
            topic_name.to_lowercase()
        ),
        ProposalSchema::TagAdd {
            knowledge_id,
            tag_name,
            ..
        } => format!("tag\u{1f}{knowledge_id}\u{1f}{}", tag_name.to_lowercase()),
        ProposalSchema::RelationCreate {
            source_id,
            target_id,
            relation_type,
            ..
        } => {
            if is_symmetric_relation(relation_type) {
                let (left, right) = if source_id <= target_id {
                    (source_id, target_id)
                } else {
                    (target_id, source_id)
                };
                format!("relation\u{1f}{left}\u{1f}{right}\u{1f}{relation_type}")
            } else {
                format!("relation\u{1f}{source_id}\u{1f}{target_id}\u{1f}{relation_type}")
            }
        }
    }
}

fn proposal_parts(proposal: ProposalSchema) -> (&'static str, Value, String, f64) {
    match proposal {
        ProposalSchema::TopicAssign {
            knowledge_id,
            topic_name,
            rationale,
            confidence,
        } => (
            "topic_assign",
            json!({ "knowledge_id": knowledge_id, "topic_name": topic_name }),
            rationale,
            confidence,
        ),
        ProposalSchema::TagAdd {
            knowledge_id,
            tag_name,
            rationale,
            confidence,
        } => (
            "tag_add",
            json!({ "knowledge_id": knowledge_id, "tag_name": tag_name }),
            rationale,
            confidence,
        ),
        ProposalSchema::RelationCreate {
            source_id,
            target_id,
            relation_type,
            rationale,
            confidence,
        } => (
            "relation_create",
            json!({ "source_id": source_id, "target_id": target_id, "relation_type": relation_type }),
            rationale,
            confidence,
        ),
    }
}

fn load_catalog(connection: &Connection) -> Result<LibrarianCatalog, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, core_claim, concepts_json FROM knowledge_units
             WHERE deleted_at IS NULL AND archived_at IS NULL AND status <> 'captured' AND trim(core_claim) <> ''
             ORDER BY updated_at DESC LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare librarian catalog: {error}"))?;
    let rows = statement
        .query_map([MAX_CATALOG_ITEMS], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("failed to query librarian catalog: {error}"))?;
    let mut knowledge = Vec::new();
    for row in rows {
        let (id, claim, concepts_json) =
            row.map_err(|error| format!("failed to read librarian catalog: {error}"))?;
        knowledge.push(CatalogKnowledge {
            topics: query_assignment_names(
                connection,
                "topics",
                "knowledge_topics",
                "topic_id",
                &id,
            )?,
            tags: query_assignment_names(connection, "tags", "knowledge_tags", "tag_id", &id)?,
            id,
            claim: bounded_text(&claim, 1200),
            concepts: serde_json::from_str::<Vec<String>>(&concepts_json)
                .unwrap_or_default()
                .into_iter()
                .take(16)
                .collect(),
        });
    }
    Ok(LibrarianCatalog {
        knowledge,
        existing_topics: query_global_names(
            connection,
            "topics",
            Some("deleted_at IS NULL AND archived_at IS NULL"),
        )?,
        existing_tags: query_global_names(connection, "tags", None)?,
        existing_relations: query_existing_relations(connection)?,
    })
}

fn query_assignment_names(
    connection: &Connection,
    table: &str,
    join_table: &str,
    join_column: &str,
    knowledge_id: &str,
) -> Result<Vec<String>, String> {
    let sql = format!(
        "SELECT item.name FROM {table} item JOIN {join_table} link ON link.{join_column} = item.id
         WHERE link.knowledge_unit_id = ?1 ORDER BY lower(item.name) ASC"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("failed to prepare librarian names: {error}"))?;
    let rows = statement
        .query_map([knowledge_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query librarian names: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read librarian names: {error}"))
}

fn query_global_names(
    connection: &Connection,
    table: &str,
    predicate: Option<&str>,
) -> Result<Vec<String>, String> {
    let where_clause = predicate
        .map(|value| format!(" WHERE {value}"))
        .unwrap_or_default();
    let sql = format!("SELECT name FROM {table}{where_clause} ORDER BY lower(name) ASC");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("failed to prepare librarian vocabulary: {error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query librarian vocabulary: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read librarian vocabulary: {error}"))
}

fn query_existing_relations(connection: &Connection) -> Result<Vec<CatalogRelation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT kr.source_knowledge_id, kr.target_knowledge_id, kr.relation_type
             FROM knowledge_relations kr
             JOIN knowledge_units source ON source.id = kr.source_knowledge_id AND source.deleted_at IS NULL
             JOIN knowledge_units target ON target.id = kr.target_knowledge_id AND target.deleted_at IS NULL",
        )
        .map_err(|error| format!("failed to prepare librarian relations: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(CatalogRelation {
                source_id: row.get(0)?,
                target_id: row.get(1)?,
                relation_type: row.get(2)?,
            })
        })
        .map_err(|error| format!("failed to query librarian relations: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read librarian relations: {error}"))
}

fn load_run_detail(connection: &Connection, run_id: &str) -> Result<LibrarianRunDetail, String> {
    let mut detail = connection
        .query_row(
            "SELECT id, status, knowledge_count, summary, error, created_at, updated_at FROM librarian_runs WHERE id = ?1",
            [run_id],
            |row| {
                Ok(LibrarianRunDetail {
                    id: row.get(0)?, status: row.get(1)?, knowledge_count: row.get(2)?, summary: row.get(3)?,
                    error: row.get(4)?, created_at: row.get(5)?, updated_at: row.get(6)?, proposals: Vec::new(),
                })
            },
        )
        .map_err(|error| format!("failed to load librarian run: {error}"))?;
    let mut statement = connection
        .prepare(
            "SELECT id FROM librarian_proposals WHERE run_id = ?1
             ORDER BY CASE status WHEN 'pending' THEN 0 WHEN 'applied' THEN 1 ELSE 2 END, confidence DESC, created_at ASC",
        )
        .map_err(|error| format!("failed to prepare librarian proposals: {error}"))?;
    let ids = statement
        .query_map([run_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query librarian proposals: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read librarian proposal ids: {error}"))?;
    for id in ids {
        if let Some(proposal) = load_proposal_record(connection, &id)? {
            detail.proposals.push(proposal);
        }
    }
    Ok(detail)
}

fn load_raw_proposal(
    connection: &Connection,
    proposal_id: &str,
) -> Result<Option<RawProposal>, String> {
    connection
        .query_row(
            "SELECT id, run_id, action_type, payload_json, rationale, confidence, status, created_at, decided_at
             FROM librarian_proposals WHERE id = ?1",
            [proposal_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?, row.get::<_, f64>(5)?, row.get::<_, String>(6)?, row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("failed to load librarian proposal: {error}"))?
        .map(|(id, run_id, action_type, payload_json, rationale, confidence, status, created_at, decided_at)| {
            let payload = serde_json::from_str(&payload_json)
                .map_err(|error| format!("invalid stored librarian payload: {error}"))?;
            Ok(RawProposal { id, run_id, action_type, payload, rationale, confidence, status, created_at, decided_at })
        })
        .transpose()
}

fn load_proposal_record(
    connection: &Connection,
    proposal_id: &str,
) -> Result<Option<LibrarianProposalRecord>, String> {
    let Some(raw) = load_raw_proposal(connection, proposal_id)? else {
        return Ok(None);
    };
    let description = proposal_description(connection, &raw.action_type, &raw.payload)?;
    Ok(Some(LibrarianProposalRecord {
        id: raw.id,
        run_id: raw.run_id,
        action_type: raw.action_type,
        description,
        rationale: raw.rationale,
        confidence: raw.confidence,
        status: raw.status,
        created_at: raw.created_at,
        decided_at: raw.decided_at,
    }))
}

fn proposal_description(
    connection: &Connection,
    action_type: &str,
    payload: &Value,
) -> Result<String, String> {
    match action_type {
        "topic_assign" => {
            let payload: TopicPayload =
                serde_json::from_value(payload.clone()).map_err(|error| error.to_string())?;
            Ok(format!(
                "将「{}」加入主题「{}」",
                knowledge_label(connection, &payload.knowledge_id)?,
                payload.topic_name
            ))
        }
        "tag_add" => {
            let payload: TagPayload =
                serde_json::from_value(payload.clone()).map_err(|error| error.to_string())?;
            Ok(format!(
                "给「{}」添加 #{}",
                knowledge_label(connection, &payload.knowledge_id)?,
                payload.tag_name
            ))
        }
        "relation_create" => {
            let payload: RelationPayload =
                serde_json::from_value(payload.clone()).map_err(|error| error.to_string())?;
            Ok(format!(
                "「{}」 → {} → 「{}」",
                knowledge_label(connection, &payload.source_id)?,
                relation_label(&payload.relation_type),
                knowledge_label(connection, &payload.target_id)?
            ))
        }
        _ => Ok("未知整理建议".into()),
    }
}

fn knowledge_label(connection: &Connection, knowledge_id: &str) -> Result<String, String> {
    let claim = connection
        .query_row(
            "SELECT core_claim FROM knowledge_units WHERE id = ?1",
            [knowledge_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to label librarian knowledge: {error}"))?;
    Ok(claim
        .filter(|value| !value.trim().is_empty())
        .map(|value| bounded_text(&value, 90))
        .unwrap_or_else(|| format!("知识 {}", &knowledge_id[..knowledge_id.len().min(8)])))
}

fn active_knowledge_exists(connection: &Connection, knowledge_id: &str) -> Result<bool, String> {
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_units
             WHERE id = ?1 AND deleted_at IS NULL AND archived_at IS NULL AND status <> 'captured'",
            [knowledge_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to validate librarian knowledge: {error}"))?;
    Ok(exists > 0)
}

fn relation_exists(
    connection: &Connection,
    source_id: &str,
    target_id: &str,
    relation_type: &str,
) -> Result<bool, String> {
    let exact: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_relations WHERE source_knowledge_id = ?1 AND target_knowledge_id = ?2 AND relation_type = ?3",
            params![source_id, target_id, relation_type], |row| row.get(0),
        )
        .map_err(|error| format!("failed to inspect librarian relation: {error}"))?;
    if exact > 0 {
        return Ok(true);
    }
    if is_symmetric_relation(relation_type) {
        let reverse: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relations WHERE source_knowledge_id = ?1 AND target_knowledge_id = ?2 AND relation_type = ?3",
                params![target_id, source_id, relation_type], |row| row.get(0),
            )
            .map_err(|error| format!("failed to inspect reverse librarian relation: {error}"))?;
        return Ok(reverse > 0);
    }
    Ok(false)
}

fn relation_key(source_id: &str, target_id: &str, relation_type: &str) -> String {
    format!("{source_id}\u{1f}{target_id}\u{1f}{relation_type}")
}

fn is_symmetric_relation(relation_type: &str) -> bool {
    matches!(relation_type, "related_to" | "contradicts")
}

fn relation_label(relation_type: &str) -> &'static str {
    match relation_type {
        "supports" => "支持",
        "contradicts" => "冲突",
        "example_of" => "示例",
        "prerequisite_of" => "前置",
        "derived_from" => "来源于",
        "extends" => "扩展",
        _ => "相关",
    }
}

fn require_known_id(ids: &HashSet<&str>, value: &str) -> Result<(), String> {
    if !ids.contains(value) {
        return Err(format!(
            "proposal references unknown knowledge id '{value}'"
        ));
    }
    Ok(())
}

fn validate_confidence(value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("proposal confidence must be between 0 and 1".into());
    }
    Ok(())
}

fn normalize_name(value: &str, max_chars: usize) -> Result<String, String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return Err("proposal name is empty".into());
    }
    if value.chars().count() > max_chars {
        return Err("proposal name is too long".into());
    }
    Ok(value)
}

fn normalize_rationale(value: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err("proposal rationale is empty".into());
    }
    if value.chars().count() > 500 {
        return Err("proposal rationale is too long".into());
    }
    Ok(value)
}

fn required_id(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 160 {
        return Err(format!("invalid {label} id"));
    }
    Ok(value.to_string())
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> LibrarianCatalog {
        LibrarianCatalog {
            knowledge: vec![
                CatalogKnowledge {
                    id: "k1".into(),
                    claim: "A supports B".into(),
                    concepts: vec!["A".into()],
                    topics: vec!["已有主题".into()],
                    tags: vec!["旧标签".into()],
                },
                CatalogKnowledge {
                    id: "k2".into(),
                    claim: "B detail".into(),
                    concepts: vec!["B".into()],
                    topics: vec![],
                    tags: vec![],
                },
            ],
            existing_topics: vec!["已有主题".into()],
            existing_tags: vec!["旧标签".into()],
            existing_relations: vec![],
        }
    }

    #[test]
    fn validator_rejects_unknown_ids() {
        let raw = r#"{"summary":"x","proposals":[{"action":"tag_add","knowledge_id":"missing","tag_name":"test","rationale":"why","confidence":0.9}]}"#;
        let output = serde_json::from_str::<LibrarianOutput>(raw).unwrap();
        assert!(validate_output(output, &catalog())
            .unwrap_err()
            .contains("unknown knowledge id"));
    }

    #[test]
    fn validator_filters_existing_and_low_confidence_suggestions() {
        let raw = r#"{"summary":"x","proposals":[
          {"action":"topic_assign","knowledge_id":"k1","topic_name":"已有主题","rationale":"same","confidence":0.9},
          {"action":"tag_add","knowledge_id":"k2","tag_name":"新标签","rationale":"weak","confidence":0.4},
          {"action":"relation_create","source_id":"k1","target_id":"k2","relation_type":"supports","rationale":"clear","confidence":0.9}
        ]}"#;
        let output = serde_json::from_str::<LibrarianOutput>(raw).unwrap();
        let output = validate_output(output, &catalog()).unwrap();
        assert_eq!(output.proposals.len(), 1);
        assert!(matches!(
            output.proposals[0],
            ProposalSchema::RelationCreate { .. }
        ));
    }

    #[test]
    fn validator_rejects_destructive_or_unknown_action() {
        let raw = r#"{"summary":"x","proposals":[{"action":"trash","knowledge_id":"k1","rationale":"bad","confidence":1.0}]}"#;
        assert!(serde_json::from_str::<LibrarianOutput>(raw).is_err());
    }
    #[test]
    fn validator_deduplicates_same_semantic_action() {
        let raw = r#"{"summary":"x","proposals":[
          {"action":"tag_add","knowledge_id":"k2","tag_name":"Agent","rationale":"first","confidence":0.91},
          {"action":"tag_add","knowledge_id":"k2","tag_name":"agent","rationale":"second","confidence":0.82},
          {"action":"relation_create","source_id":"k1","target_id":"k2","relation_type":"related_to","rationale":"a","confidence":0.9},
          {"action":"relation_create","source_id":"k2","target_id":"k1","relation_type":"related_to","rationale":"b","confidence":0.88}
        ]}"#;
        let output = serde_json::from_str::<LibrarianOutput>(raw).unwrap();
        let output = validate_output(output, &catalog()).unwrap();
        assert_eq!(output.proposals.len(), 2);
    }

    fn executor_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE knowledge_units(
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL DEFAULT 'learning',
                    archived_at INTEGER,
                    deleted_at INTEGER
                 );
                 CREATE TABLE topics(
                    id TEXT PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    description TEXT NOT NULL DEFAULT '', created_by TEXT NOT NULL,
                    locked INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL, archived_at INTEGER, deleted_at INTEGER
                 );
                 CREATE TABLE knowledge_topics(
                    knowledge_unit_id TEXT NOT NULL, topic_id TEXT NOT NULL,
                    created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
                    PRIMARY KEY(knowledge_unit_id, topic_id)
                 );
                 CREATE TABLE tags(
                    id TEXT PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    created_by TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
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
                 INSERT INTO knowledge_units(id, archived_at, deleted_at) VALUES ('k1', NULL, NULL), ('k2', NULL, NULL);"
            )
            .unwrap();
        connection
    }

    fn raw_proposal(action_type: &str, payload: Value, confidence: f64) -> RawProposal {
        RawProposal {
            id: "p1".into(),
            run_id: "r1".into(),
            action_type: action_type.into(),
            payload,
            rationale: "test".into(),
            confidence,
            status: "pending".into(),
            created_at: 0,
            decided_at: None,
        }
    }

    #[test]
    fn executor_applies_topic_and_tag_as_agent_changes() {
        let connection = executor_connection();
        let topic = raw_proposal(
            "topic_assign",
            json!({ "knowledge_id": "k1", "topic_name": "Agent 工程" }),
            0.92,
        );
        assert!(matches!(
            apply_topic_proposal(&connection, &topic).unwrap(),
            ApplyOutcome::Applied(_)
        ));
        let topic_creator: String = connection
            .query_row(
                "SELECT created_by FROM topics WHERE name = 'Agent 工程'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let topic_link_creator: String = connection
            .query_row(
                "SELECT created_by FROM knowledge_topics WHERE knowledge_unit_id = 'k1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(topic_creator, "agent");
        assert_eq!(topic_link_creator, "agent");

        let tag = raw_proposal(
            "tag_add",
            json!({ "knowledge_id": "k1", "tag_name": "工作流" }),
            0.86,
        );
        assert!(matches!(
            apply_tag_proposal(&connection, &tag).unwrap(),
            ApplyOutcome::Applied(_)
        ));
        let tag_creator: String = connection
            .query_row(
                "SELECT created_by FROM tags WHERE name = '工作流'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_creator, "agent");
    }

    #[test]
    fn executor_applies_relation_with_confirmed_agent_metadata() {
        let connection = executor_connection();
        let relation = raw_proposal(
            "relation_create",
            json!({ "source_id": "k1", "target_id": "k2", "relation_type": "supports" }),
            0.93,
        );
        assert!(matches!(
            apply_relation_proposal(&connection, &relation).unwrap(),
            ApplyOutcome::Applied(_)
        ));
        let stored: (String, i64, f64) = connection
            .query_row(
                "SELECT created_by, confirmed, confidence FROM knowledge_relations",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(stored.0, "agent");
        assert_eq!(stored.1, 1);
        assert!((stored.2 - 0.93).abs() < f64::EPSILON);
    }
    #[test]
    fn executor_stales_proposal_when_knowledge_is_archived() {
        let connection = executor_connection();
        connection
            .execute(
                "UPDATE knowledge_units SET archived_at = 1 WHERE id = 'k1'",
                [],
            )
            .unwrap();
        let tag = raw_proposal(
            "tag_add",
            json!({ "knowledge_id": "k1", "tag_name": "不应写入" }),
            0.9,
        );
        assert!(matches!(
            apply_tag_proposal(&connection, &tag).unwrap(),
            ApplyOutcome::Stale(_)
        ));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
