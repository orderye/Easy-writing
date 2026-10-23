//! T06 上下文装配透明度：`get_context_manifest` 与 `set_entry_pinned`
//! 的命令级集成测试。
//!
//! 装配逻辑本身（优先级、命中、裁剪）的单元用例在 `src/ai/context.rs`
//! 的 `#[cfg(test)]` 里；本文件钉住的是**命令契约**：
//!   * 预算解析链与 `ai_chat_stream` 完全一致
//!     （`llm_profile.params.context_window` 缺省 8k，字符预算按 1.5 字符/token 换算）；
//!   * 清单逐条记录入选原因，前端「本次上下文」面板据此渲染；
//!   * pinned 标记持久化到库，切换后立即反映在装配结果里。

use easy_writing_lib::db::{self, DbPool};
use shared_types::ContextManifest;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::Manager;

static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 集成测试 fixture：
/// 作品《测试之书》+ 三章（c2 为当前章，正文命中角色「沈砚」与词库术语「灵气」）
/// + 两名角色（沈砚带别名、李四未出场）+ 两条世界观 + 一待回收一已回收伏笔
/// + 词库术语 + 默认 LLM 接口配置（`params_json` 决定装配预算）。
struct Fixture {
    pool: DbPool,
    app: tauri::App<tauri::test::MockRuntime>,
    novel_id: String,
    c0_id: String,
    c1_id: String,
    /// 当前章（正文含「沈砚与灵气」）。
    chapter_id: String,
    shen_id: String,
    li_id: String,
    wv1_id: String,
    wv2_id: String,
    plot1_id: String,
    plot2_id: String,
}

fn fixture(params_json: &str) -> Fixture {
    let count = FIXTURE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ew_ctx_{}_{}", std::process::id(), count));
    let _ = std::fs::remove_dir_all(&dir);
    let pool = db::init_db(dir).unwrap();

    let (novel_id, c0_id, c1_id, chapter_id, shen_id, li_id, wv1_id, wv2_id, plot1_id, plot2_id) = {
        let conn = pool.get().unwrap();

        let novel = db::novel::create_novel(&conn, "测试之书", "一部关于沈砚的长篇测试作品").unwrap();
        let volume = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
        let c0 = db::chapter::create_chapter(&conn, &novel.id, &volume.id, "序章", "序章摘要").unwrap();
        let c1 = db::chapter::create_chapter(&conn, &novel.id, &volume.id, "第一章", "第一章摘要").unwrap();
        let c2 = db::chapter::create_chapter(&conn, &novel.id, &volume.id, "第二章", "第二章摘要").unwrap();
        // 当前章正文：命中角色「沈砚」与词库术语「灵气」
        conn.execute(
            "UPDATE chapter SET content = ?1, content_plain = ?2, word_count = 5 WHERE id = ?3",
            rusqlite::params!["沈砚与灵气", "沈砚与灵气", c2.id],
        )
        .unwrap();

        let shen = db::settings::save_character(
            &conn,
            None,
            &novel.id,
            "沈砚",
            r#"["小沈","沈师兄"]"#,
            "主角档案：沉稳的剑修",
        )
        .unwrap();
        let li = db::settings::save_character(&conn, None, &novel.id, "李四", "[]", "路人档案").unwrap();
        let wv1 = db::settings::save_worldview(&conn, None, &novel.id, "世界观", "法术体系", "灵气是修炼的根本").unwrap();
        let wv2 = db::settings::save_worldview(&conn, None, &novel.id, "世界观", "门派分布", "三大门派鼎立").unwrap();
        let plot1 = db::settings::save_plot(
            &conn,
            None,
            &novel.id,
            "伏笔一",
            "沈砚的身世",
            "foreshadow",
            "planted",
            Some(c2.id.clone()),
        )
        .unwrap();
        let plot2 = db::settings::save_plot(&conn, None, &novel.id, "伏笔二", "已回收的往事", "foreshadow", "resolved", None).unwrap();

        db::lexicon::upsert_lexicon_term(&conn, &novel.id, "灵气", "术语", None).unwrap();

        // 默认接口配置：params 里的 context_window 决定装配预算
        db::llm_profile::create_profile(
            &conn,
            "默认配置",
            "openai",
            "http://127.0.0.1:1/v1",
            "",
            "test-model",
            params_json,
            true,
        )
        .unwrap();

        (
            novel.id,
            c0.id,
            c1.id,
            c2.id,
            shen.id,
            li.id,
            wv1.id,
            wv2.id,
            plot1.id,
            plot2.id,
        )
    };

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::ai_cmd::get_context_manifest,
            easy_writing_lib::commands::ai_cmd::set_entry_pinned,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(pool.clone());

    Fixture {
        pool,
        app,
        novel_id,
        c0_id,
        c1_id,
        chapter_id,
        shen_id,
        li_id,
        wv1_id,
        wv2_id,
        plot1_id,
        plot2_id,
    }
}

