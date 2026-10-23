use rusqlite::{params, Connection, Result};
use shared_types::ExtractionCandidate;
use uuid::Uuid;

/// 写入一条待审候选。
///
/// `possible_duplicate_of`（T07）为「疑似同人」锚点：既有角色 id 或另一条候选 id。
#[allow(clippy::too_many_arguments)]
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
    possible_duplicate_of: Option<&str>,
) -> Result<ExtractionCandidate> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO extraction_candidate (id, novel_id, chapter_id, agent_id, kind, action, target_id, title, payload, evidence, dedup_key, possible_duplicate_of, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'pending')",
        params![id, novel_id, chapter_id, agent_id, kind, action, target_id, title, payload, evidence, dedup_key, possible_duplicate_of],
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
        possible_duplicate_of: possible_duplicate_of.map(|s| s.to_string()),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn list_pending_candidates(conn: &Connection, novel_id: &str) -> Result<Vec<ExtractionCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, chapter_id, agent_id, kind, action, target_id, COALESCE(title, ''), payload, evidence, dedup_key, status, possible_duplicate_of, created_at
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
            possible_duplicate_of: row.get(12)?,
            created_at: row.get(13)?,
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

/// 把一条「疑似同人」的角色候选合并进既有角色（T07）。
///
/// 语义与采纳不同：这里**不新建角色**，而是把候选标题写入目标角色的
/// `aliases`（去重），并把候选描述并入角色卡的 `raw_notes`，最后把候选
/// 标记为 `accepted`。这样作者确认「小舟就是林舟」后，
/// 库里不会多出一个重复角色，而已有的角色卡也拿到了新信息。
///
/// 返回合并后的角色。
pub fn merge_character_candidate(
    conn: &Connection,
    candidate_id: &str,
    target_character_id: &str,
) -> std::result::Result<shared_types::Character, String> {
    let (novel_id, kind, title, payload): (String, String, String, String) = conn
        .query_row(
            "SELECT novel_id, kind, COALESCE(title, ''), payload FROM extraction_candidate WHERE id = ?1",
            params![candidate_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| format!("候选不存在：{e}"))?;

    if kind != "character" {
        return Err("只有角色候选支持合并到既有角色".to_string());
    }

    let existing = crate::db::settings::list_characters(conn, &novel_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == target_character_id)
        .ok_or_else(|| "目标角色不存在或已删除".to_string())?;

    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("候选标题为空，无法作为别名合并".to_string());
    }

    // 别名去重按归一化比较，避免「小舟 / 小舟 」这种纯空白差异堆两条。
    let mut aliases: Vec<String> = serde_json::from_str(&existing.aliases).unwrap_or_default();
    let norm_title = crate::agent::extraction::normalize_title(&title);
    let already = !norm_title.is_empty()
        && (crate::agent::extraction::normalize_title(&existing.name) == norm_title
            || aliases
                .iter()
                .any(|a| crate::agent::extraction::normalize_title(a) == norm_title));
    if !already {
        aliases.push(title.clone());
    }

    let mut ep = shared_types::ExplodedProfile::parse_from_str(&existing.profile);
    if ep.raw_notes.is_empty() {
        ep.raw_notes = payload.clone();
    } else if !payload.trim().is_empty() && !ep.raw_notes.contains(&payload) {
        ep.raw_notes.push_str("\n\n---\n");
        ep.raw_notes.push_str(&payload);
    }

    let aliases_json = serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string());
    let merged = crate::db::settings::save_character(
        conn,
        Some(existing.id.clone()),
        &novel_id,
        &existing.name,
        &aliases_json,
        &ep.to_json_string(),
    )
    .map_err(|e| e.to_string())?;

    // 新名称进词库，编辑器里输入「小舟」也能补全。
    let _ = crate::db::lexicon::upsert_lexicon_term(
        conn,
        &novel_id,
        &title,
        "character",
        Some(&existing.id),
    );

    update_candidate_status(conn, candidate_id, "accepted").map_err(|e| e.to_string())?;
    Ok(merged)
}