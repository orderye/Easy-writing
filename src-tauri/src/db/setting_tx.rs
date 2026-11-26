//! 统一设定变更事务通道（MIT 公理设计下三角解耦层）。
//!
//! 所有对核心设定表（`worldview_entry` / `character` / `plot_item` / `chapter.summary` /
//! `novel.synopsis`）的写入、冲突裁决落库、提炼采纳、同人合并、伏笔回收与变更撤回，
//! 均通过本模块的 savepoint 事务通道执行，保证业务表写入与 `setting_change` 留档严格原子化。

use rusqlite::{params, Connection, Result};
use shared_types::{Character, PlotItem, WorldviewEntry};
use uuid::Uuid;

/// 在 SQLite `SAVEPOINT` 中执行闭包；无论外层是否已开启 `Transaction`，均可安全嵌套并保证原子性。
pub fn with_savepoint<T, F>(conn: &Connection, name: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    conn.execute_batch(&format!("SAVEPOINT {name};"))?;
    match f() {
        Ok(val) => {
            conn.execute_batch(&format!("RELEASE SAVEPOINT {name};"))?;
            Ok(val)
        }
        Err(err) => {
            let _ = conn.execute_batch(&format!(
                "ROLLBACK TO SAVEPOINT {name}; RELEASE SAVEPOINT {name};"
            ));
            Err(err)
        }
    }
}

/// 记录一条设定变更留档。
#[allow(clippy::too_many_arguments)]
pub fn record_setting_change(
    conn: &Connection,
    novel_id: &str,
    chapter_id: Option<&str>,
    target_type: &str,
    target_id: &str,
    target_title: &str,
    before_content: &str,
    after_content: &str,
    reason: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO setting_change (id, novel_id, chapter_id, target_type, target_id, target_title, before_content, after_content, reason, is_reverted, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, datetime('now'))",
        params![
            id,
            novel_id,
            chapter_id,
            target_type,
            target_id,
            target_title,
            before_content,
            after_content,
            reason
        ],
    )?;
    Ok(id)
}

