use rusqlite::{params, Connection, Result};
use shared_types::{Character, PlotItem, WorldviewEntry};

// --- Worldview ---
pub fn list_worldview(conn: &Connection, novel_id: &str) -> Result<Vec<WorldviewEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, group_name, title, content, origin, source_chapter_id, sort_order, created_at, updated_at, pinned, deleted_at
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
            pinned: row.get::<_, i32>(10)? != 0,
            deleted_at: row.get(11)?,
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
    crate::db::setting_tx::save_worldview_with_reason(
        conn,
        id,
        novel_id,
        group_name,
        title,
        content,
        "用户手动编辑",
    )
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
        "SELECT id, novel_id, name, aliases, profile, origin, source_chapter_id, sort_order, created_at, updated_at, pinned, deleted_at
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
            pinned: row.get::<_, i32>(10)? != 0,
            deleted_at: row.get(11)?,
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
    crate::db::setting_tx::save_character_with_reason(
        conn,
        id,
        novel_id,
        name,
        aliases,
        profile,
        "用户手动编辑",
    )
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
        "SELECT id, novel_id, title, content, kind, status, chapter_id, resolved_chapter_id, origin, sort_order, created_at, updated_at, pinned, deleted_at
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
            resolved_chapter_id: row.get(7)?,
            origin: row.get(8)?,
            sort_order: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
            pinned: row.get::<_, i32>(12)? != 0,
            deleted_at: row.get(13)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

#[allow(clippy::too_many_arguments)]
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
    crate::db::setting_tx::save_plot_with_reason(
        conn,
        id,
        novel_id,
        title,
        content,
        kind,
        status,
        chapter_id,
        "用户手动编辑",
    )
}

pub fn delete_plot(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE plot_item SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Setting Changes Audit & Rollback ---
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
    crate::db::setting_tx::record_setting_change(
        conn,
        novel_id,
        chapter_id,
        target_type,
        target_id,
        target_title,
        before_content,
        after_content,
        reason,
    )
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

pub fn revert_setting_change(conn: &Connection, change_id: &str) -> Result<()> {
    crate::db::setting_tx::revert_setting_change(conn, change_id)
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
    crate::db::setting_tx::apply_conflict_setting_update(
        conn,
        novel_id,
        chapter_id,
        setting_module,
        setting_title,
        new_content,
        reason,
    )
}


