//! Regression tests for the IPC argument-naming contract.
//!
//! The Leptos frontend serializes its `*Args` structs with **snake_case** field names
//! (`serde_wasm_bindgen` emits them verbatim and the Tauri JS layer forwards the payload
//! unchanged), so every command declares
//! `#[tauri::command(rename_all = "snake_case")]` and the backend looks up the same keys.
//!
//! Dropping that attribute makes every command with a multi-word parameter fail with
//! `invalid args \`novelId\` for command \`...\`: missing required key novelId` — which
//! silently breaks volume/chapter loading, all setting lists, AI features and, most
//! importantly, chapter saving (`content_plain` / `word_count`).
//!
//! Run: cargo test -p easy-writing-core --test ipc_argcase
use easy_writing_lib::db;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::Manager;

const COMMAND_SOURCES: &[&str] = &[
    include_str!("../src/commands/mod.rs"),
    include_str!("../src/commands/ai_cmd.rs"),
    include_str!("../src/commands/config_cmd.rs"),
    include_str!("../src/commands/sync_cmd.rs"),
    include_str!("../src/commands/update_cmd.rs"),
];

/// Every `#[tauri::command]` must declare the snake_case contract; a new command added
/// without it would silently reject the frontend payload.
#[test]
fn every_command_declares_snake_case() {
    let mut total = 0;
    let mut offenders = Vec::new();
    for src in COMMAND_SOURCES {
        for chunk in src.split("#[tauri::command").skip(1) {
            total += 1;
            if !chunk.starts_with("(rename_all = \"snake_case\")]") {
                offenders.push(chunk.lines().next().unwrap_or("").trim().to_string());
            }
        }
    }
    // 77 → 85：PRD §4.1/§5.1 补齐 8 个命令（6 个对话记录 + update_llm_profile +
    // fetch_remote_models）。已逐条确认均带 rename_all = "snake_case"，
    // 且已加入 command_key_sweep 的参数键契约用例。
    // 85 → 86：作品删除功能补齐 list_deleted_novels（跨作品作用域的回收站入口）。
    // 86 → 88：编辑器插图补齐 get_app_data_dir 与 list_image_assets。
    // 三者均确认带 snake_case 属性；其中 get_app_data_dir 首参为 AppHandle，
    // 在 MockRuntime 下无法注册（见 command_key_sweep 的说明），
    // 另两个已加入 sweep 用例。
    // 88 → 87：删除 get_novel 命令（前端无调用，等价数据源为 list_novels 与
    // 前端持有的 current_novel；db::novel::get_novel 作为内部函数保留）。
    // 87 → 93：图库素材管理补齐 6 个命令（rename/delete/list_deleted/restore/
    // purge/get_image_asset_usage），均确认带 rename_all = "snake_case"
    // 并已加入 command_key_sweep 的参数键用例。
    // 93 → 95：图库「维护」补齐 scan_image_storage 与 prune_orphan_images
    // （均带 snake_case；首参为 AppHandle，故无法进入 command_key_sweep，
    //  与 save_image_asset / get_app_data_dir 同属已知的 MockRuntime 限制）。
    // 95 → 96：导入作品增加 parse_import_epub（带 snake_case，已加入 command_key_sweep）。
    // 96 → 97：作品管理增加 list_novels_with_stats（带 snake_case，已加入 command_key_sweep）。
    assert_eq!(total, 97, "command count changed — re-audit the frontend call sites");
    assert!(
        offenders.is_empty(),
        "these commands are missing rename_all = \"snake_case\" and will reject the \
         frontend payload: {offenders:?}"
    );
}

/// Multi-word keys from the frontend must be accepted, and the camelCase form must not be
/// the contract we accidentally rely on.
#[test]
fn snake_case_multiword_args_are_the_contract() {
    let (app, novel_id) = app_with_pool();

    let vols = call(&app, "list_volumes", serde_json::json!({ "novel_id": novel_id }));
    assert!(vols.is_ok(), "list_volumes rejected: {vols:?}");

    let trash_camel = call(
        &app,
        "list_trash_items",
        serde_json::json!({ "novelId": novel_id }),
    );
    assert!(
        trash_camel.is_err(),
        "camelCase unexpectedly works — the frontend contract changed; update the Args structs"
    );
}

/// The full authoring flow using only the keys the UI sends.
#[test]
fn end_to_end_authoring_flow() {
    let (app, novel_id) = app_with_pool();

    let vol = call(
        &app,
        "create_volume",
        serde_json::json!({ "novel_id": novel_id, "title": "第一卷", "summary": "卷简介" }),
    )
    .expect("create_volume");
    let volume_id = vol["id"].as_str().unwrap().to_string();

    let ch = call(
        &app,
        "create_chapter",
        serde_json::json!({
            "novel_id": novel_id, "volume_id": volume_id,
            "title": "第一章", "summary": "章摘要"
        }),
    )
    .expect("create_chapter");
    let chapter_id = ch["id"].as_str().unwrap().to_string();

    // The save button / autosave path. `save_chapter_content` takes a Wry-typed
    // AppHandle so it cannot be registered on the MockRuntime; its argument contract is
    // covered by `every_command_declares_snake_case`, and the data path is exercised here.
    {
        let pool = app.state::<db::DbPool>();
        let conn = pool.get().unwrap();
        db::chapter::save_chapter_content(
            &conn,
            &chapter_id,
            "{\"type\":\"doc\",\"content\":[]}",
            "正文内容",
            4,
        )
        .expect("save_chapter_content — without this the editor silently loses text");
    }

    let headers = call(
        &app,
        "list_chapter_headers",
        serde_json::json!({ "volume_id": volume_id }),
    )
    .expect("list_chapter_headers");
    assert_eq!(headers.as_array().unwrap().len(), 1);

    let full = call(&app, "get_chapter", serde_json::json!({ "id": chapter_id }))
        .expect("get_chapter");
    assert_eq!(full["word_count"], 4);
    assert_eq!(full["content_plain"], "正文内容");
}