pub fn save_worldview_with_reason(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    group_name: &str,
    title: &str,
    content: &str,
    reason: &str,
) -> Result<WorldviewEntry> {
    with_savepoint(conn, "sp_save_worldview", || {
        let entry_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let old_info: Option<(String, String)> = conn
            .query_row(
                "SELECT title, content FROM worldview_entry WHERE id = ?1",
                params![entry_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        conn.execute(
            "INSERT INTO worldview_entry (id, novel_id, group_name, title, content, origin, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, 'manual', 0)
             ON CONFLICT(id) DO UPDATE SET
                group_name = excluded.group_name,
                title = excluded.title,
                content = excluded.content,
                updated_at = datetime('now')",
            params![entry_id, novel_id, group_name, title, content],
        )?;

        if let Some((old_title, old_content)) = old_info {
            if old_content != content {
                record_setting_change(
                    conn,
                    novel_id,
                    None,
                    "worldview",
                    &entry_id,
                    &old_title,
                    &old_content,
                    content,
                    reason,
                )?;
            }
        }

        let mut stmt = conn.prepare(
            "SELECT id, novel_id, group_name, title, content, origin, source_chapter_id, sort_order, created_at, updated_at, pinned, deleted_at
             FROM worldview_entry WHERE id = ?1",
        )?;
        stmt.query_row(params![entry_id], |row| {
            Ok(WorldviewEntry {
                id: row.get(0)?,
                novel_id: row.get(1)?,
                group_name: row.get(2)?,
                title: row.get(3)?,
                content: row.get(4)?,
                origin: row.get(5)?,
                source_chapter_id: row.get(6)?,
                sort_order: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                pinned: row.get::<_, i32>(10)? != 0,
                deleted_at: row.get(11)?,
            })
        })
    })
}

pub fn save_character_with_reason(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    name: &str,
    aliases: &str,
    profile: &str,
    reason: &str,
) -> Result<Character> {
    with_savepoint(conn, "sp_save_character", || {
        let char_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let old_info: Option<(String, String)> = conn
            .query_row(
                "SELECT name, profile FROM character WHERE id = ?1",
                params![char_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        conn.execute(
            "INSERT INTO character (id, novel_id, name, aliases, profile, origin, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, 'manual', 0)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                aliases = excluded.aliases,
                profile = excluded.profile,
                updated_at = datetime('now')",
            params![char_id, novel_id, name, aliases, profile],
        )?;

        if let Some((old_name, old_profile)) = old_info {
            if old_profile != profile {
                record_setting_change(
                    conn,
                    novel_id,
                    None,
                    "character",
                    &char_id,
                    &old_name,
                    &old_profile,
                    profile,
                    reason,
                )?;
            }
        }

        let mut stmt = conn.prepare(
            "SELECT id, novel_id, name, aliases, profile, origin, source_chapter_id, sort_order, created_at, updated_at, pinned, deleted_at
             FROM character WHERE id = ?1",
        )?;
        stmt.query_row(params![char_id], |row| {
            Ok(Character {
                id: row.get(0)?,
                novel_id: row.get(1)?,
                name: row.get(2)?,
                aliases: row.get(3)?,
                profile: row.get(4)?,
                origin: row.get(5)?,
                source_chapter_id: row.get(6)?,
                sort_order: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                pinned: row.get::<_, i32>(10)? != 0,
                deleted_at: row.get(11)?,
            })
        })
    })
}

#[allow(clippy::too_many_arguments)]
pub fn save_plot_with_reason(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    title: &str,
    content: &str,
    kind: &str,
    status: &str,
    chapter_id: Option<String>,
    reason: &str,
) -> Result<PlotItem> {
    with_savepoint(conn, "sp_save_plot", || {
        let plot_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

        let old_info: Option<(String, String)> = conn
            .query_row(
                "SELECT title, content FROM plot_item WHERE id = ?1",
                params![plot_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        conn.execute(
            "INSERT INTO plot_item (id, novel_id, title, content, kind, status, chapter_id, origin, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual', 0)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                content = excluded.content,
                kind = excluded.kind,
                status = excluded.status,
                chapter_id = excluded.chapter_id,
                -- 刻意不更新 resolved_chapter_id：编辑标题/内容时不能把回收章节抹掉。
                -- 只有 resolve_plot 命令能改它。
                updated_at = datetime('now')",
            params![plot_id, novel_id, title, content, kind, status, chapter_id],
        )?;

        if let Some((old_title, old_content)) = old_info {
            if old_content != content {
                record_setting_change(
                    conn,
                    novel_id,
                    chapter_id.as_deref(),
                    "plot",
                    &plot_id,
                    &old_title,
                    &old_content,
                    content,
                    reason,
                )?;
            }
        }

        let mut stmt = conn.prepare(
            "SELECT id, novel_id, title, content, kind, status, chapter_id, resolved_chapter_id, origin, sort_order, created_at, updated_at, pinned, deleted_at
             FROM plot_item WHERE id = ?1",
        )?;
        stmt.query_row(params![plot_id], |row| {
            Ok(PlotItem {
                id: row.get(0)?,
                novel_id: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                chapter_id: row.get(6)?,
                resolved_chapter_id: row.get(7)?,
                origin: row.get(8)?,
                sort_order: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                pinned: row.get::<_, i32>(12)? != 0,
                deleted_at: row.get(13)?,
            })
        })
    })
}

pub fn update_chapter_summary_with_reason(
    conn: &Connection,
    novel_id: &str,
    chapter_id: &str,
    target_title: &str,
    new_summary: &str,
    reason: &str,
) -> Result<()> {
    with_savepoint(conn, "sp_update_chapter_summary", || {
        let old_summary: Option<String> = conn
            .query_row(
                "SELECT summary FROM chapter WHERE id = ?1",
                params![chapter_id],
                |r| r.get(0),
            )
            .ok();

        if let Some(old_s) = old_summary {
            conn.execute(
                "UPDATE chapter SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![new_summary, chapter_id],
            )?;
            record_setting_change(
                conn,
                novel_id,
                Some(chapter_id),
                "chapter",
                chapter_id,
                target_title,
                &old_s,
                new_summary,
                reason,
            )?;
        }
        Ok(())
    })
}

pub fn update_volume_summary_with_reason(
    conn: &Connection,
    novel_id: &str,
    volume_id: &str,
    target_title: &str,
    new_summary: &str,
    reason: &str,
) -> Result<()> {
    with_savepoint(conn, "sp_update_volume_summary", || {
        let old_summary: Option<String> = conn
            .query_row(
                "SELECT summary FROM volume WHERE id = ?1",
                params![volume_id],
                |r| r.get(0),
            )
            .ok();

        if let Some(old_s) = old_summary {
            conn.execute(
                "UPDATE volume SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![new_summary, volume_id],
            )?;
            record_setting_change(
                conn,
                novel_id,
                None,
                "volume",
                volume_id,
                target_title,
                &old_s,
                new_summary,
                reason,
            )?;
        }
        Ok(())
    })
}

pub fn update_novel_synopsis_with_reason(
    conn: &Connection,
    novel_id: &str,
    target_title: &str,
    new_synopsis: &str,
    reason: &str,
) -> Result<()> {
    with_savepoint(conn, "sp_update_novel_synopsis", || {
        let old_synopsis: Option<String> = conn
            .query_row(
                "SELECT synopsis FROM novel WHERE id = ?1",
                params![novel_id],
                |r| r.get(0),
            )
            .ok();

        if let Some(old_s) = old_synopsis {
            conn.execute(
                "UPDATE novel SET synopsis = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![new_synopsis, novel_id],
            )?;
            record_setting_change(
                conn,
                novel_id,
                None,
                "outline",
                novel_id,
                target_title,
                &old_s,
                new_synopsis,
                reason,
            )?;
        }
        Ok(())
    })
}

pub fn resolve_plot_with_reason(
    conn: &Connection,
    id: &str,
    resolved_chapter_id: Option<String>,
    reason: &str,
) -> Result<PlotItem> {
    with_savepoint(conn, "sp_resolve_plot", || {
        let plot: PlotItem = {
            let mut stmt = conn.prepare(
                "SELECT id, novel_id, title, content, kind, status, chapter_id, resolved_chapter_id, origin, sort_order, created_at, updated_at, pinned, deleted_at
                 FROM plot_item WHERE id = ?1 AND deleted_at IS NULL",
            )?;
            stmt.query_row(params![id], |row| {
                Ok(PlotItem {
                    id: row.get(0)?,
                    novel_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    kind: row.get(4)?,
                    status: row.get(5)?,
                    chapter_id: row.get(6)?,
                    resolved_chapter_id: row.get(7)?,
                    origin: row.get(8)?,
                    sort_order: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    pinned: row.get::<_, i32>(12)? != 0,
                    deleted_at: row.get(13)?,
                })
            })?
        };

        if let Some(cid) = resolved_chapter_id.as_deref() {
            let belongs: bool = conn
                .query_row(
                    "SELECT 1 FROM chapter WHERE id = ?1 AND novel_id = ?2 AND deleted_at IS NULL",
                    params![cid, plot.novel_id],
                    |_| Ok(()),
                )
                .is_ok();
            if !belongs {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "回收章节不属于该作品，拒绝写入：{}",
                    cid
                )));
            }
        }

        let before_status = plot.status.clone();
        conn.execute(
            "UPDATE plot_item SET status = 'resolved', resolved_chapter_id = ?1, updated_at = datetime('now')
             WHERE id = ?2",
            params![resolved_chapter_id, id],
        )?;

        record_setting_change(
            conn,
            &plot.novel_id,
            plot.chapter_id.as_deref(),
            "plot",
            &plot.id,
            &plot.title,
            &format!("status={}", before_status),
            &format!("status=resolved, resolved_chapter_id={:?}", resolved_chapter_id),
            reason,
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, novel_id, title, content, kind, status, chapter_id, resolved_chapter_id, origin, sort_order, created_at, updated_at, pinned, deleted_at
             FROM plot_item WHERE id = ?1",
        )?;
        stmt.query_row(params![id], |row| {
            Ok(PlotItem {
                id: row.get(0)?,
                novel_id: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                chapter_id: row.get(6)?,
                resolved_chapter_id: row.get(7)?,
                origin: row.get(8)?,
                sort_order: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                pinned: row.get::<_, i32>(12)? != 0,
                deleted_at: row.get(13)?,
            })
        })
    })
}

pub fn apply_conflict_setting_update(
    conn: &Connection,
    novel_id: &str,
    chapter_id: Option<&str>,
    setting_module: &str,
    setting_title: &str,
    new_content: &str,
    reason: &str,
) -> Result<()> {
    with_savepoint(conn, "sp_apply_conflict", || {
        match setting_module {
            "worldview" => {
                let row: Option<(String, String)> = conn
                    .query_row(
                        "SELECT id, content FROM worldview_entry WHERE novel_id = ?1 AND title = ?2 AND deleted_at IS NULL LIMIT 1",
                        params![novel_id, setting_title],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .ok();

                if let Some((id, old_content)) = row {
                    conn.execute(
                        "UPDATE worldview_entry SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                        params![new_content, id],
                    )?;
                    record_setting_change(
                        conn,
                        novel_id,
                        chapter_id,
                        "worldview",
                        &id,
                        setting_title,
                        &old_content,
                        new_content,
                        reason,
                    )?;
                }
            }
            "character" => {
                let row: Option<(String, String)> = conn
                    .query_row(
                        "SELECT id, profile FROM character WHERE novel_id = ?1 AND name = ?2 AND deleted_at IS NULL LIMIT 1",
                        params![novel_id, setting_title],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .ok();

                if let Some((id, old_profile)) = row {
                    conn.execute(
                        "UPDATE character SET profile = ?1, updated_at = datetime('now') WHERE id = ?2",
                        params![new_content, id],
                    )?;
                    record_setting_change(
                        conn,
                        novel_id,
                        chapter_id,
                        "character",
                        &id,
                        setting_title,
                        &old_profile,
                        new_content,
                        reason,
                    )?;
                }
            }
            "plot" => {
                let row: Option<(String, String)> = conn
                    .query_row(
                        "SELECT id, content FROM plot_item WHERE novel_id = ?1 AND title = ?2 AND deleted_at IS NULL LIMIT 1",
                        params![novel_id, setting_title],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .ok();

                if let Some((id, old_content)) = row {
                    conn.execute(
                        "UPDATE plot_item SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                        params![new_content, id],
                    )?;
                    record_setting_change(
                        conn,
                        novel_id,
                        chapter_id,
                        "plot",
                        &id,
                        setting_title,
                        &old_content,
                        new_content,
                        reason,
                    )?;
                }
            }
            "chapter" => {
                if let Some(cid) = chapter_id {
                    update_chapter_summary_with_reason(
                        conn,
                        novel_id,
                        cid,
                        setting_title,
                        new_content,
                        reason,
                    )?;
                }
            }
            "outline" => {
                if let Some(cid) = chapter_id {
                    update_chapter_summary_with_reason(
                        conn,
                        novel_id,
                        cid,
                        setting_title,
                        new_content,
                        reason,
                    )?;
                } else {
                    update_novel_synopsis_with_reason(
                        conn,
                        novel_id,
                        setting_title,
                        new_content,
                        reason,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    })
}

pub fn revert_setting_change(conn: &Connection, change_id: &str) -> Result<()> {
    with_savepoint(conn, "sp_revert_setting_change", || {
        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_id, target_title, before_content, after_content, is_reverted
             FROM setting_change WHERE id = ?1",
        )?;

        let (id, target_type, target_id, _title, before_content, _after_content, is_reverted): (
            String,
            String,
            String,
            String,
            String,
            String,
            i32,
        ) = stmt.query_row(params![change_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?;

        if is_reverted != 0 {
            return Ok(());
        }

        match target_type.as_str() {
            "worldview" => {
                conn.execute(
                    "UPDATE worldview_entry SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                    params![before_content, target_id],
                )?;
            }
            "character" => {
                conn.execute(
                    "UPDATE character SET profile = ?1, updated_at = datetime('now') WHERE id = ?2",
                    params![before_content, target_id],
                )?;
            }
            "plot" => {
                if let Some(prev_status) = before_content.strip_prefix("status=") {
                    let status = prev_status.trim();
                    if matches!(status, "planted" | "resolved") {
                        conn.execute(
                            "UPDATE plot_item SET status = ?1, resolved_chapter_id = NULL, updated_at = datetime('now') WHERE id = ?2",
                            params![status, target_id],
                        )?;
                    }
                } else {
                    conn.execute(
                        "UPDATE plot_item SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                        params![before_content, target_id],
                    )?;
                }
            }
            "chapter" => {
                conn.execute(
                    "UPDATE chapter SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                    params![before_content, target_id],
                )?;
            }
            "volume" => {
                conn.execute(
                    "UPDATE volume SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                    params![before_content, target_id],
                )?;
            }
            "outline" | "novel" => {
                let is_chapter: bool = conn
                    .query_row(
                        "SELECT 1 FROM chapter WHERE id = ?1",
                        params![target_id],
                        |_| Ok(true),
                    )
                    .unwrap_or(false);
                if is_chapter {
                    conn.execute(
                        "UPDATE chapter SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                        params![before_content, target_id],
                    )?;
                } else {
                    let is_volume: bool = conn
                        .query_row(
                            "SELECT 1 FROM volume WHERE id = ?1",
                            params![target_id],
                            |_| Ok(true),
                        )
                        .unwrap_or(false);
                    if is_volume {
                        conn.execute(
                            "UPDATE volume SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                            params![before_content, target_id],
                        )?;
                    } else {
                        let updated = conn
                            .execute(
                                "UPDATE novel SET synopsis = ?1, updated_at = datetime('now') WHERE id = ?2",
                                params![before_content, target_id],
                            )
                            .unwrap_or(0);
                        if updated == 0 {
                            let novel_id: Option<String> = conn
                                .query_row(
                                    "SELECT novel_id FROM setting_change WHERE id = ?1",
                                    params![change_id],
                                    |r| r.get(0),
                                )
                                .ok();
                            if let Some(nid) = novel_id {
                                let _ = conn.execute(
                                    "UPDATE novel SET synopsis = ?1, updated_at = datetime('now') WHERE id = ?2",
                                    params![before_content, nid],
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        conn.execute(
            "UPDATE setting_change SET is_reverted = 1 WHERE id = ?1",
            params![id],
        )?;

        Ok(())
    })
}
