use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQualityStateRecord {
    pub knowledge_unit_id: String,
    pub status: String,
    pub reason: Option<String>,
    pub updated_by: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimEvidenceRecord {
    pub evidence_id: String,
    pub source_id: String,
    pub text: String,
    pub start_offset: Option<i64>,
    pub end_offset: Option<i64>,
    pub stance: String,
    pub confidence: Option<f64>,
    pub created_by: String,
    pub source_platform: String,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub source_author: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecord {
    pub id: String,
    pub knowledge_unit_id: String,
    pub text: String,
    pub claim_type: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub evidence: Vec<ClaimEvidenceRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceLinkRecord {
    pub source_id: String,
    pub role: String,
    pub platform: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub captured_at: i64,
}

pub(crate) fn load_quality_state(
    connection: &Connection,
    knowledge_unit_id: &str,
) -> Result<KnowledgeQualityStateRecord, String> {
    connection
        .query_row(
            "SELECT knowledge_unit_id, status, reason, updated_by, updated_at
             FROM knowledge_quality_states WHERE knowledge_unit_id = ?1",
            [knowledge_unit_id],
            |row| {
                Ok(KnowledgeQualityStateRecord {
                    knowledge_unit_id: row.get(0)?,
                    status: row.get(1)?,
                    reason: row.get(2)?,
                    updated_by: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to query knowledge quality state: {error}"))?
        .ok_or_else(|| "knowledge quality state not found".to_string())
}

pub(crate) fn load_claims(
    connection: &Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<ClaimRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at
             FROM claims
             WHERE knowledge_unit_id = ?1 AND retired_at IS NULL
             ORDER BY CASE claim_type WHEN 'primary' THEN 0 ELSE 1 END, created_at ASC, rowid ASC",
        )
        .map_err(|error| format!("failed to prepare claim query: {error}"))?;
    let rows = statement
        .query_map([knowledge_unit_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|error| format!("failed to query claims: {error}"))?;

    let mut claims = Vec::new();
    for row in rows {
        let (id, knowledge_unit_id, text, claim_type, created_by, created_at, updated_at) =
            row.map_err(|error| format!("failed to read claim: {error}"))?;
        claims.push(ClaimRecord {
            evidence: load_claim_evidence(connection, &id)?,
            id,
            knowledge_unit_id,
            text,
            claim_type,
            created_by,
            created_at,
            updated_at,
        });
    }
    Ok(claims)
}

pub(crate) fn load_source_links(
    connection: &Connection,
    knowledge_unit_id: &str,
) -> Result<Vec<KnowledgeSourceLinkRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT l.source_id, l.role, s.platform, s.url, s.title, s.author, s.captured_at
             FROM knowledge_source_links l
             JOIN sources s ON s.id = l.source_id
             WHERE l.knowledge_unit_id = ?1
             ORDER BY CASE l.role WHEN 'origin' THEN 0 WHEN 'supporting' THEN 1 ELSE 2 END,
                      l.created_at ASC, s.captured_at ASC",
        )
        .map_err(|error| format!("failed to prepare knowledge source query: {error}"))?;
    let rows = statement
        .query_map([knowledge_unit_id], |row| {
            Ok(KnowledgeSourceLinkRecord {
                source_id: row.get(0)?,
                role: row.get(1)?,
                platform: row.get(2)?,
                url: row.get(3)?,
                title: row.get(4)?,
                author: row.get(5)?,
                captured_at: row.get(6)?,
            })
        })
        .map_err(|error| format!("failed to query knowledge sources: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read knowledge sources: {error}"))
}

pub(crate) fn link_primary_evidence(
    connection: &Connection,
    knowledge_unit_id: &str,
    evidence_ids: &[String],
    created_at: i64,
) -> Result<(), String> {
    let claim_id = format!("primary:{knowledge_unit_id}");
    let exists = connection
        .query_row(
            "SELECT 1 FROM claims WHERE id = ?1 AND retired_at IS NULL",
            [&claim_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| format!("failed to inspect primary claim: {error}"))?
        .is_some();
    if !exists {
        return Err("primary claim was not created before evidence linking".into());
    }

    for evidence_id in evidence_ids {
        connection
            .execute(
                "INSERT OR IGNORE INTO claim_evidence(
                    claim_id, evidence_id, stance, confidence, created_by, created_at
                 ) VALUES (?1, ?2, 'supports', 1.0, 'internalization', ?3)",
                params![claim_id, evidence_id, created_at],
            )
            .map_err(|error| format!("failed to link evidence to primary claim: {error}"))?;
    }
    Ok(())
}

fn load_claim_evidence(
    connection: &Connection,
    claim_id: &str,
) -> Result<Vec<ClaimEvidenceRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT e.id, e.source_id, e.text, e.start_offset, e.end_offset,
                    ce.stance, ce.confidence, ce.created_by,
                    s.platform, s.url, s.title, s.author
             FROM claim_evidence ce
             JOIN evidence e ON e.id = ce.evidence_id
             JOIN sources s ON s.id = e.source_id
             WHERE ce.claim_id = ?1
             ORDER BY CASE ce.stance WHEN 'supports' THEN 0 ELSE 1 END, ce.created_at ASC, e.rowid ASC",
        )
        .map_err(|error| format!("failed to prepare claim evidence query: {error}"))?;
    let rows = statement
        .query_map([claim_id], |row| {
            Ok(ClaimEvidenceRecord {
                evidence_id: row.get(0)?,
                source_id: row.get(1)?,
                text: row.get(2)?,
                start_offset: row.get(3)?,
                end_offset: row.get(4)?,
                stance: row.get(5)?,
                confidence: row.get(6)?,
                created_by: row.get(7)?,
                source_platform: row.get(8)?,
                source_url: row.get(9)?,
                source_title: row.get(10)?,
                source_author: row.get(11)?,
            })
        })
        .map_err(|error| format!("failed to query claim evidence: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read claim evidence: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_evidence_link_is_explicit_and_supporting() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE claims(id TEXT PRIMARY KEY, knowledge_unit_id TEXT NOT NULL, text TEXT NOT NULL, claim_type TEXT NOT NULL, created_by TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, retired_at INTEGER);
                 CREATE TABLE evidence(id TEXT PRIMARY KEY, knowledge_unit_id TEXT NOT NULL, source_id TEXT NOT NULL, text TEXT NOT NULL, start_offset INTEGER, end_offset INTEGER);
                 CREATE TABLE claim_evidence(claim_id TEXT NOT NULL, evidence_id TEXT NOT NULL, stance TEXT NOT NULL, confidence REAL, created_by TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(claim_id, evidence_id, stance), FOREIGN KEY(claim_id) REFERENCES claims(id), FOREIGN KEY(evidence_id) REFERENCES evidence(id));
                 INSERT INTO claims VALUES ('primary:k1', 'k1', 'Claim', 'primary', 'system', 1, 1, NULL);
                 INSERT INTO evidence VALUES ('e1', 'k1', 's1', 'quote', 0, 5);"
            )
            .unwrap();

        link_primary_evidence(&connection, "k1", &["e1".into()], 10).unwrap();
        let row: (String, f64, String) = connection
            .query_row(
                "SELECT stance, confidence, created_by FROM claim_evidence WHERE claim_id = 'primary:k1' AND evidence_id = 'e1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row.0, "supports");
        assert_eq!(row.1, 1.0);
        assert_eq!(row.2, "internalization");
    }
}