/// 读取某作品（可按章节）的装配清单。
fn manifest_of(
    app: &tauri::App<tauri::test::MockRuntime>,
    novel_id: &str,
    chapter_id: Option<&str>,
    prompt: Option<&str>,
) -> ContextManifest {
    let body = serde_json::json!({
        "novel_id": novel_id,
        "chapter_id": chapter_id,
        "prompt": prompt,
    });
    let v = call(app, "get_context_manifest", body).expect("get_context_manifest 不应失败");
    serde_json::from_value(v).expect("ContextManifest 应能反序列化")
}

/// 在清单里找条目；找不到时打印整份清单便于定位。
fn entry<'a>(m: &'a ContextManifest, id: &str) -> &'a shared_types::ContextManifestEntry {
    m.entries
        .iter()
        .find(|e| e.entry_id == id)
        .unwrap_or_else(|| panic!("manifest 应包含条目 {id}；实际条目：{:#?}", m.entries))
}

fn set_pinned(
    app: &tauri::App<tauri::test::MockRuntime>,
    entity_type: &str,
    entry_id: &str,
    pinned: bool,
) -> Result<(), String> {
    call(
        app,
        "set_entry_pinned",
        serde_json::json!({
            "entity_type": entity_type,
            "entry_id": entry_id,
            "pinned": pinned,
        }),
    )
    .map(|_| ())
}

#[test]
fn manifest_reports_budget_and_entry_reasons() {
    // params 为空对象：context_window 未设置 → 缺省 8k token
    let fx = fixture("{}");
    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(m.budget_tokens, 8000, "缺省预算应为 8k token");

    // 作品信息：最高优先级，reason=core
    let novel_entry = entry(&m, &fx.novel_id);
    assert_eq!(novel_entry.entity_type, "novel");
    assert_eq!(novel_entry.reason, "core");
    assert!(novel_entry.included);
    assert!(novel_entry.token_count > 0);

    // 当前章：core
    let cur = entry(&m, &fx.chapter_id);
    assert_eq!(cur.entity_type, "chapter");
    assert_eq!(cur.reason, "core");

    // 当前章之前的两章摘要：recent_summary
    for ch_id in [&fx.c0_id, &fx.c1_id] {
        let e = entry(&m, ch_id);
        assert_eq!(e.entity_type, "chapter");
        assert_eq!(e.reason, "recent_summary", "章节 {ch_id} 应以摘要入选");
        assert!(e.included);
    }

    // 出场角色（本名命中正文）→ 完整档案；未出场角色 → 仅名单
    let shen = entry(&m, &fx.shen_id);
    assert_eq!(shen.entity_type, "character");
    assert_eq!(shen.reason, "presence");
    assert!(shen.included);
    let li = entry(&m, &fx.li_id);
    assert_eq!(li.entity_type, "character");
    assert_eq!(li.reason, "listed");

    // 待回收伏笔入选；已回收伏笔不进清单
    let plot1 = entry(&m, &fx.plot1_id);
    assert_eq!(plot1.entity_type, "plot");
    assert_eq!(plot1.reason, "active_plot");
    assert!(plot1.included);
    assert!(
        m.entries.iter().all(|e| e.entry_id != fx.plot2_id),
        "已回收伏笔不应出现在清单里"
    );

    // 词库术语命中正文 → lexicon
    let lex = m
        .entries
        .iter()
        .find(|e| e.entity_type == "lexicon")
        .expect("正文命中的词库术语应产生 lexicon 条目");
    assert_eq!(lex.reason, "lexicon");
    assert_eq!(lex.title, "灵气");
    assert!(lex.included);

    // 世界观：优先级最低，但默认预算下仍入选
    let wv1 = entry(&m, &fx.wv1_id);
    assert_eq!(wv1.entity_type, "worldview");
    assert_eq!(wv1.reason, "worldview");
    assert!(wv1.included);

    // 估算用量必须落在预算内；默认预算下没有裁剪
    assert!(m.used_tokens > 0);
    assert!(
        m.used_tokens <= m.budget_tokens,
        "估算用量 {} 不应超过预算 {}",
        m.used_tokens,
        m.budget_tokens
    );
    assert!(m.trimmed.is_empty(), "默认预算下不应有裁剪：{:#?}", m.trimmed);
}

