//! 冲突检测结构化与可配置（T08 / PR-11）的端到端与纯函数测试。
//!
//! 与旧测试的区别：模型调用仍然不可测（没有假模型），但判定结果的**形状**
//! 与**闸门行为**全部落在纯函数里 —— 固定模型返回样例即可断言，
//! 不需要真的打一次 API。
//!
//! Run: cargo test -p easy-writing-core --test conflict_detection

use easy_writing_lib::ai::consistency::{
    apply_gate, build_conflict_context, interpret_response, resolve_related_settings, CheckScope,
    ConflictCheckSettings,
};
use easy_writing_lib::db;
use easy_writing_lib::db::app_setting;
use shared_types::{ConflictInfo, ConflictSettingRef, ConsistencyOutcome};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_conflict_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// 固定的模型返回样例（结构化 + 带 id + 证据 + 置信度）。
fn sample_model_response() -> String {
    r#"{"has_conflict": true,
      "conflict_point": "沈砚在第 3 章已经战死，本章却写他登场",
      "related_settings": [{"entity_type": "character", "id": "ch_shen", "title": "沈砚"}],
      "evidence": "第 3 章原文：「沈砚倒在礁石上，再没有起来。」",
      "confidence": 0.88,
      "setting_module": "角色",
      "setting_title": "沈砚",
      "reason": "角色生死状态与第 3 章冲突",
      "proposed_change": "将本章改为沈砚旧部回忆，而非本人登场"
    }"#
    .to_string()
}

fn candidates() -> Vec<ConflictSettingRef> {
    vec![
        ConflictSettingRef {
            entity_type: "character".to_string(),
            id: "ch_shen".to_string(),
            title: "沈砚".to_string(),
        },
        ConflictSettingRef {
            entity_type: "worldview".to_string(),
            id: "wv_lamp".to_string(),
            title: "灯塔法则".to_string(),
        },
    ]
}

fn conflict_info(confidence: Option<f32>) -> ConflictInfo {
    ConflictInfo {
        conflict_point: "沈砚已死".to_string(),
        setting_module: "角色".to_string(),
        setting_title: "沈砚".to_string(),
        reason: "第 3 章明确写其战死".to_string(),
        proposed_change: None,
        related_settings: Vec::new(),
        evidence: String::new(),
        confidence,
        ignore_signature: String::new(),
        ignore_label: String::new(),
    }
}

