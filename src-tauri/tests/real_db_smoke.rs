//! Smoke test against a COPY of the real application database.
//!
//! Verifies that data the app previously could not read (because every multi-word
//! argument key was rejected) is now returned through the real command layer.
//!
//! Set EW_REAL_DB to the directory holding a copy of `easy-writing.db`.
use easy_writing_lib::db;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::Manager;

#[test]
fn real_user_database_is_readable() {
    let dir = match std::env::var("EW_REAL_DB") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => {
            eprintln!("EW_REAL_DB not set — skipping real-database smoke test");
            return;
        }
    };

    let pool = db::init_db(dir).expect("open real db copy");
    let novel_id = {
        let conn = pool.get().unwrap();
        db::novel::list_novels(&conn).unwrap()[0].id.clone()
    };

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::list_volumes,
            easy_writing_lib::commands::list_chapter_headers,
            easy_writing_lib::commands::list_worldview,
            easy_writing_lib::commands::list_characters,
            easy_writing_lib::commands::list_plots,
            easy_writing_lib::commands::list_all_chapter_headers_for_novel,
        ])
        .build(tauri::generate_context!())
        .unwrap();
    app.manage(pool);

    // Same shapes the UI sends: multi-word snake_case keys.
    let vols = call(&app, "list_volumes", serde_json::json!({ "novel_id": novel_id }))
        .expect("list_volumes must succeed");
    let vols = vols.as_array().unwrap();
    assert!(!vols.is_empty(), "the user's volumes must now be visible");
    let volume_id = vols[0]["id"].as_str().unwrap().to_string();

    let chs = call(
        &app,
        "list_chapter_headers",
        serde_json::json!({ "volume_id": volume_id }),
    )
    .expect("list_chapter_headers must succeed");
    assert!(!chs.as_array().unwrap().is_empty(), "chapters must now be visible");

    for (cmd, body) in [
        ("list_worldview", serde_json::json!({ "novel_id": novel_id })),
        ("list_characters", serde_json::json!({ "novel_id": novel_id })),
        ("list_plots", serde_json::json!({ "novel_id": novel_id })),
        (
            "list_all_chapter_headers_for_novel",
            serde_json::json!({ "novel_id": novel_id }),
        ),
    ] {
        call(&app, cmd, body).unwrap_or_else(|e| panic!("{cmd} failed: {e}"));
    }

    println!(
        "real DB OK — volumes: {}, chapters in first volume: {}",
        vols.len(),
        chs.as_array().unwrap().len()
    );
}

fn call(
    app: &tauri::App<tauri::test::MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let label = format!("r{}", N.fetch_add(1, Ordering::SeqCst));
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
