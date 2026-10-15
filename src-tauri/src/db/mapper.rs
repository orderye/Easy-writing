//! db 层共用助手：列清单常量、行映射器与查询样板。
//!
//! 消重目标（原先散落在各实体模块里逐字复制的形态）：
//! 1. `list_X` 与 `get_X` 各自内联一份完全相同的 `|row| Ok(X { row.get(0)?, .. })` 闭包；
//! 2. `if let Some(res) = rows.next() { Ok(Some(res?)) } else { Ok(None) }` 三段式（30 处）；
//! 3. `let mut v = Vec::new(); for r in rows { v.push(r?); } Ok(v)` 收集循环（20 处）。
//!
//! 把这三者收敛到本模块后，列顺序与映射器绑定在一处：改 schema 时改
//! SELECT 常量与对应 mapper 即可，不会再出现「改了 SELECT 忘了改 row.get(5)」
//! 这类静默错位。

use rusqlite::{Connection, Result};

/// 收集 query_map 的全部行；遇错即返回（替代 20 处手写 push 循环）。
pub fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> Result<T>>,
) -> Result<Vec<T>> {
    rows.collect()
}

/// 取第一行；无行则返回 None（替代 30 处 `rows.next()` 三段式）。
pub fn first_row<T>(
    rows: &mut rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> Result<T>>,
) -> Result<Option<T>> {
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// 查单行可选记录：prepare + query_map + first_row 三步合一。
pub fn query_optional<T, P, F>(
    conn: &Connection,
    sql: &str,
    params: P,
    mapper: F,
) -> Result<Option<T>>
where
    P: rusqlite::Params,
    F: FnMut(&rusqlite::Row<'_>) -> Result<T>,
{
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query_map(params, mapper)?;
    first_row(&mut rows)
}

/// 查多行记录：prepare + query_map + collect 三步合一。
pub fn query_list<T, P, F>(conn: &Connection, sql: &str, params: P, mapper: F) -> Result<Vec<T>>
where
    P: rusqlite::Params,
    F: FnMut(&rusqlite::Row<'_>) -> Result<T>,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, mapper)?;
    collect_rows(rows)
}

/// SQLite 的 INTEGER 布尔列转 bool（原先每处都写 `let x: i32 = row.get(n)?; x == 1`）。
pub fn int_to_bool(v: i32) -> bool {
    v == 1
}

// ---------- 各实体的列清单常量 ----------
// 与各 mapper 的 row.get 序号严格对应，改列时两处一起改。

pub const NOVEL_COLS: &str =
    "id, title, synopsis, created_at, updated_at, deleted_at";
pub const VOLUME_COLS: &str =
    "id, novel_id, title, summary, sort_order, created_at, updated_at, deleted_at";
pub const CHAPTER_HEADER_COLS: &str =
    "id, novel_id, volume_id, title, summary, word_count, status, sort_order, updated_at";
pub const CHAPTER_COLS: &str =
    "id, novel_id, volume_id, title, summary, content, content_plain, word_count, status,      extracted_hash, sort_order, created_at, updated_at, deleted_at";
pub const AGENT_COLS: &str =
    "id, key, name, system_prompt, llm_profile_id, params, enabled, created_at, updated_at";
pub const LLM_PROFILE_COLS: &str =
    "id, name, protocol, base_url, key_ref, model, params, is_default, created_at, updated_at";
pub const CHARACTER_RELATION_COLS: &str =
    "id, novel_id, from_char_id, to_char_id, relation_name, description, created_at, updated_at";
pub const REVISION_HEADER_COLS: &str =
    "id, chapter_id, novel_id, title, word_count, revision_type, note, created_at";
pub const REVISION_COLS: &str =
    "id, chapter_id, novel_id, title, content, content_plain, word_count, revision_type, note, created_at";