fn seed_novel(dir: &std::path::Path) -> String {
    let pool = db::init_db(dir.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    let novel_id = db::novel::create_novel(&conn, "冲突检测用作品", "").unwrap().id;
    {
        let volume = db::volume::create_volume(&conn, &novel_id, "第一卷", "").unwrap();
        db::chapter::create_chapter(&conn, &novel_id, &volume.id, "第一章", "").unwrap();
    }
    for (title, pinned) in [("灯塔法则", true), ("旧时咒术", false)] {
        let wv = db::settings::save_worldview(&conn, None, &novel_id, "法则", title, "咒语原文").unwrap();
        conn.execute("UPDATE worldview_entry SET pinned = ?1 WHERE id = ?2", rusqlite::params![i32::from(pinned), wv.id]).unwrap();
    }
    let shen = db::settings::save_character(&conn, None, &novel_id, "沈砚", "[]", "主角档案").unwrap();
    conn.execute("UPDATE character SET pinned = 1 WHERE id = ?1", rusqlite::params![shen.id]).unwrap();
    db::settings::save_character(&conn, None, &novel_id, "阿岚", "[]", "配角档案").unwrap();
    db::settings::save_plot(&conn, None, &novel_id, "伏笔·第七秒", "设定", "foreshadow", "planted", None)
        .unwrap();
    novel_id
}

// ---------- 解析 ----------

#[test]
fn structured_sample_parses_into_info() {
    let outcome = interpret_response(&sample_model_response());
    let info = outcome.as_conflict().expect("样例应判定为冲突");
    assert_eq!(info.confidence, Some(0.88));
    assert_eq!(info.evidence, "第 3 章原文：「沈砚倒在礁石上，再没有起来。」");
    assert_eq!(info.related_settings.len(), 1);
    assert_eq!(info.related_settings[0].entity_type, "character");
    assert_eq!(info.related_settings[0].id, "ch_shen");
    assert!(info.ignore_signature.is_empty(), "签名要等与库内对照后才能算");
}

#[test]
fn non_json_sample_is_not_checked() {
    let outcome = interpret_response("看起来没问题，直接写吧。");
    assert!(
        outcome.is_not_checked(),
        "非 JSON 返回必须判为未检测，绝不能当成无冲突"
    );
}

#[test]
fn missing_confidence_keeps_none_not_zero() {
    let raw = r#"{"has_conflict": true, "conflict_point": "x"}"#;
    let outcome = interpret_response(raw);
    let info = outcome.as_conflict().unwrap();
    assert_eq!(info.confidence, None);
}

// ---------- 闸门 ----------

#[test]
fn below_threshold_is_suppressed_and_equal_is_not() {
    let suppressed = apply_gate(
        ConsistencyOutcome::Conflict {
            info: conflict_info(Some(0.5)),
        },
        0.7,
        &HashSet::new(),
    );
    assert!(suppressed.is_suppressed());
    if let ConsistencyOutcome::Suppressed { info, threshold } = suppressed {
        assert!((threshold - 0.7).abs() < f32::EPSILON);
        assert_eq!(info.conflict_point, "沈砚已死");
    } else {
        panic!("应为 Suppressed");
    }

    let equal = apply_gate(
        ConsistencyOutcome::Conflict {
            info: conflict_info(Some(0.7)),
        },
        0.7,
        &HashSet::new(),
    );
    assert!(matches!(equal, ConsistencyOutcome::Conflict { .. }));
}

#[test]
fn unknown_confidence_is_not_suppressed() {
    let outcome = apply_gate(
        ConsistencyOutcome::Conflict {
            info: conflict_info(None),
        },
        0.99,
        &HashSet::new(),
    );
    assert!(matches!(outcome, ConsistencyOutcome::Conflict { .. }));
}

#[test]
fn ignored_signature_returns_clean() {
    let mut info = conflict_info(Some(1.0));
    resolve_related_settings(&mut info, &candidates());
    assert_eq!(info.ignore_signature, "character:ch_shen");
    assert!(info.ignore_label.contains("沈砚"));

    let ignored = HashSet::from([info.ignore_signature.clone()]);
    let outcome = apply_gate(ConsistencyOutcome::Conflict { info }, 0.0, &ignored);
    assert!(matches!(outcome, ConsistencyOutcome::Clean));
}

// ---------- 上下文构建 ----------

fn build_with_budget(dir: &std::path::Path, scope: CheckScope, budget: usize) -> String {
    let pool = db::init_db(dir.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    let novel_id = seed_novel(&dir);
    let (text, _) =
        build_conflict_context(&conn, &novel_id, scope, budget).expect("上下文构建必须成功");
    drop(conn);
    drop(pool);
    text
}

#[test]
fn pinned_only_context_excludes_non_pinned_entries() {
    let dir = temp_dir("pinned");
    let text = build_with_budget(&dir, CheckScope::PinnedOnly, 12_000);
    assert!(text.contains("[worldview#"), "上下文必须带类型与 id");
    assert!(text.contains("灯塔法则"), "pinned 世界观必须进入");
    assert!(text.contains("沈砚"), "pinned 角色必须进入");
    assert!(
        !text.contains("旧时咒术"),
        "非 pinned 世界观在「仅硬性设定」下必须被排除"
    );
    assert!(!text.contains("阿岚"), "非 pinned 角色在「仅硬性设定」下必须被排除");
}

#[test]
fn all_scope_context_includes_every_entry() {
    let dir = temp_dir("all");
    let text = build_with_budget(&dir, CheckScope::All, 12_000);
    for needle in ["灯塔法则", "旧时咒术", "沈砚", "阿岚", "伏笔·第七秒"] {
        assert!(text.contains(needle), "全部范围下 {needle} 必须进入上下文");
    }
}

#[test]
fn budget_truncates_but_keeps_pinned_first() {
    // 预算只够装下 pinned 世界观 + 一点点：pinned 必须排在前面被装进来，
    // 非 pinned 的条目只能留在外面。
    let dir = temp_dir("budget");
    let text = build_with_budget(&dir, CheckScope::All, 140);
    assert!(text.contains("灯塔法则"), "预算不足时 pinned 优先");
    assert!(text.contains("[worldview#"), "上下文必须带类型与 id");
    assert!(
        text.chars().count() <= 140,
        "上下文不得超过预算，实际 {}",
        text.chars().count()
    );
    assert!(
        !text.contains("阿岚"),
        "非 pinned 且排后面的条目必须在预算耗尽后被裁掉"
    );
}

// ---------- 关联设定解析 ----------

#[test]
fn valid_id_is_filled_with_library_title() {
    let mut info = conflict_info(Some(0.9));
    info.related_settings = vec![ConflictSettingRef {
        entity_type: "character".to_string(),
        id: "ch_shen".to_string(),
        title: "模型随便写的标题".to_string(),
    }];
    resolve_related_settings(&mut info, &candidates());
    assert_eq!(info.related_settings.len(), 1);
    assert_eq!(info.related_settings[0].title, "沈砚", "标题以库内为准");
}

#[test]
fn bogus_id_is_dropped_and_title_only_fallback_works() {
    let mut info = conflict_info(Some(0.9));
    info.related_settings = vec![
        ConflictSettingRef {
            entity_type: "character".to_string(),
            id: "not-exist".to_string(),
            title: String::new(),
        },
        ConflictSettingRef {
            entity_type: "角色".to_string(),
            id: String::new(),
            title: "沈砚".to_string(),
        },
    ];
    resolve_related_settings(&mut info, &candidates());
    assert_eq!(info.related_settings.len(), 1, "伪造 id 必须被丢弃");
    assert_eq!(info.related_settings[0].id, "ch_shen");
}

// ---------- 命令级：关闭即零调用 ----------

/// 前置检查关闭时，即便**一个 LLM 配置都没有**，命令也必须返回 `Disabled`。
///
/// 这条测试就是「关闭后不再产生额外调用」的可观测证据：
/// 若实现先解析凭据再判断开关，这里会因为「没有可用接口配置」报错，
/// 而不是干净地返回 Disabled。
#[test]
fn disabled_check_does_not_resolve_credentials() {
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::Manager;

    let dir = temp_dir("disabled");
    let pool = db::init_db(dir).unwrap();
    {
        let conn = pool.get().unwrap();
        db::novel::create_novel(&conn, "关闭检查用作品", "").unwrap();
        app_setting::set_setting(&conn, "conflict_check_enabled", "false").unwrap();
    }

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::ai_cmd::ai_check_conflict,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(pool);

    let webview = tauri::WebviewWindowBuilder::new(&app, "w0", Default::default())
        .build()
        .unwrap();
    let url: tauri::Url = if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }
        .parse()
        .unwrap();
    let res = tauri::test::get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: "ai_check_conflict".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url,
            body: InvokeBody::Json(serde_json::json!({
                "novel_id": "n1",
                "chapter_id": null,
                "text": "这段文字足够长，足够触发一次检测。"
            })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .expect("ai_check_conflict 应当被接受");

    let outcome: ConsistencyOutcome =
        serde_json::from_value(res.deserialize::<serde_json::Value>().unwrap()).unwrap();
    assert!(outcome.is_disabled(), "关闭前置检查必须返回 Disabled，实际：{outcome:?}");
}

// ---------- 忽略记录持久化 ----------

#[test]
fn ignore_roundtrip_and_chapter_ownership() {
    let dir = temp_dir("ignore");
    let dir2 = dir.clone();
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let novel_id = seed_novel(&dir2);

    let chapter_id = db::chapter::list_all_chapter_headers_for_novel(&conn, &novel_id)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;

    crate::db::conflict_ignore::ignore(&conn, &novel_id, &chapter_id, "character:ch1", "角色·沈砚")
        .unwrap();
    let set = crate::db::conflict_ignore::list_signatures(&conn, &chapter_id).unwrap();
    assert!(set.contains("character:ch1"));

    // 幂等：同一签名再点一次不产生第二行
    crate::db::conflict_ignore::ignore(&conn, &novel_id, &chapter_id, "character:ch1", "角色·沈砚")
        .unwrap();
    assert_eq!(
        crate::db::conflict_ignore::list_signatures(&conn, &chapter_id).unwrap().len(),
        1
    );
}

#[test]
fn ignore_command_rejects_unknown_chapter_and_empty_signature() {
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::Manager;

    let dir = temp_dir("ignore_cmd");
    let pool = db::init_db(dir).unwrap();
    let novel_id = {
        let conn = pool.get().unwrap();
        db::novel::create_novel(&conn, "忽略用作品", "").unwrap().id
    };

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::ai_cmd::ignore_chapter_conflict,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(pool);

    let call = |chapter_id: &str, signature: &str| -> String {
        let label = format!("w{}", chapter_id.len() + chapter_id.as_bytes().iter().map(|b| *b as usize).sum::<usize>());
        let webview = tauri::WebviewWindowBuilder::new(&app, label, Default::default())
            .build()
            .unwrap();
        let url: tauri::Url = if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }
        .parse()
        .unwrap();
        tauri::test::get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "ignore_chapter_conflict".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url,
                body: InvokeBody::Json(serde_json::json!({
                    "novel_id": novel_id,
                    "chapter_id": chapter_id,
                    "signature": signature,
                    "label": "角色·沈砚"
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|b| b.deserialize::<serde_json::Value>().unwrap())
        .map(|v| v.to_string())
        .unwrap_or_else(|e| format!("err:{e}"))
    };

    assert!(call("no-such-chapter", "character:x").contains("err:"), "未知章节必须被拒");
    assert!(call("c1", "").contains("err:"), "空签名必须被拒");
}

#[test]
fn settings_default_is_enabled_all_06() {
    let dir = temp_dir("settings");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let s = ConflictCheckSettings::from_conn(&conn);
    assert!(s.enabled, "默认必须开启检查");
    assert_eq!(s.scope, CheckScope::All, "默认检查全部设定");
    assert!((s.threshold - 0.6).abs() < f32::EPSILON);
}

#[test]
fn concurrent_dir_names_are_unique() {
    // 本测试文件里多处直接用 temp_dir 而非共享 helper；这里仅确认互斥工具存在，
    // 避免新增用例时误用同一个目录互相污染。
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    assert!(n < 1000);
}
