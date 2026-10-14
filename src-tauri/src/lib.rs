pub mod agent;
pub mod ai;
pub mod asset;
pub mod commands;
pub mod db;
pub mod keyring;
pub mod sync;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            let pool = db::init_db(app_data_dir)?;
            app.manage(pool);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Novel
            commands::create_novel,
            commands::list_novels,
            commands::get_novel,
            commands::update_novel,
            commands::delete_novel,
            // Volume
            commands::create_volume,
            commands::list_volumes,
            commands::update_volume,
            commands::reorder_volumes,
            commands::delete_volume,
            // Chapter
            commands::create_chapter,
            commands::list_chapter_headers,
            commands::list_all_chapter_headers_for_novel,
            commands::get_chapter,
            commands::save_chapter_content,
            commands::update_chapter_meta,
            commands::move_chapter,
            commands::reorder_chapters,
            commands::delete_chapter,
            // Chapter Revisions
            commands::list_chapter_revisions,
            commands::get_chapter_revision,
            commands::create_manual_chapter_revision,
            commands::revert_chapter_to_revision,
            // Settings
            commands::list_worldview,
            commands::save_worldview,
            commands::delete_worldview,
            commands::list_characters,
            commands::save_character,
            commands::delete_character,
            commands::list_plots,
            commands::save_plot,
            commands::delete_plot,
            // Setting Changes
            commands::list_setting_changes,
            commands::revert_setting_change,
            commands::apply_conflict_setting_update,
            // Asset
            commands::save_image_asset,
            // AI commands
            commands::ai_cmd::ai_polish,
            commands::ai_cmd::ai_chat_stream,
            commands::ai_cmd::ai_check_conflict,
            commands::ai_cmd::trigger_chapter_extract,
            commands::ai_cmd::list_extraction_candidates,
            commands::ai_cmd::resolve_candidate,
            // Config & Profiles
            commands::config_cmd::create_llm_profile,
            commands::config_cmd::list_llm_profiles,
            commands::config_cmd::delete_llm_profile,
            commands::config_cmd::test_llm_connection,
            commands::config_cmd::list_agents,
            commands::config_cmd::update_agent,
            // App settings
            commands::get_app_setting,
            commands::set_app_setting,
            // WebDAV Sync
            commands::sync_cmd::webdav_save_config,
            commands::sync_cmd::webdav_get_config,
            commands::sync_cmd::webdav_test_connection,
            commands::sync_cmd::webdav_backup_now,
            commands::sync_cmd::webdav_list_backups,
            commands::sync_cmd::webdav_restore_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running easy-writing core");
}
