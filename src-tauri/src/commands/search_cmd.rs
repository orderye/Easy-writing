//! 全书搜索与替换命令（任务书 T09）。
//!
//! 四个命令，刻意拆开而不是一个大命令：
//!
//! 1. `search_chapters` —— 纯读，前端搜索框防抖调用；
//! 2. `preview_replace` —— 替换前先看「会改哪些章、各改几处」；
//! 3. `apply_replace` —— 执行替换，先快照后改写，同一事务；
//! 4. `undo_replace_batch` —— 按 `batch_id` 整批撤销。
//!
//! 拆开的原因与导出命令一致：预览是**读**，执行是**写**，
//! 混在一个命令里，「预览了但没执行」和「执行了没看到结果」都没法表达。

use shared_types::{ChapterSearchHit, ReplacePreviewItem, ReplaceReport, UndoReplaceReport};
use tauri::State;

use crate::db::DbPool;

/// 全书搜索。
///
/// `limit` 限制返回章节数（不是命中数），缺省 50：
/// 百万字的书搜一个常用词可能命中几百章，一次全量返回会把前端塞爆；
/// 面板按需加载即可，这里不追求一次给全。
#[tauri::command(rename_all = "snake_case")]
pub fn search_chapters(
    pool: State<'_, DbPool>,
    novel_id: String,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<ChapterSearchHit>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::search::search_chapters(&conn, &novel_id, &query, limit.unwrap_or(50) as usize)
        .map_err(|e| e.to_string())
}

/// 替换预览：逐章列出命中数与片段，不写库。
#[tauri::command(rename_all = "snake_case")]
pub fn preview_replace(
    pool: State<'_, DbPool>,
    novel_id: String,
    query: String,
    case_sensitive: Option<bool>,
) -> Result<Vec<ReplacePreviewItem>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let case_sensitive = case_sensitive.unwrap_or(false);
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let chapters = crate::db::chapter::list_chapters_for_export(&conn, &novel_id)
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for ch in chapters {
        // 命中数必须与执行时同一套匹配规则（大小写口径含在内），
        // 否则预览与执行会对不上，作者就不会再信预览。
        let hits = (crate::replace::find_all(&ch.content_plain, &query, case_sensitive).len()
            + crate::replace::find_all(&ch.title, &query, case_sensitive).len())
            as i64;
        if hits == 0 {
            continue;
        }
        let unsupported = serde_json::from_str::<serde_json::Value>(&ch.content)
            .ok()
            .and_then(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t != "doc")
            })
            .unwrap_or(true);
        out.push(ReplacePreviewItem {
            chapter_id: ch.id,
            title: ch.title,
            hits,
            snippets: preview_snippets(&ch.content_plain, &query, case_sensitive),
            unsupported,
        });
    }
    Ok(out)
}

/// 执行替换。
///
/// **同一事务**里逐章「先快照、后改写」：任一章失败整体回滚，
/// 不会出现「改了一半」的中间态。快照的 `batch_id` 是整批撤销的钥匙。
#[tauri::command(rename_all = "snake_case")]
pub fn apply_replace(
    pool: State<'_, DbPool>,
    novel_id: String,
    query: String,
    replacement: String,
    chapter_ids: Option<Vec<String>>,
    case_sensitive: Option<bool>,
) -> Result<ReplaceReport, String> {
    let mut conn = pool.get().map_err(|e| e.to_string())?;
    let case_sensitive = case_sensitive.unwrap_or(false);
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err("查找内容不能为空".to_string());
    }

    let selected: Option<std::collections::HashSet<String>> =
        chapter_ids.map(|ids| ids.into_iter().collect());

    let chapters = crate::db::chapter::list_chapters_for_export(&conn, &novel_id)
        .map_err(|e| e.to_string())?;

    let batch_id = uuid::Uuid::new_v4().to_string();
    let options = crate::replace::ReplaceOptions {
        case_sensitive,
        all: true,
    };

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let mut chapters_changed = 0u32;
    let mut total_replacements = 0u32;
    let mut skipped = Vec::new();

    for ch in chapters {
        if let Some(sel) = &selected {
            if !sel.contains(&ch.id) {
                continue;
            }
        }
        let Some(outcome) =
            crate::replace::replace_in_chapter(&ch.content, &query, &replacement, options)
        else {
            skipped.push(ch.title.clone());
            continue;
        };
        if outcome.replacements == 0 {
            continue;
        }

        // 先快照当前内容（替换前），再写入替换后的内容。
        crate::db::chapter_revision::create_revision_with_batch(
            &tx,
            &ch.id,
            &novel_id,
            &ch.title,
            &ch.content,
            &ch.content_plain,
            ch.word_count,
            "manual",
            &format!("批量替换前（查找「{query}」）"),
            Some(&batch_id),
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "UPDATE chapter SET content = ?1, content_plain = ?2, word_count = ?3,
                    updated_at = datetime('now') WHERE id = ?4",
            rusqlite::params![outcome.content, outcome.content_plain, outcome.word_count, ch.id],
        )
        .map_err(|e| e.to_string())?;

        chapters_changed += 1;
        total_replacements += outcome.replacements as u32;
    }

    tx.commit().map_err(|e| e.to_string())?;

    Ok(ReplaceReport {
        batch_id,
        chapters_changed,
        total_replacements,
        skipped_chapters: skipped,
    })
}

/// 整批撤销。
///
/// 按 `batch_id` 找出本轮为每章写的快照，逐章还原为快照内容。
/// 还原本身也走 `revert_chapter_to_revision`：它会把「撤销前的现状」
/// 再存一条 `rollback_before` 快照，撤销错了还能再撤销回来。
#[tauri::command(rename_all = "snake_case")]
pub fn undo_replace_batch(
    pool: State<'_, DbPool>,
    batch_id: String,
) -> Result<UndoReplaceReport, String> {
    let mut conn = pool.get().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let rows: Vec<(String, String)> = {
        let mut stmt = tx
            .prepare(
                "SELECT chapter_id, id FROM chapter_revision
                 WHERE batch_id = ?1 AND revision_type = 'manual'
                 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map(rusqlite::params![batch_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        mapped.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
    };

    if rows.is_empty() {
        return Err("未找到该批次的替换记录（可能已被撤销过）".to_string());
    }

    let mut restored = 0u32;
    let mut missing = Vec::new();
    for (chapter_id, revision_id) in rows {
        match crate::db::chapter_revision::revert_chapter_to_revision(&tx, &chapter_id, &revision_id)
        {
            Ok(_) => restored += 1,
            Err(_) => missing.push(chapter_id),
        }
    }

    tx.commit().map_err(|e| e.to_string())?;

    Ok(UndoReplaceReport {
        batch_id,
        chapters_restored: restored,
        missing_chapters: missing,
    })
}

/// 预览片段（与搜索结果同一套截取逻辑）。
///
/// 用 `replace::find_all` 定位，而不是裸 `text.find` —— 后者区分大小写，
/// 与执行侧的大小写口径不一致时，片段会指向「预览说会改、实际不会改」的位置。
fn preview_snippets(text: &str, term: &str, case_sensitive: bool) -> Vec<String> {
    crate::replace::find_all(text, term, case_sensitive)
        .into_iter()
        .take(3)
        .map(|(pos, _)| crate::db::chapter::snippet_around_at(text, pos, term, 30))
        .collect()
}
