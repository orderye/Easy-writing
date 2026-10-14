use rusqlite::{params, Connection, Result};
use shared_types::{Character, PlotItem, WorldviewEntry};
use uuid::Uuid;

// --- Worldview ---
pub fn list_worldview(conn: &Connection, novel_id: &str) -> Result<Vec<WorldviewEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, group_name, title, content, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
         FROM worldview_entry
         WHERE novel_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
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
            deleted_at: row.get(10)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_worldview(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    group_name: &str,
    title: &str,
    content: &str,
) -> Result<WorldviewEntry> {
    let entry_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

    // If updating existing entry, check previous content for setting_change log
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
            let _ = record_setting_change(
                conn,
                novel_id,
                None,
                "worldview",
                &entry_id,
                &old_title,
                &old_content,
                content,
                "用户手动编辑",
            );
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, group_name, title, content, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
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
            deleted_at: row.get(10)?,
        })
    })
}

pub fn delete_worldview(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE worldview_entry SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Character ---
pub fn list_characters(conn: &Connection, novel_id: &str) -> Result<Vec<Character>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, name, aliases, profile, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
         FROM character
         WHERE novel_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
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
            deleted_at: row.get(10)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_character(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    name: &str,
    aliases: &str,
    profile: &str,
) -> Result<Character> {
    let char_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

    // If updating existing entry, check previous profile for setting_change log
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
            let _ = record_setting_change(
                conn,
                novel_id,
                None,
                "character",
                &char_id,
                &old_name,
                &old_profile,
                profile,
                "用户手动编辑",
            );
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, name, aliases, profile, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
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
            deleted_at: row.get(10)?,
        })
    })
}

pub fn delete_character(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE character SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Plot ---
pub fn list_plots(conn: &Connection, novel_id: &str) -> Result<Vec<PlotItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, title, content, kind, status, chapter_id, origin, sort_order, created_at, updated_at, deleted_at
         FROM plot_item
         WHERE novel_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(PlotItem {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            kind: row.get(4)?,
            status: row.get(5)?,
            chapter_id: row.get(6)?,
            origin: row.get(7)?,
            sort_order: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            deleted_at: row.get(11)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_plot(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    title: &str,
    content: &str,
    kind: &str,
    status: &str,
    chapter_id: Option<String>,
) -> Result<PlotItem> {
    let plot_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

    // If updating existing entry, check previous content for setting_change log
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
            updated_at = datetime('now')",
        params![plot_id, novel_id, title, content, kind, status, chapter_id],
    )?;

    if let Some((old_title, old_content)) = old_info {
        if old_content != content {
            let _ = record_setting_change(
                conn,
                novel_id,
                chapter_id.as_deref(),
                "plot",
                &plot_id,
                &old_title,
                &old_content,
                content,
                "用户手动编辑",
            );
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, title, content, kind, status, chapter_id, origin, sort_order, created_at, updated_at, deleted_at
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
            origin: row.get(7)?,
            sort_order: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            deleted_at: row.get(11)?,
        })
    })
}

pub fn delete_plot(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE plot_item SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Setting Changes Audit & Rollback ---
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
        params![id, novel_id, chapter_id, target_type, target_id, target_title, before_content, after_content, reason],
    )?;
    Ok(id)
}

pub fn list_setting_changes(
    conn: &Connection,
    novel_id: &str,
) -> Result<Vec<shared_types::SettingChange>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, chapter_id, target_type, target_id, target_title, before_content, after_content, reason, is_reverted, created_at
         FROM setting_change
         WHERE novel_id = ?1
         ORDER BY created_at DESC, rowid DESC
         LIMIT 100",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        let is_reverted_int: i32 = row.get(9)?;
        Ok(shared_types::SettingChange {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            chapter_id: row.get(2)?,
            target_type: row.get(3)?,
            target_id: row.get(4)?,
            target_title: row.get(5)?,
            before_content: row.get(6)?,
            after_content: row.get(7)?,
            reason: row.get(8)?,
            is_reverted: is_reverted_int != 0,
            created_at: row.get(10)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn revert_setting_change(
    conn: &Connection,
    change_id: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT id, target_type, target_id, target_title, before_content, after_content, is_reverted
         FROM setting_change WHERE id = ?1",
    )?;

    let (id, target_type, target_id, _title, before_content, _after_content, is_reverted): (String, String, String, String, String, String, i32) =
        stmt.query_row(params![change_id], |row| {
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
            conn.execute(
                "UPDATE plot_item SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![before_content, target_id],
            )?;
        }
        _ => {}
    }

    conn.execute(
        "UPDATE setting_change SET is_reverted = 1 WHERE id = ?1",
        params![id],
    )?;

    Ok(())
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
                let _ = record_setting_change(
                    conn,
                    novel_id,
                    chapter_id,
                    "worldview",
                    &id,
                    setting_title,
                    &old_content,
                    new_content,
                    reason,
                );
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
                let _ = record_setting_change(
                    conn,
                    novel_id,
                    chapter_id,
                    "character",
                    &id,
                    setting_title,
                    &old_profile,
                    new_content,
                    reason,
                );
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
                let _ = record_setting_change(
                    conn,
                    novel_id,
                    chapter_id,
                    "plot",
                    &id,
                    setting_title,
                    &old_content,
                    new_content,
                    reason,
                );
            }
        }
        _ => {}
    }
    Ok(())
}


