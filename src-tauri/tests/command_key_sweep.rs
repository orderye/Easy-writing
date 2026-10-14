// Key-contract sweep: calls every command registrable under MockRuntime with the exact
// snake_case keys the frontend sends, and asserts none fail on argument lookup.
// Keep the handler list and `cases` in sync when commands are added or their parameters
// change; `ipc_argcase.rs::every_command_declares_snake_case` guards the attribute itself.
use easy_writing_lib::db;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::Manager;

#[test]
fn all_command_keys_resolve() {
    let dir = std::env::temp_dir().join(format!("ew_sweep_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let pool = db::init_db(dir).unwrap();
    let novel_id = { let c = pool.get().unwrap(); db::novel::create_novel(&c, "T", "").unwrap().id };
    let volume_id = { let c = pool.get().unwrap(); db::volume::create_volume(&c, &novel_id, "V", "").unwrap().id };
    let chapter_id = { let c = pool.get().unwrap(); db::chapter::create_chapter(&c, &novel_id, &volume_id, "C", "").unwrap().id };

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::create_novel,
            easy_writing_lib::commands::list_novels,
            easy_writing_lib::commands::get_novel,
            easy_writing_lib::commands::update_novel,
            easy_writing_lib::commands::delete_novel,
            easy_writing_lib::commands::create_volume,
            easy_writing_lib::commands::list_volumes,
            easy_writing_lib::commands::update_volume,
            easy_writing_lib::commands::reorder_volumes,
            easy_writing_lib::commands::delete_volume,
            easy_writing_lib::commands::create_chapter,
            easy_writing_lib::commands::list_chapter_headers,
            easy_writing_lib::commands::list_all_chapter_headers_for_novel,
            easy_writing_lib::commands::get_chapter,
            easy_writing_lib::commands::update_chapter_meta,
            easy_writing_lib::commands::move_chapter,
            easy_writing_lib::commands::reorder_chapters,
            easy_writing_lib::commands::delete_chapter,
            easy_writing_lib::commands::list_chapter_revisions,
            easy_writing_lib::commands::get_chapter_revision,
            easy_writing_lib::commands::create_manual_chapter_revision,
            easy_writing_lib::commands::revert_chapter_to_revision,
            easy_writing_lib::commands::list_worldview,
            easy_writing_lib::commands::save_worldview,
            easy_writing_lib::commands::delete_worldview,
            easy_writing_lib::commands::list_characters,
            easy_writing_lib::commands::save_character,
            easy_writing_lib::commands::delete_character,
            easy_writing_lib::commands::list_plots,
            easy_writing_lib::commands::save_plot,
            easy_writing_lib::commands::delete_plot,
            easy_writing_lib::commands::list_setting_changes,
            easy_writing_lib::commands::revert_setting_change,
            easy_writing_lib::commands::apply_conflict_setting_update,
            easy_writing_lib::commands::ai_cmd::ai_polish,
            easy_writing_lib::commands::ai_cmd::ai_check_conflict,
            easy_writing_lib::commands::ai_cmd::trigger_chapter_extract,
            easy_writing_lib::commands::ai_cmd::list_extraction_candidates,
            easy_writing_lib::commands::ai_cmd::resolve_candidate,
            easy_writing_lib::commands::config_cmd::create_llm_profile,
            easy_writing_lib::commands::config_cmd::list_llm_profiles,
            easy_writing_lib::commands::config_cmd::delete_llm_profile,
            easy_writing_lib::commands::config_cmd::test_llm_connection,
            easy_writing_lib::commands::config_cmd::list_agents,
            easy_writing_lib::commands::config_cmd::update_agent,
            easy_writing_lib::commands::get_app_setting,
            easy_writing_lib::commands::set_app_setting,
            easy_writing_lib::commands::list_trash_items,
            easy_writing_lib::commands::restore_trash_item,
            easy_writing_lib::commands::purge_trash_item,
            easy_writing_lib::commands::empty_trash,
            easy_writing_lib::commands::sync_cmd::webdav_save_config,
            easy_writing_lib::commands::sync_cmd::webdav_get_config,
            easy_writing_lib::commands::sync_cmd::webdav_test_connection,
            easy_writing_lib::commands::sync_cmd::webdav_list_backups,
            easy_writing_lib::commands::parse_import_document,
            easy_writing_lib::commands::commit_import_novel,
            easy_writing_lib::commands::list_character_relations,
            easy_writing_lib::commands::save_character_relation,
            easy_writing_lib::commands::delete_character_relation,
            easy_writing_lib::commands::list_prompt_templates,
            easy_writing_lib::commands::save_prompt_template,
            easy_writing_lib::commands::reset_prompt_template,
        ])
        .build(tauri::generate_context!()).unwrap();
    app.manage(pool);

    // Raw payloads; @@..@@ placeholders are substituted with the real row ids below.
    let cases: &[(&str, &str)] = &[
        ("create_novel", r#"{"title": "v", "synopsis": "v"}"#),
        ("list_novels", r#"{}"#),
        ("get_novel", r#"{"id": "@@CHAPTER@@"}"#),
        ("update_novel", r#"{"id": "@@CHAPTER@@", "title": "v", "synopsis": "v"}"#),
        ("delete_novel", r#"{"id": "@@CHAPTER@@"}"#),
        ("create_volume", r#"{"novel_id": "@@NOVEL@@", "title": "v", "summary": "v"}"#),
        ("list_volumes", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("update_volume", r#"{"id": "@@CHAPTER@@", "title": "v", "summary": "v"}"#),
        ("reorder_volumes", r#"{"volume_ids": []}"#),
        ("delete_volume", r#"{"id": "@@CHAPTER@@"}"#),
        ("create_chapter", r#"{"novel_id": "@@NOVEL@@", "volume_id": "@@VOLUME@@", "title": "v", "summary": "v"}"#),
        ("list_chapter_headers", r#"{"volume_id": "@@VOLUME@@"}"#),
        ("list_all_chapter_headers_for_novel", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("get_chapter", r#"{"id": "@@CHAPTER@@"}"#),
        ("update_chapter_meta", r#"{"id": "@@CHAPTER@@", "title": "v", "summary": "v", "status": "planted"}"#),
        ("move_chapter", r#"{"chapter_id": "v", "new_volume_id": "v", "new_sort_order": 0}"#),
        ("reorder_chapters", r#"{"chapter_ids": []}"#),
        ("delete_chapter", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_chapter_revisions", r#"{"chapter_id": "v"}"#),
        ("get_chapter_revision", r#"{"id": "@@CHAPTER@@"}"#),
        ("create_manual_chapter_revision", r#"{"chapter_id": "v", "note": "v"}"#),
        ("revert_chapter_to_revision", r#"{"chapter_id": "v", "revision_id": "v"}"#),
        ("list_worldview", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("save_worldview", r#"{"id": null, "novel_id": "@@NOVEL@@", "group_name": "v", "title": "v", "content": "v"}"#),
        ("delete_worldview", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_characters", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("save_character", r#"{"id": null, "novel_id": "@@NOVEL@@", "name": "v", "aliases": "v", "profile": "v"}"#),
        ("delete_character", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_plots", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("save_plot", r#"{"id": null, "novel_id": "@@NOVEL@@", "title": "v", "content": "v", "kind": "foreshadow", "status": "planted", "chapter_id": null}"#),
        ("delete_plot", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_setting_changes", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("revert_setting_change", r#"{"change_id": "v"}"#),
        ("apply_conflict_setting_update", r#"{"novel_id": "@@NOVEL@@", "chapter_id": null, "setting_module": "worldview", "setting_title": "v", "new_content": "v", "reason": "v"}"#),
        ("ai_polish", r#"{"module_key": "v", "current_content": "v"}"#),
        ("ai_check_conflict", r#"{"novel_id": "@@NOVEL@@", "text": "v"}"#),
        ("trigger_chapter_extract", r#"{"novel_id": "@@NOVEL@@", "chapter_id": "v"}"#),
        ("list_extraction_candidates", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("resolve_candidate", r#"{"candidate_id": "v", "action": "ignore"}"#),
        ("create_llm_profile", r#"{"name": "v", "protocol": "openai", "base_url": "http://127.0.0.1:1/v1", "model": "v", "api_key": "sk-test", "is_default": true}"#),
        ("list_llm_profiles", r#"{}"#),
        ("delete_llm_profile", r#"{"id": "@@CHAPTER@@"}"#),
        ("test_llm_connection", r#"{"profile_id": "v"}"#),
        ("list_agents", r#"{}"#),
        ("update_agent", r#"{"key": "k", "system_prompt": "v", "llm_profile_id": null, "enabled": true}"#),
        ("get_app_setting", r#"{"key": "k"}"#),
        ("set_app_setting", r#"{"key": "k", "value": "v"}"#),
        ("list_trash_items", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("restore_trash_item", r#"{"item_type": "chapter", "item_id": "@@CHAPTER@@"}"#),
        ("purge_trash_item", r#"{"item_type": "chapter", "item_id": "@@CHAPTER@@"}"#),
        ("empty_trash", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("webdav_save_config", r#"{"url": "http://127.0.0.1:1/dav", "username": "v", "password": "pw", "remote_dir": "v"}"#),
        ("webdav_get_config", r#"{}"#),
        ("webdav_test_connection", r#"{}"#),
        ("webdav_list_backups", r#"{}"#),
        ("parse_import_document", r#"{"file_name": "v", "raw_text": "chapter one text"}"#),
        ("commit_import_novel", r#"{"target_novel_id": null, "preview": {"novel_title":"N","total_word_count":10,"volumes":[{"title":"V","chapters":[{"title":"C","content":"{}","content_plain":"x","word_count":1}]}]}}"#),
        ("list_character_relations", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("save_character_relation", r#"{"id": null, "novel_id": "@@NOVEL@@", "from_char_id": "v", "to_char_id": "v", "relation_name": "v", "description": "v"}"#),
        ("delete_character_relation", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_prompt_templates", r#"{}"#),
        ("save_prompt_template", r#"{"key": "k", "template": "v"}"#),
        ("reset_prompt_template", r#"{"key": "k"}"#),
    ];

    let fill = |s: &str| {
        s.replace("@@NOVEL@@", &novel_id)
            .replace("@@VOLUME@@", &volume_id)
            .replace("@@CHAPTER@@", &chapter_id)
    };

    let mut rejected = Vec::new();
    let mut checked = 0usize;
    for (cmd, raw) in cases {
        let body: serde_json::Value = serde_json::from_str(&fill(raw)).unwrap();
        checked += 1;
        if let Err(e) = call(&app, cmd, body) {
            if e.contains("missing required key") || e.contains("invalid args") {
                rejected.push(format!("{cmd}: {e}"));
            }
        }
    }
    println!("checked {checked} commands");
    assert!(rejected.is_empty(), "commands rejected on argument lookup:\n{}", rejected.join("\n"));
}

fn call(app: &tauri::App<tauri::test::MockRuntime>, cmd: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let label = format!("s{}", N.fetch_add(1, Ordering::SeqCst));
    let webview = tauri::WebviewWindowBuilder::new(app, label, Default::default()).build().unwrap();
    let res = tauri::test::get_ipc_response(&webview, InvokeRequest {
        cmd: cmd.into(), callback: CallbackFn(0), error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(), body: InvokeBody::Json(body),
        headers: Default::default(), invoke_key: INVOKE_KEY.to_string(),
    });
    match res { Ok(b) => Ok(b.deserialize::<serde_json::Value>().unwrap()), Err(e) => Err(e.to_string()) }
}