#[test]
fn manifest_budget_comes_from_profile_context_window() {
    // context_window = 300 → token 预算 300 / 字符预算 450。
    // 把一条世界观内容拉长到单条上限，确保低优先级段被裁掉。
    let fx = fixture(r#"{"context_window":300}"#);
    {
        let conn = fx.pool.get().unwrap();
        let long = "鼎立".repeat(300);
        db::settings::save_worldview(
            &conn,
            Some(fx.wv2_id.clone()),
            &fx.novel_id,
            "世界观",
            "门派分布",
            &long,
        )
        .unwrap();
    }

    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(m.budget_tokens, 300, "预算必须来自 profile.params.context_window");

    // 高优先级段（作品、当前章、出场角色）仍然入选
    assert!(entry(&m, &fx.novel_id).included);
    assert!(entry(&m, &fx.chapter_id).included);
    assert!(entry(&m, &fx.shen_id).included);
    // 世界观优先级最低：超长的那条被裁掉并记入 trimmed
    let wv2 = entry(&m, &fx.wv2_id);
    assert!(!wv2.included);
    assert!(
        m.trimmed.iter().any(|e| e.entry_id == fx.wv2_id),
        "被裁掉的条目必须记入 trimmed"
    );
}

#[test]
fn pinned_character_survives_budget_trim() {
    // context_window = 200 → 字符预算 300：世界观段必然被裁，
    // 但 pinned 角色在角色段里排在最前，仍然入选。
    let fx = fixture(r#"{"context_window":200}"#);
    set_pinned(&fx.app, "character", &fx.shen_id, true).expect("set_entry_pinned");

    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(m.budget_tokens, 200);

    let shen = entry(&m, &fx.shen_id);
    assert_eq!(shen.reason, "pinned");
    assert!(shen.included, "pinned 角色在紧缩预算下仍应入选");

    // 世界观优先级最低，本预算下全部被裁
    for e in &m.entries {
        if e.entity_type == "worldview" {
            assert!(!e.included, "世界观在 300 字符预算下应被裁掉：{e:?}");
        }
    }
    assert!(
        m.trimmed.iter().any(|e| e.entity_type == "worldview"),
        "被裁掉的世界观条目应记入 trimmed"
    );
}

#[test]
fn set_entry_pinned_toggles_persistence_and_manifest_reason() {
    let fx = fixture("{}");
    let conn = fx.pool.get().unwrap();

    // 角色：开 → 关
    set_pinned(&fx.app, "character", &fx.li_id, true).expect("pin character");
    let li = db::settings::list_characters(&conn, &fx.novel_id)
        .unwrap()
        .into_iter()
        .find(|c| c.id == fx.li_id)
        .unwrap();
    assert!(li.pinned, "pinned 必须持久化到库");
    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(entry(&m, &fx.li_id).reason, "pinned");

    set_pinned(&fx.app, "character", &fx.li_id, false).expect("unpin character");
    let li = db::settings::list_characters(&conn, &fx.novel_id)
        .unwrap()
        .into_iter()
        .find(|c| c.id == fx.li_id)
        .unwrap();
    assert!(!li.pinned, "取消 pinned 必须持久化");
    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(entry(&m, &fx.li_id).reason, "listed");

    // 世界观：pinned 后清单 reason 变为 pinned
    set_pinned(&fx.app, "worldview", &fx.wv1_id, true).expect("pin worldview");
    let wv = db::settings::list_worldview(&conn, &fx.novel_id)
        .unwrap()
        .into_iter()
        .find(|w| w.id == fx.wv1_id)
        .unwrap();
    assert!(wv.pinned);
    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(entry(&m, &fx.wv1_id).reason, "pinned");

    // 伏笔同样支持
    set_pinned(&fx.app, "plot", &fx.plot1_id, true).expect("pin plot");
    let m = manifest_of(&fx.app, &fx.novel_id, Some(&fx.chapter_id), None);
    assert_eq!(entry(&m, &fx.plot1_id).reason, "pinned");

    // 未知实体类型：明确报错而不是静默成功
    let err = set_pinned(&fx.app, "bogus", "x", true).expect_err("未知实体类型必须报错");
    assert!(err.contains("不支持的实体类型"), "实际错误：{err}");
}

#[test]
fn manifest_without_llm_profile_errors() {
    // 没有接口配置时明确报错（与 ai_chat_stream 同款前置检查），
    // 而不是静默产出空清单。
    let count = FIXTURE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ew_ctx_{}_{}", std::process::id(), count));
    let _ = std::fs::remove_dir_all(&dir);
    let pool = db::init_db(dir).unwrap();
    let novel_id = {
        let conn = pool.get().unwrap();
        db::novel::create_novel(&conn, "无配置作品", "").unwrap().id
    };

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            easy_writing_lib::commands::ai_cmd::get_context_manifest
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(pool);

    let err = call(
        &app,
        "get_context_manifest",
        serde_json::json!({ "novel_id": novel_id, "chapter_id": null, "prompt": null }),
    )
    .expect_err("无接口配置应报错");
    assert!(err.contains("No LLM profile configured"), "实际错误：{err}");
}

#[test]
fn manifest_without_chapter_still_reports_novel() {
    // 不传章节（如作品级对话）：作品信息仍在，但不产生任何章节条目
    let fx = fixture("{}");
    let m = manifest_of(&fx.app, &fx.novel_id, None, None);
    assert_eq!(entry(&m, &fx.novel_id).reason, "core");
    assert!(m.entries.iter().all(|e| e.entity_type != "chapter"));
    assert!(m.entries.iter().all(|e| e.reason != "recent_summary"));
}

fn call(
    app: &tauri::App<tauri::test::MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let label = format!("ctx{}", N.fetch_add(1, Ordering::SeqCst));
    let webview = tauri::WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .unwrap();
    let url: tauri::Url = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    }
    .parse()
    .unwrap();
    let res = tauri::test::get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url,
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
