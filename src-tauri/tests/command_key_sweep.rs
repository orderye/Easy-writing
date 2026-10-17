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
    let pool = db::init_db(dir.clone()).unwrap();
    let novel_id = { let c = pool.get().unwrap(); db::novel::create_novel(&c, "T", "").unwrap().id };
    let volume_id = { let c = pool.get().unwrap(); db::volume::create_volume(&c, &novel_id, "V", "").unwrap().id };
    let chapter_id = { let c = pool.get().unwrap(); db::chapter::create_chapter(&c, &novel_id, &volume_id, "C", "").unwrap().id };
    let session_id = {
        let c = pool.get().unwrap();
        db::chat::ensure_session(&c, &novel_id, Some(&chapter_id)).unwrap().id
    };
    // 图片资产：供图库相关命令的参数键解析校验（save_image 返回 rel_path，故再查一次拿 id）
    let asset_id = {
        let c = pool.get().unwrap();
        easy_writing_lib::asset::save_image(&c, &dir, &novel_id, "pic.png", "image/png", "aGVsbG8=").unwrap();
        easy_writing_lib::asset::list_assets(&c, &novel_id).unwrap()[0].id.clone()
    };
    // 仅用于 update_llm_profile 的参数键解析校验；不写钥匙串，故 key_ref 留空
    let profile_id = {
        let c = pool.get().unwrap();
        db::llm_profile::create_profile(&c, "P", "openai", "http://127.0.0.1:1/v1", "", "m", "{}", false).unwrap().id
    };

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::create_novel,
            easy_writing_lib::commands::list_novels,
            easy_writing_lib::commands::list_novels_with_stats,
            easy_writing_lib::commands::update_novel,
            easy_writing_lib::commands::delete_novel,
            easy_writing_lib::commands::list_deleted_novels,
            easy_writing_lib::commands::list_image_assets,
            easy_writing_lib::commands::rename_image_asset,
            easy_writing_lib::commands::delete_image_asset,
            easy_writing_lib::commands::list_deleted_image_assets,
            easy_writing_lib::commands::restore_image_asset,
            easy_writing_lib::commands::purge_image_asset,
            easy_writing_lib::commands::get_image_asset_usage,
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
            easy_writing_lib::commands::find_affected_chapters,
            easy_writing_lib::commands::sync_cmd::webdav_save_config,
            easy_writing_lib::commands::sync_cmd::webdav_get_config,
            easy_writing_lib::commands::sync_cmd::webdav_test_connection,
            easy_writing_lib::commands::sync_cmd::webdav_list_backups,
            easy_writing_lib::commands::parse_import_document,
            easy_writing_lib::commands::parse_import_epub,
            easy_writing_lib::commands::commit_import_novel,
            easy_writing_lib::commands::list_character_relations,
            easy_writing_lib::commands::save_character_relation,
            easy_writing_lib::commands::delete_character_relation,
            easy_writing_lib::commands::list_prompt_templates,
            easy_writing_lib::commands::save_prompt_template,
            easy_writing_lib::commands::reset_prompt_template,
            easy_writing_lib::commands::list_lexicon,
            easy_writing_lib::commands::save_lexicon_term,
            easy_writing_lib::commands::delete_lexicon_term,
            easy_writing_lib::commands::record_lexicon_usage,
            easy_writing_lib::commands::sync_lexicon_from_novel,
            easy_writing_lib::commands::import_lexicon_terms,
            easy_writing_lib::commands::config_cmd::update_llm_profile,
            easy_writing_lib::commands::config_cmd::fetch_remote_models,
            easy_writing_lib::commands::ai_cmd::get_or_create_chat_session,
            easy_writing_lib::commands::ai_cmd::list_chat_sessions,
            easy_writing_lib::commands::ai_cmd::list_chat_messages,
            easy_writing_lib::commands::ai_cmd::append_chat_message,
            easy_writing_lib::commands::ai_cmd::clear_chat_session,
            easy_writing_lib::commands::ai_cmd::delete_chat_session,
        ])
        .build(tauri::generate_context!()).unwrap();
    app.manage(pool);

    // Raw payloads; @@..@@ placeholders are substituted with the real row ids below.
    let cases: &[(&str, &str)] = &[
        ("create_novel", r#"{"title": "v", "synopsis": "v"}"#),
        ("list_novels", r#"{}"#),
        ("list_novels_with_stats", r#"{}"#),
        ("update_novel", r#"{"id": "@@CHAPTER@@", "title": "v", "synopsis": "v"}"#),
        ("delete_novel", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_deleted_novels", r#"{}"#),
        ("list_image_assets", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("rename_image_asset", r#"{"asset_id": "@@ASSET@@", "name": "v"}"#),
        ("delete_image_asset", r#"{"asset_id": "@@ASSET@@"}"#),
        ("list_deleted_image_assets", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("get_image_asset_usage", r#"{"asset_id": "@@ASSET@@"}"#),
        ("restore_image_asset", r#"{"asset_id": "@@ASSET@@"}"#),
        ("purge_image_asset", r#"{"asset_id": "@@ASSET@@"}"#),
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
        ("find_affected_chapters", r#"{"novel_id": "@@NOVEL@@", "term": "剑"}"#),
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
        ("parse_import_epub", r#"{"file_name": "v.epub", "epub_base64": ""}"#),
        ("commit_import_novel", r#"{"target_novel_id": null, "preview": {"novel_title":"N","total_word_count":10,"volumes":[{"title":"V","chapters":[{"title":"C","content":"{}","content_plain":"x","word_count":1}]}]}}"#),
        ("list_character_relations", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("save_character_relation", r#"{"id": null, "novel_id": "@@NOVEL@@", "from_char_id": "v", "to_char_id": "v", "relation_name": "v", "description": "v"}"#),
        ("delete_character_relation", r#"{"id": "@@CHAPTER@@"}"#),
        ("list_prompt_templates", r#"{}"#),
        ("save_prompt_template", r#"{"key": "k", "template": "v"}"#),
        ("reset_prompt_template", r#"{"key": "k"}"#),
        ("list_lexicon", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("save_lexicon_term", r#"{"novel_id": "@@NOVEL@@", "term": "术语甲", "category": "custom", "source_id": null}"#),
        ("delete_lexicon_term", r#"{"id": "@@CHAPTER@@"}"#),
        ("record_lexicon_usage", r#"{"novel_id": "@@NOVEL@@", "term": "术语甲"}"#),
        ("sync_lexicon_from_novel", r#"{"novel_id": "@@NOVEL@@"}"#),
        ("import_lexicon_terms", r#"{"novel_id": "@@NOVEL@@", "terms": ["术语甲"], "category": "custom"}"#),
        // 生成参数：只校验参数键可解析，不关心网络结果
        ("update_llm_profile", r#"{"id": "@@PROFILE@@", "name": "v", "protocol": "openai", "base_url": "http://127.0.0.1:1/v1", "model": "v", "is_default": true, "temperature": 0.7, "max_tokens": 2048, "stream": false}"#),
        // 指向必失败的地址：只要不是「参数缺失」被拒即算通过
        ("fetch_remote_models", r#"{"protocol": "openai", "base_url": "http://127.0.0.1:1/v1", "api_key": "sk-test", "profile_id": null}"#),
        // 对话记录（PRD §4.1 P1）
        ("get_or_create_chat_session", r#"{"novel_id": "@@NOVEL@@", "chapter_id": "@@CHAPTER@@"}"#),
        ("list_chat_sessions", r#"{"novel_id": "@@NOVEL@@", "chapter_id": null}"#),
        ("list_chat_messages", r#"{"session_id": "@@SESSION@@", "limit": 50}"#),
        ("append_chat_message", r#"{"session_id": "@@SESSION@@", "role": "user", "content": "v"}"#),
        ("clear_chat_session", r#"{"session_id": "@@SESSION@@"}"#),
        ("delete_chat_session", r#"{"session_id": "@@SESSION@@"}"#),
    ];

    // 说明：ai_chat_stream / save_chapter_content / save_image_asset /
    // get_app_data_dir / scan_image_storage / prune_orphan_images /
    // webdav_backup_now / webdav_restore_backup 的参数首参为 tauri::AppHandle，
    // 在 MockRuntime 下无法注册（AppHandle 不实现 CommandArg<MockRuntime>），
    // 因此不在本 sweep 覆盖范围内；它们的键名契约由 ipc_argcase.rs 兜底。

    let fill = |s: &str| {
        s.replace("@@NOVEL@@", &novel_id)
            .replace("@@VOLUME@@", &volume_id)
            .replace("@@CHAPTER@@", &chapter_id)
            .replace("@@ASSET@@", &asset_id)
            .replace("@@SESSION@@", &session_id)
            .replace("@@PROFILE@@", &profile_id)
    };

    // 会对外部世界产生真实、不可回滚副作用的命令，sweep 一律不执行。
    //
    // 为什么必须排除（都是本测试真实踩出来的，不是理论担忧）：
    //
    //   * webdav_save_config —— 把密码写进系统钥匙串，而且用的是**固定** key_ref
    //     （webdav_default_pwd）。第一次运行是新建条目，之后每次都是「修改已存在
    //     条目」，macOS 会弹钥匙串授权框；无人值守的测试进程会**永久阻塞**。
    //     实测表现：sweep 打印到 (60/86) webdav_save_config 后不再前进。
    //   * create_llm_profile —— 每次用随机 key_ref 往钥匙串塞一条新条目，
    //     跑一次留一条、永不清理，是对开发机钥匙串的持续污染。
    //
    // 代价可控：这两个命令的**参数键契约**由
    // ipc_argcase::every_command_declares_snake_case 对全部 97 条命令做源码级
    // 扫描兜底，另有 snake_case_multiword_args_are_the_contract 证明机制本身有效。
    // 本 sweep 负责的是「注册后运行时真的能解析出参数键」，这两条属于已知例外。
    const SIDE_EFFECT_COMMANDS: &[&str] = &["webdav_save_config", "create_llm_profile"];

    let mut rejected = Vec::new();
    let mut checked = 0usize;
    let mut skipped = 0usize;
    for (cmd, raw) in cases {
        // 逐条打印（配合 --nocapture 使用）。
        //
        // 这个背景不是装饰：sweep 会**真的执行**每一条命令，因此任何依赖外部
        // 状态的命令都可能把它挂死。没有逐条输出时完全无法定位 —— 本文件就是
        // 这样才发现它卡在 (60/86) webdav_save_config 上的。
        eprintln!("[sweep] ({checked}/{}) {cmd}", cases.len());

        if SIDE_EFFECT_COMMANDS.contains(cmd) {
            skipped += 1;
            eprintln!("[sweep]   ^ 跳过：该命令有真实且不可回滚的副作用（见 SIDE_EFFECT_COMMANDS 注释）");
            continue;
        }

        let body: serde_json::Value = serde_json::from_str(&fill(raw)).unwrap();
        checked += 1;
        let started = std::time::Instant::now();
        if let Err(e) = call(&app, cmd, body) {
            if e.contains("missing required key") || e.contains("invalid args") {
                rejected.push(format!("{cmd}: {e}"));
            }
        }
        let elapsed = started.elapsed();
        if elapsed.as_secs() >= 1 {
            eprintln!("[sweep]   ^ 耗时 {elapsed:.1?}");
        }
    }
    println!("checked {checked} commands, skipped {skipped}");
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
