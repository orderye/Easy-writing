use super::mapper::{query_list, query_optional, CHAPTER_COLS, CHAPTER_HEADER_COLS};
use rusqlite::{params, Connection, Result};
use shared_types::{Chapter, ChapterHeader};
use uuid::Uuid;

fn map_chapter_header(row: &rusqlite::Row<'_>) -> Result<ChapterHeader> {
    Ok(ChapterHeader {
        id: row.get(0)?,
        novel_id: row.get(1)?,
        volume_id: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        word_count: row.get(5)?,
        status: row.get(6)?,
        sort_order: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_chapter(row: &rusqlite::Row<'_>) -> Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        novel_id: row.get(1)?,
        volume_id: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        content: row.get(5)?,
        content_plain: row.get(6)?,
        word_count: row.get(7)?,
        status: row.get(8)?,
        extracted_hash: row.get(9)?,
        sort_order: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        deleted_at: row.get(13)?,
    })
}

pub fn create_chapter(
    conn: &Connection,
    novel_id: &str,
    volume_id: &str,
    title: &str,
    summary: &str,
) -> Result<Chapter> {
    let id = Uuid::new_v4().to_string();

    let max_order: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM chapter WHERE volume_id = ?1 AND deleted_at IS NULL",
        params![volume_id],
        |row| row.get(0),
    ).unwrap_or(-1);

    conn.execute(
        "INSERT INTO chapter (id, novel_id, volume_id, title, summary, content, content_plain, word_count, status, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, '{}', '', 0, 'draft', ?6)",
        params![id, novel_id, volume_id, title, summary, max_order + 1],
    )?;

    get_chapter(conn, &id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn list_chapter_headers(conn: &Connection, volume_id: &str) -> Result<Vec<ChapterHeader>> {
    query_list(
        conn,
        &format!(
            "SELECT {} FROM chapter 
             WHERE volume_id = ?1 AND deleted_at IS NULL 
             ORDER BY sort_order ASC, created_at ASC",
            CHAPTER_HEADER_COLS
        ),
        params![volume_id],
        map_chapter_header,
    )
}

pub fn list_all_chapter_headers_for_novel(conn: &Connection, novel_id: &str) -> Result<Vec<ChapterHeader>> {
    query_list(
        conn,
        "SELECT c.id, c.novel_id, c.volume_id, c.title, c.summary, c.word_count, c.status, c.sort_order, c.updated_at
         FROM chapter c
         LEFT JOIN volume v ON c.volume_id = v.id
         WHERE c.novel_id = ?1 AND c.deleted_at IS NULL
         ORDER BY COALESCE(v.sort_order, 0) ASC, c.sort_order ASC, c.created_at ASC",
        params![novel_id],
        map_chapter_header,
    )
}

/// 一次取出整部作品的章节**正文**，按卷序 → 章序排列。
///
/// 导出要用到正文（`content` / `content_plain`），而
/// `list_all_chapter_headers_for_novel` 只取表头。300 章的书逐章
/// `get_chapter` 就是 300 次往返，这里一次查询解决。
///
/// 排序与 `list_all_chapter_headers_for_novel` 保持一致，
/// 保证「目录里的顺序」与「导出的顺序」是同一个。
pub fn list_chapters_for_export(conn: &Connection, novel_id: &str) -> Result<Vec<Chapter>> {
    query_list(
        conn,
        "SELECT c.id, c.novel_id, c.volume_id, c.title, c.summary, c.content, c.content_plain,
                c.word_count, c.status, c.extracted_hash, c.sort_order, c.created_at, c.updated_at, c.deleted_at
         FROM chapter c
         LEFT JOIN volume v ON c.volume_id = v.id
         WHERE c.novel_id = ?1 AND c.deleted_at IS NULL
         ORDER BY COALESCE(v.sort_order, 0) ASC, c.sort_order ASC, c.created_at ASC",
        params![novel_id],
        map_chapter,
    )
}

pub fn get_chapter(conn: &Connection, id: &str) -> Result<Option<Chapter>> {
    query_optional(
        conn,
        &format!(
            "SELECT {} FROM chapter WHERE id = ?1 AND deleted_at IS NULL",
            CHAPTER_COLS
        ),
        params![id],
        map_chapter,
    )
}

pub fn save_chapter_content(
    conn: &Connection,
    id: &str,
    content: &str,
    content_plain: &str,
    word_count: i32,
) -> Result<()> {
    conn.execute(
        "UPDATE chapter 
         SET content = ?1, content_plain = ?2, word_count = ?3, updated_at = datetime('now') 
         WHERE id = ?4",
        params![content, content_plain, word_count, id],
    )?;
    Ok(())
}

/// 找出正文里提到过某个词的全部章节（按命中次数降序）。
///
/// 这是「改动早期设定后，哪些后续章节可能受影响」的**确定性**版本：
/// 只做字面匹配，不调模型。它的价值在于先把候选面收窄 ——
/// 有没有真的需要同步修改，交给作者或后续的模型判断。
///
/// 用 LIKE 先粗筛（能走不到索引，但章节量级下可接受），
/// 再在 Rust 侧数出现次数并截取上下文，避免在 SQL 里做子串计数。
pub fn find_chapters_mentioning(
    conn: &Connection,
    novel_id: &str,
    term: &str,
) -> Result<Vec<shared_types::AffectedChapter>> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT id, title, content_plain FROM chapter
         WHERE novel_id = ?1 AND deleted_at IS NULL
           AND content_plain LIKE '%' || ?2 || '%'
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id, term], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let plain: String = row.get(2)?;
        Ok((id, title, plain))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id, title, plain) = row?;
        let hits = plain.matches(term).count() as i64;
        if hits == 0 {
            // LIKE 已经筛过，理论上不会走到这里；留个保护免得出现"命中 0 次"的脏结果
            continue;
        }
        out.push(shared_types::AffectedChapter {
            id,
            title,
            hits,
            snippet: snippet_around(&plain, term, 40),
        });
    }

    out.sort_by(|a, b| b.hits.cmp(&a.hits).then(a.title.cmp(&b.title)));
    Ok(out)
}