/// Settings panels: worldview / characters / plots / relations / trash all key off novel_id.
#[test]
fn settings_and_assets_write_paths() {
    let (app, novel_id) = app_with_pool();

    call(
        &app,
        "save_worldview",
        serde_json::json!({
            "id": null, "novel_id": novel_id,
            "group_name": "势力", "title": "青云宗", "content": "天下第一宗"
        }),
    )
    .expect("save_worldview");

    let wv = call(&app, "list_worldview", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_worldview");
    assert_eq!(wv.as_array().unwrap().len(), 1);
    assert_eq!(wv[0]["group_name"], "势力");

    call(
        &app,
        "save_character",
        serde_json::json!({
            "id": null, "novel_id": novel_id,
            "name": "陈云舟", "aliases": "云舟", "profile": "主角"
        }),
    )
    .expect("save_character");

    let chars = call(&app, "list_characters", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_characters");
    assert_eq!(chars.as_array().unwrap().len(), 1);

    call(
        &app,
        "save_plot",
        serde_json::json!({
            "id": null, "novel_id": novel_id, "title": "初现端倪",
            "content": "主线冲突", "kind": "foreshadow", "status": "planted", "chapter_id": null
        }),
    )
    .expect("save_plot");

    let plots = call(&app, "list_plots", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_plots");
    assert_eq!(plots.as_array().unwrap().len(), 1);

    // 资产读取路径（原先本函数名为 settings_and_assets_write_paths，却没有任何
    // 一条资产调用——list_image_assets 因此长期没有参数键覆盖）。
    call(&app, "list_image_assets", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_image_assets");
    call(
        &app,
        "list_deleted_image_assets",
        serde_json::json!({ "novel_id": novel_id }),
    )
    .expect("list_deleted_image_assets");
    call(&app, "list_trash_items", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_trash_items");
    call(&app, "list_setting_changes", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_setting_changes");
    call(&app, "empty_trash", serde_json::json!({ "novel_id": novel_id }))
        .expect("empty_trash");
}

/// Single-word parameters keep working (snake_case is a no-op for them).
#[test]
fn single_word_args_still_work() {
    let (app, _) = app_with_pool();
    assert!(call(&app, "list_novels", serde_json::json!({})).is_ok());
    call(
        &app,
        "set_app_setting",
        serde_json::json!({ "key": "k", "value": "v" }),
    )
    .unwrap();
    assert_eq!(
        call(&app, "get_app_setting", serde_json::json!({ "key": "k" })).unwrap(),
        serde_json::json!("v")
    );
}

fn call(
    app: &tauri::App<tauri::test::MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let label = format!("w{}", N.fetch_add(1, Ordering::SeqCst));
    let webview = tauri::WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .unwrap();
    let res = tauri::test::get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    );
    match res {
        Ok(b) => Ok(b.deserialize::<serde_json::Value>().unwrap()),
        Err(e) => Err(e.to_string()),
    }
}

fn app_with_pool() -> (tauri::App<tauri::test::MockRuntime>, String) {
    let dir = std::env::temp_dir().join(format!("ew_ipc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let pool = db::init_db(dir).unwrap();
    let novel_id = {
        let conn = pool.get().unwrap();
        db::novel::create_novel(&conn, "T", "").unwrap().id
    };
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::list_novels,
            easy_writing_lib::commands::create_volume,
            easy_writing_lib::commands::list_volumes,
            easy_writing_lib::commands::create_chapter,
            easy_writing_lib::commands::list_chapter_headers,
            easy_writing_lib::commands::get_chapter,
            easy_writing_lib::commands::save_worldview,
            easy_writing_lib::commands::list_worldview,
            easy_writing_lib::commands::save_character,
            easy_writing_lib::commands::list_characters,
            easy_writing_lib::commands::save_plot,
            easy_writing_lib::commands::list_plots,
            easy_writing_lib::commands::list_image_assets,
            easy_writing_lib::commands::list_deleted_image_assets,
            easy_writing_lib::commands::list_trash_items,
            easy_writing_lib::commands::list_setting_changes,
            easy_writing_lib::commands::empty_trash,
            easy_writing_lib::commands::set_app_setting,
            easy_writing_lib::commands::get_app_setting,
        ])
        .build(tauri::generate_context!())
        .unwrap();
    app.manage(pool);
    (app, novel_id)
}
