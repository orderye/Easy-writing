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
        &format!(
            "SELECT {} FROM chapter 
             WHERE novel_id = ?1 AND deleted_at IS NULL 
             ORDER BY sort_order ASC, created_at ASC",
            CHAPTER_HEADER_COLS
        ),
        params![novel_id],
        map_chapter_header,
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
