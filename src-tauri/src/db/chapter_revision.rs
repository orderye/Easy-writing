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
    create_revision_with_batch(
        conn,
        chapter_id,
        novel_id,
        title,
        content,
        content_plain,
        word_count,
        revision_type,
        note,
        None,
    )
}

/// 带批次标识的快照。
///
/// `batch_id` 由批量替换（T09）写入：同一轮替换为每章写的快照共享一个 id，
/// 「整批撤销」靠它一次找回全部。普通快照传 `None`。
pub fn create_revision_with_batch(
    conn: &Connection,
    chapter_id: &str,
    novel_id: &str,
    title: &str,
    content: &str,
    content_plain: &str,
    word_count: i32,
    revision_type: &str,
    note: &str,
    batch_id: Option<&str>,
) -> Result<ChapterRevision> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO chapter_revision (id, chapter_id, novel_id, title, content, content_plain, word_count, revision_type, note, batch_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))",
        params![id, chapter_id, novel_id, title, content, content_plain, word_count, revision_type, note, batch_id],
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

/// 每章自动快照的默认上限。
///
/// 旧实现把 `30` 写死在清理语句里，作者无法调整。任务书 T03 ② 要求的默认值是
/// `50`，这里按任务书取值，并把上限本身变成设置项。
pub const DEFAULT_SNAPSHOT_LIMIT: usize = 50;

/// 设置项键名（存 `app_setting` 表）。
pub const SETTING_SNAPSHOT_LIMIT: &str = "revision_snapshot_limit";

/// 读取每章自动快照上限。缺失或非法时回落到默认值。
pub fn snapshot_limit(conn: &Connection) -> usize {
    match crate::db::app_setting::get_setting(conn, SETTING_SNAPSHOT_LIMIT) {
        Ok(Some(raw)) => raw.trim().parse::<usize>().unwrap_or(DEFAULT_SNAPSHOT_LIMIT),
        _ => DEFAULT_SNAPSHOT_LIMIT,
    }
}

/// 内容指纹：章节正文的 SHA-256。
///
/// 只用 `content_plain`（纯文本），不用 Tiptap JSON —— JSON 里含有
/// ProseMirror 生成的稳定 id 与标记顺序，格式上的抖动不该被当成「内容变了」。
fn content_fingerprint(content_plain: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content_plain.as_bytes());
    format!("{:x}", hasher.finalize())
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
            // 内容指纹相同 → 这一轮没有新内容，不值得为它再存一份快照。
            //
            // 少了这一条，「反复点保存」「切换章节又切回来」这类无内容变化的
            // 操作会不断堆出体积完全相同的快照 —— 它们既不提供任何回退价值，
            // 又让历史版本列表变成一串无法分辨的重复项。
            if content_fingerprint(&prev.content_plain) == content_fingerprint(content_plain) {
                return Ok(None);
            }

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

        // Prune auto-snapshots: retain the latest N auto snapshots for this chapter.
        //
        // 只清 `revision_type = 'auto'`：手动快照、修改前/后快照与回滚前快照
        // 都是作者显式动作或安全网的一部分，绝不能被自动清理带走。
        let limit = snapshot_limit(conn);
        let _ = conn.execute(
            "DELETE FROM chapter_revision
             WHERE chapter_id = ?1
               AND revision_type = 'auto'
               AND id NOT IN (
                   SELECT id FROM chapter_revision
                   WHERE chapter_id = ?1 AND revision_type = 'auto'
                   ORDER BY created_at DESC, rowid DESC
                   LIMIT ?2
               )",
            params![chapter_id, limit as i64],
        );

        Ok(Some(rev))
    } else {
        Ok(None)
    }
}
