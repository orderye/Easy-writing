use super::mapper::query_list;
use rusqlite::{params, Connection, OptionalExtension, Result};
use shared_types::{ChatMessage, ChatSession};
use uuid::Uuid;

/// 会话列清单：第 7 列固定为 message_count（非列表查询时以 NULL 占位，
/// 由 map_session 归一化为 0）。列序与 map_session 严格对应。
const SESSION_COLS: &str =
    "id, novel_id, chapter_id, title, created_at, updated_at, NULL AS message_count";

/// 会话列清单（带真实消息条数），用于列表查询。
const SESSION_COLS_WITH_COUNT: &str = "s.id, s.novel_id, s.chapter_id, s.title, s.created_at, s.updated_at,
     (SELECT COUNT(*) FROM chat_message m WHERE m.session_id = s.id)";

/// 找到（或新建）某作品 + 某章节的对话会话。
/// chapter_id 为 None 时表示该作品下不区分章节的会话（写作台未选中章节）。
pub fn ensure_session(
    conn: &Connection,
    novel_id: &str,
    chapter_id: Option<&str>,
) -> Result<ChatSession> {
    if let Some(existing) = find_session(conn, novel_id, chapter_id)? {
        return Ok(existing);
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO chat_session (id, novel_id, chapter_id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
        params![id, novel_id, chapter_id, default_title(chapter_id)],
    )?;

    get_session(conn, &id).and_then(|opt| opt.ok_or(rusqlite::Error::QueryReturnedNoRows))
}

fn default_title(chapter_id: Option<&str>) -> String {
    match chapter_id {
        Some(_) => "本章对话".to_string(),
        None => "全书对话".to_string(),
    }
}

/// 按作品 + 章节定位会话；chapter_id 为 None 时只匹配 chapter_id IS NULL 的会话。
pub fn find_session(
    conn: &Connection,
    novel_id: &str,
    chapter_id: Option<&str>,
) -> Result<Option<ChatSession>> {
    let row = match chapter_id {
        Some(cid) => conn
            .query_row(
                &format!(
                    "SELECT {} FROM chat_session
                     WHERE novel_id = ?1 AND chapter_id = ?2
                     ORDER BY updated_at DESC, rowid DESC
                     LIMIT 1",
                    SESSION_COLS
                ),
                params![novel_id, cid],
                map_session,
            )
            .optional()?,
        None => conn
            .query_row(
                &format!(
                    "SELECT {} FROM chat_session
                     WHERE novel_id = ?1 AND chapter_id IS NULL
                     ORDER BY updated_at DESC, rowid DESC
                     LIMIT 1",
                    SESSION_COLS
                ),
                params![novel_id],
                map_session,
            )
            .optional()?,
    };
    Ok(row)
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Option<ChatSession>> {
    conn.query_row(
        &format!("SELECT {} FROM chat_session WHERE id = ?1", SESSION_COLS),
        params![id],
        map_session,
    )
    .optional()
}

/// 列出作品下的全部会话，可按章节筛选（chapter_id = Some 时只列该章节的会话）。
/// 附带每个会话的消息条数，供前端会话列表显示。
pub fn list_sessions(
    conn: &Connection,
    novel_id: &str,
    chapter_id: Option<&str>,
) -> Result<Vec<ChatSession>> {
    match chapter_id {
        Some(cid) => query_list(
            conn,
            &format!(
                "SELECT {} FROM chat_session s
                 WHERE s.novel_id = ?1 AND s.chapter_id = ?2
                 ORDER BY s.updated_at DESC, s.rowid DESC",
                SESSION_COLS_WITH_COUNT
            ),
            params![novel_id, cid],
            map_session,
        ),
        None => query_list(
            conn,
            &format!(
                "SELECT {} FROM chat_session s
                 WHERE s.novel_id = ?1
                 ORDER BY s.updated_at DESC, s.rowid DESC",
                SESSION_COLS_WITH_COUNT
            ),
            params![novel_id],
            map_session,
        ),
    }
}

pub fn delete_session(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM chat_message WHERE session_id = ?1", params![id])?;
    conn.execute("DELETE FROM chat_session WHERE id = ?1", params![id])?;
    Ok(())
}

/// 追加一条消息，并顺带把所属会话的 updated_at 推进，便于按最近活跃排序。
pub fn append_message(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
) -> Result<ChatMessage> {
    if role != "user" && role != "assistant" {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "非法的对话角色: {}",
            role
        )));
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO chat_message (id, session_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        params![id, session_id, role, content],
    )?;
    conn.execute(
        "UPDATE chat_session SET updated_at = datetime('now') WHERE id = ?1",
        params![session_id],
    )?;

    get_message(conn, &id).and_then(|opt| opt.ok_or(rusqlite::Error::QueryReturnedNoRows))
}

pub fn get_message(conn: &Connection, id: &str) -> Result<Option<ChatMessage>> {
    conn.query_row(
        "SELECT id, session_id, role, content, created_at FROM chat_message WHERE id = ?1",
        params![id],
        map_message,
    )
    .optional()
}

/// 按时间正序读取会话消息；limit 为 0 表示不限制。
pub fn list_messages(conn: &Connection, session_id: &str, limit: i64) -> Result<Vec<ChatMessage>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at
         FROM chat_message
         WHERE session_id = ?1
         ORDER BY created_at ASC, rowid ASC",
    )?;

    let mapped = stmt.query_map(params![session_id], map_message)?;
    let mut list: Vec<ChatMessage> = Vec::new();
    for r in mapped {
        list.push(r?);
    }

    if limit > 0 && list.len() as i64 > limit {
        let drop_count = list.len() as i64 - limit;
        list.drain(..drop_count as usize);
    }
    Ok(list)
}

/// 清空会话消息，但保留会话本身。
pub fn clear_messages(conn: &Connection, session_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM chat_message WHERE session_id = ?1",
        params![session_id],
    )?;
    conn.execute(
        "UPDATE chat_session SET updated_at = datetime('now') WHERE id = ?1",
        params![session_id],
    )?;
    Ok(())
}

/// 会话映射器。第 7 列（下标 6）为可选的 message_count：
/// 列查询时读出真实条数，非列查询时为 NULL → 0。两个变体合并为一个。
fn map_session(row: &rusqlite::Row<'_>) -> Result<ChatSession> {
    Ok(ChatSession {
        id: row.get(0)?,
        novel_id: row.get(1)?,
        chapter_id: row.get(2)?,
        title: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        message_count: row.get::<_, Option<i32>>(6)?.unwrap_or(0),
    })
}

fn map_message(row: &rusqlite::Row<'_>) -> Result<ChatMessage> {
    Ok(ChatMessage {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        created_at: row.get(4)?,
    })
}
