pub mod agent;
pub mod ai;
pub mod asset;
pub mod commands;
pub mod db;
pub mod export;
pub mod import;
pub mod keyring;
pub mod keys;
pub mod replace;
pub mod sync;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            let pool = db::init_db(app_data_dir)?;
            app.manage(pool);
            // 导出取消标志：跨命令共享（导出命令置位、取消命令读取）。
            app.manage(export::ExportState::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(pool) = window.try_state::<db::DbPool>() {
                    let pool = pool.inner().clone();
                    if commands::sync_cmd::is_webdav_configured(&pool) {
                        api.prevent_close();
                        let app_handle = window.app_handle().clone();
                        let window_clone = window.clone();
                        let app_data_dir = app_handle
                            .path()
                            .app_data_dir()
                            .unwrap_or_else(|_| std::env::temp_dir());

                        tauri::async_runtime::spawn(async move {
                            let sync_task = commands::sync_cmd::run_silent_sync_if_configured(pool, app_data_dir);
                            let _ = tokio::time::timeout(std::time::Duration::from_secs(10), sync_task).await;
                            let _ = window_clone.destroy();
                        });
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Novel
            commands::create_novel,
            commands::list_novels,
            commands::list_novels_with_stats,
            commands::update_novel,
            commands::delete_novel,
            commands::list_deleted_novels,
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
            commands::get_app_data_dir,
            commands::list_image_assets,
            commands::rename_image_asset,
            commands::delete_image_asset,
            commands::list_deleted_image_assets,
            commands::restore_image_asset,
            commands::purge_image_asset,
            commands::get_image_asset_usage,
            commands::scan_image_storage,
            commands::prune_orphan_images,
            // AI commands
            commands::ai_cmd::ai_polish,
            commands::ai_cmd::ai_chat_stream,
            commands::ai_cmd::ai_check_conflict,
            commands::ai_cmd::trigger_chapter_extract,
            commands::ai_cmd::list_extraction_candidates,
            commands::ai_cmd::resolve_candidate,
            commands::ai_cmd::merge_character_candidate,
            // Context Transparency (T06)
            commands::ai_cmd::get_context_manifest,
            commands::ai_cmd::set_entry_pinned,
            // Chat History (PRD §4.1 P1)
            commands::ai_cmd::get_or_create_chat_session,
            commands::ai_cmd::list_chat_sessions,
            commands::ai_cmd::list_chat_messages,
            commands::ai_cmd::append_chat_message,
            commands::ai_cmd::clear_chat_session,
            commands::ai_cmd::delete_chat_session,
            // Config & Profiles
            commands::config_cmd::create_llm_profile,
            commands::config_cmd::update_llm_profile,
            commands::config_cmd::list_llm_profiles,
            commands::config_cmd::delete_llm_profile,
            commands::config_cmd::test_llm_connection,
            commands::config_cmd::fetch_remote_models,
            commands::config_cmd::check_missing_keys,
            commands::config_cmd::list_agents,
            commands::config_cmd::update_agent,
            // App settings
            commands::get_app_setting,
            commands::set_app_setting,
            // Trash / Recycle Bin
            commands::list_trash_items,
            commands::restore_trash_item,
            commands::purge_trash_item,
            commands::empty_trash,
            // WebDAV Sync
            commands::sync_cmd::webdav_save_config,
            commands::sync_cmd::webdav_get_config,
            commands::sync_cmd::webdav_test_connection,
            commands::sync_cmd::webdav_backup_now,
            commands::sync_cmd::webdav_backup_preflight,
            commands::sync_cmd::sync_get_device_state,
            commands::sync_cmd::sync_clear_device_notice,
            commands::sync_cmd::webdav_list_backups,
            commands::sync_cmd::webdav_preview_retention,
            commands::sync_cmd::webdav_restore_preflight,
            commands::sync_cmd::webdav_restore_backup,
            // Import
            commands::parse_import_document,
            commands::parse_import_epub,
            commands::commit_import_novel,
            // Export（任务书 T02）
            commands::export_cmd::choose_export_file,
            commands::export_cmd::export_novel,
            commands::export_cmd::cancel_export,
            // 全书搜索与替换（任务书 T09）
            commands::search_cmd::search_chapters,
            commands::search_cmd::preview_replace,
            commands::search_cmd::apply_replace,
            commands::search_cmd::undo_replace_batch,
            // Character Relations
            commands::list_character_relations,
            commands::save_character_relation,
            commands::delete_character_relation,
            // Prompt Templates
            commands::list_prompt_templates,
            commands::save_prompt_template,
            commands::reset_prompt_template,
            // Lexicon / Autocomplete
            commands::list_lexicon,
            commands::save_lexicon_term,
            commands::delete_lexicon_term,
            commands::record_lexicon_usage,
            commands::sync_lexicon_from_novel,
            commands::import_lexicon_terms,
            commands::find_affected_chapters,
            // App Updates & External URLs
            commands::update_cmd::check_app_update,
            commands::update_cmd::open_external_url,
            commands::update_cmd::set_ignored_update_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running easy-writing core");
}
