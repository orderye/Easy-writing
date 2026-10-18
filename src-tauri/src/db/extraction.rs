use rusqlite::{params, Connection, Result};
use shared_types::ExtractionCandidate;
use uuid::Uuid;

pub fn insert_candidate(
    conn: &Connection,
    novel_id: &str,
    chapter_id: &str,
    agent_id: &str,
    kind: &str,
    action: &str,
    target_id: Option<&str>,
    title: &str,
    payload: &str,
    evidence: &str,
    dedup_key: &str,
) -> Result<ExtractionCandidate> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO extraction_candidate (id, novel_id, chapter_id, agent_id, kind, action, target_id, title, payload, evidence, dedup_key, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pending')",
        params![id, novel_id, chapter_id, agent_id, kind, action, target_id, title, payload, evidence, dedup_key],
    )?;

    Ok(ExtractionCandidate {
        id,
        novel_id: novel_id.to_string(),
        chapter_id: chapter_id.to_string(),
        agent_id: agent_id.to_string(),
        kind: kind.to_string(),
        action: action.to_string(),
        target_id: target_id.map(|s| s.to_string()),
        title: title.to_string(),
        payload: payload.to_string(),
        evidence: evidence.to_string(),
        dedup_key: dedup_key.to_string(),
        status: "pending".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn list_pending_candidates(conn: &Connection, novel_id: &str) -> Result<Vec<ExtractionCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, chapter_id, agent_id, kind, action, target_id, COALESCE(title, ''), payload, evidence, dedup_key, status, created_at
         FROM extraction_candidate
         WHERE novel_id = ?1 AND status = 'pending'
         ORDER BY created_at DESC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(ExtractionCandidate {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            chapter_id: row.get(2)?,
            agent_id: row.get(3)?,
            kind: row.get(4)?,
            action: row.get(5)?,
            target_id: row.get(6)?,
            title: row.get(7)?,
            payload: row.get(8)?,
            evidence: row.get(9)?,
            dedup_key: row.get(10)?,
            status: row.get(11)?,
            created_at: row.get(12)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn update_candidate_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE extraction_candidate SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    Ok(())
}

pub fn is_candidate_ignored_or_accepted(conn: &Connection, novel_id: &str, dedup_key: &str) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM extraction_candidate 
         WHERE novel_id = ?1 AND dedup_key = ?2 AND status IN ('pending', 'accepted', 'ignored')",
        params![novel_id, dedup_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