/// 截取首次命中位置前后的上下文，按字符切（中文不能被切碎）。
fn snippet_around(text: &str, term: &str, radius: usize) -> String {
    let Some(byte_pos) = text.find(term) else {
        return String::new();
    };
    snippet_around_at(text, byte_pos, term, radius)
}

/// 截取**指定字节位置**命中处的上下文。
///
/// 全书搜索要在同一章里返回多处片段，不能只取第一次命中，
/// 因此把「按位置切片」单独提出来供 `db::search` 复用 ——
/// 两处各写一份的话，中文边界处理迟早会分叉。
pub fn snippet_around_at(text: &str, byte_pos: usize, term: &str, radius: usize) -> String {
    let start_chars = text[..byte_pos].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let from = start_chars.saturating_sub(radius);
    let to = (start_chars + term.chars().count() + radius).min(chars.len());
    let mut s: String = chars[from..to].iter().collect();
    if from > 0 {
        s.insert(0, '…');
    }
    if to < chars.len() {
        s.push('…');
    }
    s
}

pub fn update_chapter_meta(
    conn: &Connection,
    id: &str,
    title: &str,
    summary: &str,
    status: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE chapter 
         SET title = ?1, summary = ?2, status = ?3, updated_at = datetime('now') 
         WHERE id = ?4",
        params![title, summary, status, id],
    )?;
    Ok(())
}

pub fn move_chapter_to_volume(
    conn: &Connection,
    chapter_id: &str,
    new_volume_id: &str,
    new_sort_order: i32,
) -> Result<()> {
    conn.execute(
        "UPDATE chapter 
         SET volume_id = ?1, sort_order = ?2, updated_at = datetime('now') 
         WHERE id = ?3",
        params![new_volume_id, new_sort_order, chapter_id],
    )?;
    Ok(())
}

pub fn reorder_chapters(conn: &Connection, chapter_ids: &[String]) -> Result<()> {
    for (idx, id) in chapter_ids.iter().enumerate() {
        conn.execute(
            "UPDATE chapter SET sort_order = ?1 WHERE id = ?2",
            params![idx as i32, id],
        )?;
    }
    Ok(())
}

pub fn delete_chapter(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE chapter SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}
