pub mod ai_cmd;
pub mod config_cmd;
pub mod sync_cmd;

use crate::db::DbPool;
use shared_types::{
    Chapter, ChapterHeader, ChapterRevision, ChapterRevisionHeader, Character, Novel, PlotItem,
    SettingChange, TrashItem, Volume, WorldviewEntry,
};
use tauri::State;

// Novel commands
#[tauri::command(rename_all = "snake_case")]
pub fn create_novel(
    pool: State<DbPool>,
    title: String,
    synopsis: String,
) -> Result<Novel, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::create_novel(&conn, &title, &synopsis).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_novels(pool: State<DbPool>) -> Result<Vec<Novel>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::list_novels(&conn).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_novel(pool: State<DbPool>, id: String) -> Result<Option<Novel>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::get_novel(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_novel(
    pool: State<DbPool>,
    id: String,
    title: String,
    synopsis: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::update_novel(&conn, &id, &title, &synopsis).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_novel(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::delete_novel(&conn, &id).map_err(|e| e.to_string())
}

// Volume commands
#[tauri::command(rename_all = "snake_case")]
pub fn create_volume(
    pool: State<DbPool>,
    novel_id: String,
    title: String,
    summary: String,
) -> Result<Volume, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::create_volume(&conn, &novel_id, &title, &summary).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_volumes(pool: State<DbPool>, novel_id: String) -> Result<Vec<Volume>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::list_volumes(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_volume(
    pool: State<DbPool>,
    id: String,
    title: String,
    summary: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::update_volume(&conn, &id, &title, &summary).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reorder_volumes(pool: State<DbPool>, volume_ids: Vec<String>) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::reorder_volumes(&conn, &volume_ids).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_volume(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::delete_volume(&conn, &id).map_err(|e| e.to_string())
}

// Chapter commands
#[tauri::command(rename_all = "snake_case")]
pub fn create_chapter(
    pool: State<DbPool>,
    novel_id: String,
    volume_id: String,
    title: String,
    summary: String,
) -> Result<Chapter, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::create_chapter(&conn, &novel_id, &volume_id, &title, &summary)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_chapter_headers(
    pool: State<DbPool>,
    volume_id: String,
) -> Result<Vec<ChapterHeader>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::list_chapter_headers(&conn, &volume_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_all_chapter_headers_for_novel(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<ChapterHeader>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::list_all_chapter_headers_for_novel(&conn, &novel_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_chapter(pool: State<DbPool>, id: String) -> Result<Option<Chapter>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::get_chapter(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_chapter_content(
    app_handle: tauri::AppHandle,
    pool: State<DbPool>,
    id: String,
    content: String,
    content_plain: String,
    word_count: i32,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::save_chapter_content(&conn, &id, &content, &content_plain, word_count)
        .map_err(|e| e.to_string())?;

    // Auto snapshot trigger if conditions are met
    if let Ok(Some(chap)) = crate::db::chapter::get_chapter(&conn, &id) {
        let _ = crate::db::chapter_revision::maybe_auto_snapshot(
            &conn,
            &id,
            &chap.novel_id,
            &chap.title,
            &content,
            &content_plain,
            word_count,
        );

        // Silent background extraction trigger if text has meaningful length
        let pool_clone = pool.inner().clone();
        let novel_id = chap.novel_id.clone();
        let chapter_id = id.clone();
        let handle_clone = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let res = (|| -> Result<_, String> {
                let conn = pool_clone.get().map_err(|e| e.to_string())?;
                let novel = match crate::db::novel::get_novel(&conn, &novel_id) {
                    Ok(Some(n)) => n,
                    _ => return Err("Novel not found".to_string()),
                };
                let chapter = match crate::db::chapter::get_chapter(&conn, &chapter_id) {
                    Ok(Some(c)) => c,
                    _ => return Err("Chapter not found".to_string()),
                };
                let (_agent, profile, api_key) =
                    crate::agent::dispatcher::resolve_agent_and_profile(&conn, "writing_assistant")?;
                Ok((novel, chapter, profile, api_key))
            })();

            if let Ok((novel, chapter, profile, api_key)) = res {
                if chapter.content_plain.chars().count() >= 100 {
                    let gateway_arc = std::sync::Arc::new(crate::ai::gateway::AiGateway::new());
                    if let Ok(candidates) = crate::agent::extraction::run_parallel_extraction(
                        gateway_arc,
                        pool_clone,
                        novel,
                        chapter,
                        profile,
                        api_key,
                    ).await {
                        use tauri::Emitter;
                        let _ = handle_clone.emit("extraction_completed", candidates.len());
                    }
                }
            }
        });
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_chapter_revisions(
    pool: State<DbPool>,
    chapter_id: String,
) -> Result<Vec<ChapterRevisionHeader>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter_revision::list_revisions_for_chapter(&conn, &chapter_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_chapter_revision(
    pool: State<DbPool>,
    id: String,
) -> Result<Option<ChapterRevision>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter_revision::get_revision(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_manual_chapter_revision(
    pool: State<DbPool>,
    chapter_id: String,
    note: String,
) -> Result<ChapterRevision, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let chap = crate::db::chapter::get_chapter(&conn, &chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Chapter not found".to_string())?;

    crate::db::chapter_revision::create_revision(
        &conn,
        &chap.id,
        &chap.novel_id,
        &chap.title,
        &chap.content,
        &chap.content_plain,
        chap.word_count,
        "manual",
        &note,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn revert_chapter_to_revision(
    pool: State<DbPool>,
    chapter_id: String,
    revision_id: String,
) -> Result<Chapter, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter_revision::revert_chapter_to_revision(&conn, &chapter_id, &revision_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_chapter_meta(
    pool: State<DbPool>,
    id: String,
    title: String,
    summary: String,
    status: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::update_chapter_meta(&conn, &id, &title, &summary, &status)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn move_chapter(
    pool: State<DbPool>,
    chapter_id: String,
    new_volume_id: String,
    new_sort_order: i32,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::move_chapter_to_volume(&conn, &chapter_id, &new_volume_id, new_sort_order)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reorder_chapters(pool: State<DbPool>, chapter_ids: Vec<String>) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::reorder_chapters(&conn, &chapter_ids).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_chapter(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::delete_chapter(&conn, &id).map_err(|e| e.to_string())
}

// Settings commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_worldview(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<WorldviewEntry>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_worldview(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_worldview(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    group_name: String,
    title: String,
    content: String,
) -> Result<WorldviewEntry, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::save_worldview(&conn, id, &novel_id, &group_name, &title, &content)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_worldview(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::delete_worldview(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_characters(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<Character>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_characters(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_character(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    name: String,
    aliases: String,
    profile: String,
) -> Result<Character, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::save_character(&conn, id, &novel_id, &name, &aliases, &profile)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_character(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::delete_character(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_plots(pool: State<DbPool>, novel_id: String) -> Result<Vec<PlotItem>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_plots(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_plot(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    title: String,
    content: String,
    kind: String,
    status: String,
    chapter_id: Option<String>,
) -> Result<PlotItem, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::save_plot(
        &conn, id, &novel_id, &title, &content, &kind, &status, chapter_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_plot(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::delete_plot(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_setting_changes(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<SettingChange>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_setting_changes(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn revert_setting_change(
    pool: State<DbPool>,
    change_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::revert_setting_change(&conn, &change_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn apply_conflict_setting_update(
    pool: State<DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
    setting_module: String,
    setting_title: String,
    new_content: String,
    reason: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::apply_conflict_setting_update(
        &conn,
        &novel_id,
        chapter_id.as_deref(),
        &setting_module,
        &setting_title,
        &new_content,
        &reason,
    )
    .map_err(|e| e.to_string())
}

// App Settings commands
#[tauri::command(rename_all = "snake_case")]
pub fn get_app_setting(pool: State<DbPool>, key: String) -> Result<Option<String>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::app_setting::get_setting(&conn, &key).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_app_setting(pool: State<DbPool>, key: String, value: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::app_setting::set_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_image_asset(
    app: tauri::AppHandle,
    pool: State<DbPool>,
    novel_id: String,
    name: String,
    mime: String,
    base64_data: String,
) -> Result<String, String> {
    use tauri::Manager;
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::save_image(&conn, &app_data_dir, &novel_id, &name, &mime, &base64_data)
}

// Trash / Recycle Bin commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_trash_items(pool: State<DbPool>, novel_id: String) -> Result<Vec<TrashItem>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::list_trash_items(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn restore_trash_item(
    pool: State<DbPool>,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::restore_trash_item(&conn, &item_type, &item_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn purge_trash_item(
    pool: State<DbPool>,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::purge_trash_item(&conn, &item_type, &item_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn empty_trash(pool: State<DbPool>, novel_id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::empty_trash(&conn, &novel_id).map_err(|e| e.to_string())
}

// Import commands
#[tauri::command(rename_all = "snake_case")]
pub fn parse_import_document(
    file_name: String,
    raw_text: String,
) -> Result<shared_types::ImportPreview, String> {
    Ok(crate::import::parse_import_document(&file_name, &raw_text))
}

#[tauri::command(rename_all = "snake_case")]
pub fn commit_import_novel(
    pool: State<DbPool>,
    target_novel_id: Option<String>,
    preview: shared_types::ImportPreview,
) -> Result<Novel, String> {
    let mut conn = pool.get().map_err(|e| e.to_string())?;
    crate::import::commit_import_novel(&mut conn, target_novel_id, preview)
}

// Character Relation commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_character_relations(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<shared_types::CharacterRelation>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::character_relation::list_character_relations(&conn, &novel_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_character_relation(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    from_char_id: String,
    to_char_id: String,
    relation_name: String,
    description: String,
) -> Result<shared_types::CharacterRelation, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::character_relation::save_character_relation(
        &conn,
        id,
        &novel_id,
        &from_char_id,
        &to_char_id,
        &relation_name,
        &description,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_character_relation(
    pool: State<DbPool>,
    id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::character_relation::delete_character_relation(&conn, &id)
        .map_err(|e| e.to_string())
}

// Prompt Template commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_prompt_templates(
    pool: State<DbPool>,
) -> Result<Vec<shared_types::PromptTemplate>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::prompt_template::list_prompt_templates(&conn)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_prompt_template(
    pool: State<DbPool>,
    key: String,
    template: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::prompt_template::save_prompt_template(&conn, &key, &template)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reset_prompt_template(
    pool: State<DbPool>,
    key: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::prompt_template::reset_prompt_template(&conn, &key)
        .map_err(|e| e.to_string())
}




