use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::State;

use crate::database::DatabaseState;

use super::now_ms;

const DEFAULT_NODE_LIMIT: u32 = 42;
const MAX_NODE_LIMIT: u32 = 72;
const MAX_GRAPH_EDGES: usize = 320;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub quality_status: String,
    pub platform: String,
    pub author: Option<String>,
    pub mastery_score: i64,
    pub review_count: i64,
    pub topic_names: Vec<String>,
    pub tag_names: Vec<String>,
    pub relation_count: i64,
    pub source_count: i64,
    pub updated_at: i64,
    pub is_center: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphEdge {
    pub id: String,
    pub source_knowledge_id: String,
    pub target_knowledge_id: String,
    pub kind: String,
    pub relation_type: Option<String>,
    pub label: String,
    pub reason: String,
    pub weight: f64,
    pub confidence: Option<f64>,
    pub created_by: Option<String>,
    pub confirmed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphSnapshot {
    pub center_knowledge_id: Option<String>,
    pub depth: u32,
    pub total_active: i64,
    pub truncated: bool,
    pub generated_at: i64,
    pub nodes: Vec<KnowledgeGraphNode>,
    pub edges: Vec<KnowledgeGraphEdge>,
}

#[derive(Clone, Debug)]
struct NeighborScore {
    id: String,
    score: f64,
}

#[tauri::command]
pub fn knowledge_graph(
    state: State<'_, DatabaseState>,
    center_knowledge_id: Option<String>,
    depth: Option<u32>,
    limit: Option<u32>,
) -> Result<KnowledgeGraphSnapshot, String> {
    let center = clean_optional(center_knowledge_id);
    let depth = depth.unwrap_or(2).clamp(1, 2);
    let limit = limit.unwrap_or(DEFAULT_NODE_LIMIT).clamp(8, MAX_NODE_LIMIT) as usize;
    state.with_connection(|connection| load_graph(connection, center.as_deref(), depth, limit))
}

fn load_graph(
    connection: &rusqlite::Connection,
    center: Option<&str>,
    depth: u32,
    limit: usize,
) -> Result<KnowledgeGraphSnapshot, String> {
    let total_active = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_units WHERE deleted_at IS NULL AND archived_at IS NULL AND status <> 'captured'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("failed to count graph knowledge: {error}"))?;

    let selected_ids = if let Some(center_id) = center {
        ensure_graph_knowledge(connection, center_id)?;
        select_local_nodes(connection, center_id, depth, limit)?
    } else {
        select_global_nodes(connection, limit)?
    };
    let selected = selected_ids.iter().cloned().collect::<HashSet<_>>();
    let mut nodes = selected_ids
        .iter()
        .map(|id| load_graph_node(connection, id, center == Some(id.as_str())))
        .collect::<Result<Vec<_>, _>>()?;
    let edges = load_graph_edges(connection, &selected)?;

    let degrees = edge_degrees(&edges);
    if center.is_none() {
        nodes.sort_by(|left, right| {
            degrees
                .get(&right.id)
                .copied()
                .unwrap_or(0)
                .cmp(&degrees.get(&left.id).copied().unwrap_or(0))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.id.cmp(&right.id))
        });
    }

    Ok(KnowledgeGraphSnapshot {
        center_knowledge_id: center.map(str::to_string),
        depth,
        total_active,
        truncated: total_active > nodes.len() as i64 && nodes.len() >= limit,
        generated_at: now_ms(),
        nodes,
        edges,
    })
}

