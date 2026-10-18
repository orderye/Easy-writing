use super::mapper::{
    query_list, query_optional, REVISION_COLS, REVISION_HEADER_COLS,
};
use rusqlite::{params, Connection, Result};
use shared_types::{Chapter, ChapterRevision, ChapterRevisionHeader};
use uuid::Uuid;

fn map_revision_header(row: &rusqlite::Row<'_>) -> Result<ChapterRevisionHeader> {
    Ok(ChapterRevisionHeader {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        novel_id: row.get(2)?,
        title: row.get(3)?,
        word_count: row.get(4)?,
        revision_type: row.get(5)?,
        note: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_revision(row: &rusqlite::Row<'_>) -> Result<ChapterRevision> {
    Ok(ChapterRevision {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        novel_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        content_plain: row.get(5)?,
        word_count: row.get(6)?,
        revision_type: row.get(7)?,
        note: row.get(8)?,
        created_at: row.get(9)?,
    })
}

pub fn create_revision(
    conn: &Connection,
    chapter_id: &str,
    novel_id: &str,
    title: &str,
    content: &str,
    content_plain: &str,
    word_count: i32,
    revision_type: &str,
    note: &str,
) -> Result<ChapterRevision> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO chapter_revision (id, chapter_id, novel_id, title, content, content_plain, word_count, revision_type, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
        params![id, chapter_id, novel_id, title, content, content_plain, word_count, revision_type, note],
    )?;

    get_revision(conn, &id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn list_revisions_for_chapter(
    conn: &Connection,
    chapter_id: &str,
) -> Result<Vec<ChapterRevisionHeader>> {
    query_list(
        conn,
        &format!(
            "SELECT {} FROM chapter_revision 
             WHERE chapter_id = ?1 
             ORDER BY created_at DESC, rowid DESC 
             LIMIT 100",
            REVISION_HEADER_COLS
        ),
        params![chapter_id],
        map_revision_header,
    )
}

pub fn get_revision(conn: &Connection, id: &str) -> Result<Option<ChapterRevision>> {
    query_optional(
        conn,
        &format!(
            "SELECT {} FROM chapter_revision WHERE id = ?1",
            REVISION_COLS
        ),
        params![id],
        map_revision,
    )
}

pub fn get_latest_revision(conn: &Connection, chapter_id: &str) -> Result<Option<ChapterRevision>> {
    query_optional(
        conn,
        &format!(
            "SELECT {} FROM chapter_revision 
             WHERE chapter_id = ?1 
             ORDER BY created_at DESC, rowid DESC 
             LIMIT 1",
            REVISION_COLS
        ),
        params![chapter_id],
        map_revision,
    )
}

pub fn revert_chapter_to_revision(
    conn: &Connection,
    chapter_id: &str,
    revision_id: &str,
) -> Result<Chapter> {
    let target = get_revision(conn, revision_id)?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)?;

    let current = crate::db::chapter::get_chapter(conn, chapter_id)?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)?;

    // Backup current state as rollback_before if it has content and is different
    if !current.content_plain.is_empty() && current.content_plain != target.content_plain {
        let note = format!("回滚前自动备份 (即将回滚至 {})", target.created_at);
        let _ = create_revision(
            conn,
            chapter_id,
            &current.novel_id,
            &current.title,
            &current.content,
            &current.content_plain,
            current.word_count,
            "rollback_before",
            &note,
        );
    }

    // Revert chapter to target revision
    conn.execute(
        "UPDATE chapter 
         SET content = ?1, content_plain = ?2, word_count = ?3, updated_at = datetime('now') 
         WHERE id = ?4",
        params![target.content, target.content_plain, target.word_count, chapter_id],
    )?;

    crate::db::chapter::get_chapter(conn, chapter_id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn maybe_auto_snapshot(
    conn: &Connection,
    chapter_id: &str,
    novel_id: &str,
    title: &str,
    content: &str,
    content_plain: &str,
    word_count: i32,
) -> Result<Option<ChapterRevision>> {
    // If empty content, do not snapshot
    if content_plain.trim().is_empty() {
        return Ok(None);
    }

    let latest = get_latest_revision(conn, chapter_id)?;
    let should_snapshot = match latest {
        None => true, // First snapshot
        Some(prev) => {
            // Check diff in word count or time elapsed >= 5 mins
            let word_diff = (word_count - prev.word_count).abs();
            // SQLite datetime diff: check if at least 300 seconds passed
            let elapsed_secs: i64 = conn.query_row(
                "SELECT CAST((strftime('%s', 'now') - strftime('%s', ?1)) AS INTEGER)",
                params![prev.created_at],
                |row| row.get(0),
            ).unwrap_or(0);

            (elapsed_secs >= 300 && word_diff >= 50) || (elapsed_secs >= 1800 && word_diff >= 10)
        }
    };

    if should_snapshot {
        let rev = create_revision(
            conn,
            chapter_id,
            novel_id,
            title,
            content,
            content_plain,
            word_count,
            "auto",
            "自动保存快照",
        )?;

        // Prune auto-snapshots: retain latest 30 auto snapshots for this chapter
        let _ = conn.execute(
            "DELETE FROM chapter_revision 
             WHERE chapter_id = ?1 
               AND revision_type = 'auto' 
               AND id NOT IN (
                   SELECT id FROM chapter_revision 
                   WHERE chapter_id = ?1 AND revision_type = 'auto' 
                   ORDER BY created_at DESC, rowid DESC 
                   LIMIT 30
               )",
            params![chapter_id],
        );

        Ok(Some(rev))
    } else {
        Ok(None)
    }
}