fn select_global_nodes(
    connection: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT k.id
             FROM knowledge_units k
             WHERE k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured'
             ORDER BY
                ((SELECT COUNT(*) FROM knowledge_relations r
                  WHERE r.source_knowledge_id = k.id OR r.target_knowledge_id = k.id) * 100)
                + ((SELECT COUNT(*) FROM knowledge_source_links sl WHERE sl.knowledge_unit_id = k.id) * 12)
                + ((SELECT COUNT(*) FROM knowledge_topics kt WHERE kt.knowledge_unit_id = k.id) * 8)
                + ((SELECT COUNT(*) FROM knowledge_tags kg WHERE kg.knowledge_unit_id = k.id) * 3) DESC,
                k.updated_at DESC,
                k.rowid DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare global graph nodes: {error}"))?;
    let rows = statement
        .query_map([limit as i64], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query global graph nodes: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read global graph nodes: {error}"))?;
    Ok(rows)
}

fn select_local_nodes(
    connection: &rusqlite::Connection,
    center: &str,
    depth: u32,
    limit: usize,
) -> Result<Vec<String>, String> {
    let mut ordered = vec![center.to_string()];
    let mut selected = HashSet::from([center.to_string()]);
    let mut frontier = vec![center.to_string()];

    for _ in 0..depth {
        if ordered.len() >= limit || frontier.is_empty() {
            break;
        }
        let mut candidates = HashMap::<String, f64>::new();
        for knowledge_id in &frontier {
            for neighbor in load_neighbor_scores(connection, knowledge_id)? {
                if !selected.contains(&neighbor.id) {
                    *candidates.entry(neighbor.id).or_default() += neighbor.score;
                }
            }
        }
        let mut ranked = candidates.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let remaining = limit.saturating_sub(ordered.len());
        let next = ranked
            .into_iter()
            .take(remaining)
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in &next {
            selected.insert(id.clone());
            ordered.push(id.clone());
        }
        frontier = next;
    }
    Ok(ordered)
}

fn load_neighbor_scores(
    connection: &rusqlite::Connection,
    knowledge_id: &str,
) -> Result<Vec<NeighborScore>, String> {
    let mut scores = HashMap::<String, f64>::new();

    let mut relation_statement = connection
        .prepare(
            "SELECT CASE WHEN r.source_knowledge_id = ?1 THEN r.target_knowledge_id ELSE r.source_knowledge_id END,
                    COALESCE(r.confidence, 1.0)
             FROM knowledge_relations r
             JOIN knowledge_units other ON other.id = CASE
                WHEN r.source_knowledge_id = ?1 THEN r.target_knowledge_id ELSE r.source_knowledge_id END
             WHERE (r.source_knowledge_id = ?1 OR r.target_knowledge_id = ?1)
               AND other.deleted_at IS NULL AND other.archived_at IS NULL AND other.status <> 'captured'",
        )
        .map_err(|error| format!("failed to prepare graph relation neighbors: {error}"))?;
    let relation_rows = relation_statement
        .query_map([knowledge_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })
        .map_err(|error| format!("failed to query graph relation neighbors: {error}"))?;
    for row in relation_rows {
        let (id, confidence) =
            row.map_err(|error| format!("failed to read graph relation neighbor: {error}"))?;
        *scores.entry(id).or_default() += 100.0 + confidence.clamp(0.0, 1.0) * 10.0;
    }

    add_shared_neighbor_scores(
        connection,
        knowledge_id,
        "knowledge_source_links",
        "source_id",
        70.0,
        6.0,
        &mut scores,
    )?;
    add_shared_neighbor_scores(
        connection,
        knowledge_id,
        "knowledge_topics",
        "topic_id",
        50.0,
        4.0,
        &mut scores,
    )?;
    add_shared_neighbor_scores(
        connection,
        knowledge_id,
        "knowledge_tags",
        "tag_id",
        24.0,
        2.0,
        &mut scores,
    )?;

    Ok(scores
        .into_iter()
        .map(|(id, score)| NeighborScore { id, score })
        .collect())
}

fn add_shared_neighbor_scores(
    connection: &rusqlite::Connection,
    knowledge_id: &str,
    table: &str,
    column: &str,
    base_score: f64,
    extra_score: f64,
    scores: &mut HashMap<String, f64>,
) -> Result<(), String> {
    let sql = format!(
        "SELECT other.knowledge_unit_id, COUNT(*)
         FROM {table} mine
         JOIN {table} other ON other.{column} = mine.{column}
         JOIN knowledge_units k ON k.id = other.knowledge_unit_id
         WHERE mine.knowledge_unit_id = ?1
           AND other.knowledge_unit_id <> ?1
           AND k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured'
         GROUP BY other.knowledge_unit_id"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("failed to prepare graph shared neighbors: {error}"))?;
    let rows = statement
        .query_map([knowledge_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| format!("failed to query graph shared neighbors: {error}"))?;
    for row in rows {
        let (id, count) =
            row.map_err(|error| format!("failed to read graph shared neighbor: {error}"))?;
        *scores.entry(id).or_default() += base_score + (count.min(3) as f64 * extra_score);
    }
    Ok(())
}

fn load_graph_node(
    connection: &rusqlite::Connection,
    knowledge_id: &str,
    is_center: bool,
) -> Result<KnowledgeGraphNode, String> {
    let mut node = connection
        .query_row(
            "SELECT k.id,
                    COALESCE(NULLIF(trim(k.core_claim), ''), NULLIF(trim(s.title), ''), substr(s.selected_text, 1, 120)),
                    k.status, COALESCE(q.status, 'unverified'), s.platform, s.author,
                    COALESCE(r.mastery_score, 0), COALESCE(r.review_count, 0),
                    (SELECT COUNT(*) FROM knowledge_relations rel
                     WHERE rel.source_knowledge_id = k.id OR rel.target_knowledge_id = k.id),
                    (SELECT COUNT(*) FROM knowledge_source_links sl WHERE sl.knowledge_unit_id = k.id),
                    k.updated_at
             FROM knowledge_units k
             JOIN sources s ON s.id = k.source_id
             LEFT JOIN review_states r ON r.knowledge_unit_id = k.id
             LEFT JOIN knowledge_quality_states q ON q.knowledge_unit_id = k.id
             WHERE k.id = ?1 AND k.deleted_at IS NULL AND k.archived_at IS NULL AND k.status <> 'captured'",
            [knowledge_id],
            |row| {
                Ok(KnowledgeGraphNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    status: row.get(2)?,
                    quality_status: row.get(3)?,
                    platform: row.get(4)?,
                    author: row.get(5)?,
                    mastery_score: row.get(6)?,
                    review_count: row.get(7)?,
                    relation_count: row.get(8)?,
                    source_count: row.get(9)?,
                    updated_at: row.get(10)?,
                    topic_names: Vec::new(),
                    tag_names: Vec::new(),
                    is_center,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to load graph node: {error}"))?
        .ok_or_else(|| "graph knowledge unit not found".to_string())?;
    node.topic_names = list_names(
        connection,
        "SELECT t.name FROM knowledge_topics kt JOIN topics t ON t.id = kt.topic_id WHERE kt.knowledge_unit_id = ?1 AND t.deleted_at IS NULL ORDER BY lower(t.name)",
        knowledge_id,
    )?;
    node.tag_names = list_names(
        connection,
        "SELECT t.name FROM knowledge_tags kg JOIN tags t ON t.id = kg.tag_id WHERE kg.knowledge_unit_id = ?1 ORDER BY lower(t.name)",
        knowledge_id,
    )?;
    Ok(node)
}

fn load_graph_edges(
    connection: &rusqlite::Connection,
    selected: &HashSet<String>,
) -> Result<Vec<KnowledgeGraphEdge>, String> {
    let mut edges = Vec::new();
    let mut statement = connection
        .prepare(
            "SELECT r.id, r.source_knowledge_id, r.target_knowledge_id, r.relation_type,
                    r.confidence, r.created_by, r.confirmed
             FROM knowledge_relations r
             JOIN knowledge_units source ON source.id = r.source_knowledge_id
             JOIN knowledge_units target ON target.id = r.target_knowledge_id
             WHERE source.deleted_at IS NULL AND source.archived_at IS NULL AND source.status <> 'captured'
               AND target.deleted_at IS NULL AND target.archived_at IS NULL AND target.status <> 'captured'",
        )
        .map_err(|error| format!("failed to prepare graph relation edges: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)? != 0,
            ))
        })
        .map_err(|error| format!("failed to query graph relation edges: {error}"))?;
    for row in rows {
        let (id, source, target, relation_type, confidence, created_by, confirmed) =
            row.map_err(|error| format!("failed to read graph relation edge: {error}"))?;
        if selected.contains(&source) && selected.contains(&target) {
            edges.push(KnowledgeGraphEdge {
                id,
                source_knowledge_id: source,
                target_knowledge_id: target,
                kind: "relation".into(),
                label: relation_type.clone(),
                reason: relation_reason(&relation_type, &created_by, confidence),
                relation_type: Some(relation_type),
                weight: 1.0,
                confidence,
                created_by: Some(created_by),
                confirmed,
            });
        }
    }

    edges.extend(load_structural_edges(connection, selected, "source")?);
    edges.extend(load_structural_edges(connection, selected, "topic")?);
    edges.extend(load_structural_edges(connection, selected, "tag")?);
    edges.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| left.id.cmp(&right.id))
    });
    edges.truncate(MAX_GRAPH_EDGES);
    Ok(edges)
}

fn load_structural_edges(
    connection: &rusqlite::Connection,
    selected: &HashSet<String>,
    kind: &str,
) -> Result<Vec<KnowledgeGraphEdge>, String> {
    let (sql, weight) = match kind {
        "source" => (
            "SELECT l.knowledge_unit_id, l.source_id,
                    COALESCE(NULLIF(trim(s.title), ''), NULLIF(trim(s.author), ''), s.platform)
             FROM knowledge_source_links l
             JOIN sources s ON s.id = l.source_id",
            0.72,
        ),
        "topic" => (
            "SELECT kt.knowledge_unit_id, kt.topic_id, t.name
             FROM knowledge_topics kt JOIN topics t ON t.id = kt.topic_id
             WHERE t.deleted_at IS NULL",
            0.52,
        ),
        "tag" => (
            "SELECT kg.knowledge_unit_id, kg.tag_id, t.name
             FROM knowledge_tags kg JOIN tags t ON t.id = kg.tag_id",
            0.28,
        ),
        _ => return Err("unsupported graph structural edge kind".into()),
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("failed to prepare graph {kind} edges: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("failed to query graph {kind} edges: {error}"))?;
    let mut groups = BTreeMap::<String, (String, Vec<String>)>::new();
    for row in rows {
        let (knowledge_id, group_id, label) =
            row.map_err(|error| format!("failed to read graph {kind} edge: {error}"))?;
        if selected.contains(&knowledge_id) {
            let entry = groups
                .entry(group_id)
                .or_insert_with(|| (label, Vec::new()));
            if !entry.1.contains(&knowledge_id) {
                entry.1.push(knowledge_id);
            }
        }
    }

    let mut pairs = BTreeMap::<(String, String), Vec<String>>::new();
    for (_, (label, mut knowledge_ids)) in groups {
        knowledge_ids.sort();
        for left in 0..knowledge_ids.len() {
            for right in (left + 1)..knowledge_ids.len() {
                pairs
                    .entry((knowledge_ids[left].clone(), knowledge_ids[right].clone()))
                    .or_default()
                    .push(label.clone());
            }
        }
    }

    Ok(pairs
        .into_iter()
        .map(|((source, target), mut labels)| {
            labels.sort();
            labels.dedup();
            let visible_labels = labels.iter().take(3).cloned().collect::<Vec<_>>();
            let label = visible_labels.join(" · ");
            let reason = match kind {
                "source" => format!("共享来源：{label}"),
                "topic" => format!("共享主题：{label}"),
                _ => format!("共享标签：{label}"),
            };
            KnowledgeGraphEdge {
                id: format!("{kind}:{source}:{target}"),
                source_knowledge_id: source,
                target_knowledge_id: target,
                kind: kind.to_string(),
                relation_type: None,
                label,
                reason,
                weight,
                confidence: None,
                created_by: None,
                confirmed: true,
            }
        })
        .collect())
}

fn edge_degrees(edges: &[KnowledgeGraphEdge]) -> HashMap<String, usize> {
    let mut degrees = HashMap::new();
    for edge in edges {
        *degrees.entry(edge.source_knowledge_id.clone()).or_default() += 1;
        *degrees.entry(edge.target_knowledge_id.clone()).or_default() += 1;
    }
    degrees
}

fn relation_reason(relation_type: &str, created_by: &str, confidence: Option<f64>) -> String {
    let source = match created_by {
        "user" => "手动确认",
        "agent" => "AI 助手确认",
        "curation" => "知识整理确认",
        value if value.starts_with("merge:") => "合并继承",
        _ => "已确认关系",
    };
    if let Some(confidence) = confidence {
        format!(
            "{source} · {} · 置信度 {:.0}%",
            relation_type,
            confidence.clamp(0.0, 1.0) * 100.0
        )
    } else {
        format!("{source} · {relation_type}")
    }
}

fn list_names(
    connection: &rusqlite::Connection,
    sql: &str,
    knowledge_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("failed to prepare graph metadata: {error}"))?;
    let names = statement
        .query_map([knowledge_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query graph metadata: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read graph metadata: {error}"))?;
    Ok(names)
}

fn ensure_graph_knowledge(connection: &rusqlite::Connection, id: &str) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM knowledge_units WHERE id = ?1 AND deleted_at IS NULL AND archived_at IS NULL AND status <> 'captured'",
            [id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| format!("failed to validate graph knowledge: {error}"))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("knowledge unit is not available in the active graph".into())
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn graph_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE sources(id TEXT PRIMARY KEY, platform TEXT NOT NULL, title TEXT, author TEXT, selected_text TEXT NOT NULL);
                 CREATE TABLE knowledge_units(id TEXT PRIMARY KEY, source_id TEXT NOT NULL, core_claim TEXT NOT NULL, status TEXT NOT NULL, updated_at INTEGER NOT NULL, archived_at INTEGER, deleted_at INTEGER);
                 CREATE TABLE review_states(knowledge_unit_id TEXT PRIMARY KEY, mastery_score INTEGER NOT NULL DEFAULT 0, review_count INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE knowledge_quality_states(knowledge_unit_id TEXT PRIMARY KEY, status TEXT NOT NULL);
                 CREATE TABLE knowledge_relations(id TEXT PRIMARY KEY, source_knowledge_id TEXT NOT NULL, target_knowledge_id TEXT NOT NULL, relation_type TEXT NOT NULL, confidence REAL, created_by TEXT NOT NULL, confirmed INTEGER NOT NULL, created_at INTEGER NOT NULL);
                 CREATE TABLE knowledge_source_links(knowledge_unit_id TEXT NOT NULL, source_id TEXT NOT NULL, role TEXT NOT NULL);
                 CREATE TABLE topics(id TEXT PRIMARY KEY, name TEXT NOT NULL, deleted_at INTEGER);
                 CREATE TABLE knowledge_topics(knowledge_unit_id TEXT NOT NULL, topic_id TEXT NOT NULL);
                 CREATE TABLE tags(id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 CREATE TABLE knowledge_tags(knowledge_unit_id TEXT NOT NULL, tag_id TEXT NOT NULL);

                 INSERT INTO sources VALUES
                    ('s1','zhihu','A source','Alice','A excerpt'),
                    ('s2','web','B source','Bob','B excerpt'),
                    ('s3','web','C source','Cara','C excerpt'),
                    ('s4','web','D source','Dana','D excerpt');
                 INSERT INTO knowledge_units VALUES
                    ('k1','s1','Center claim','learning',100,NULL,NULL),
                    ('k2','s2','Explicit neighbor','reviewing',90,NULL,NULL),
                    ('k3','s3','Topic neighbor','weak',80,NULL,NULL),
                    ('k4','s4','Archived neighbor','mastered',70,99,NULL);
                 INSERT INTO review_states VALUES ('k1',55,2),('k2',75,4),('k3',30,1),('k4',95,9);
                 INSERT INTO knowledge_quality_states VALUES ('k1','verified'),('k2','unverified'),('k3','conflicted'),('k4','verified');
                 INSERT INTO knowledge_relations VALUES
                    ('r1','k1','k2','supports',0.9,'user',1,1),
                    ('r2','k1','k4','related_to',0.8,'agent',1,2);
                 INSERT INTO knowledge_source_links VALUES
                    ('k1','s1','origin'),('k2','s2','origin'),('k3','s3','origin'),('k4','s4','origin');
                 INSERT INTO topics VALUES ('t1','Shared topic',NULL);
                 INSERT INTO knowledge_topics VALUES ('k1','t1'),('k3','t1'),('k4','t1');
                 INSERT INTO tags VALUES ('g1','Shared tag');
                 INSERT INTO knowledge_tags VALUES ('k2','g1'),('k3','g1');"
            )
            .unwrap();
        connection
    }

    #[test]
    fn local_graph_combines_explicit_and_structural_links_without_archived_nodes() {
        let connection = graph_connection();
        let graph = load_graph(&connection, Some("k1"), 1, 20).unwrap();
        let ids = graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<HashSet<_>>();
        assert!(ids.contains("k1"));
        assert!(ids.contains("k2"));
        assert!(ids.contains("k3"));
        assert!(!ids.contains("k4"));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == "relation"
                && edge.source_knowledge_id == "k1"
                && edge.target_knowledge_id == "k2"
                && edge.relation_type.as_deref() == Some("supports")
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == "topic"
                && ((edge.source_knowledge_id == "k1" && edge.target_knowledge_id == "k3")
                    || (edge.source_knowledge_id == "k3" && edge.target_knowledge_id == "k1"))
        }));
        assert_eq!(graph.center_knowledge_id.as_deref(), Some("k1"));
    }

    #[test]
    fn local_graph_rejects_archived_center_and_respects_node_limit() {
        let connection = graph_connection();
        assert!(load_graph(&connection, Some("k4"), 2, 20).is_err());
        let graph = load_graph(&connection, Some("k1"), 2, 2).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].id, "k1");
        assert_eq!(graph.nodes[1].id, "k2");
    }
}
