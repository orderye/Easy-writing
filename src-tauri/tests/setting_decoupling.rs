//! MIT 公理设计下三角解耦改造专项集成测试：
//!   1. `outline` 冲突更新写入 `novel.synopsis` 且 `setting_change.target_type = 'outline'` 可正常撤回；
//!   2. `resolve_plot` 撤回恢复 `status` 与清空 `resolved_chapter_id`，不破坏 `plot_item.content`；
//!   3. `update_chapter_summary_with_reason` 保留旧 `summary` 作为 `before_content` 且可撤回；
//!   4. `merge_character_candidate` 留档 `reason` 为 `"AI提炼疑似同人合并"`；
//!   5. `db::setting_view` 按 `pinned` 优先排序且支持别名命中。

mod common;

use easy_writing_lib::db;
use shared_types::ExplodedProfile;

fn setup(tag: &str) -> (db::DbPool, String, String, String) {
    let dir = common::temp_dir(tag);
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "解耦测试作品", "初始全篇梗概").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
    let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "第一章", "初始章纲").unwrap();
    (pool, novel.id, vol.id, ch.id)
}

#[test]
fn conflict_outline_update_uses_synopsis_and_reverts_cleanly() {
    let (pool, novel_id, _, _) = setup("outline_synopsis");
    let conn = pool.get().unwrap();

    db::setting_tx::apply_conflict_setting_update(
        &conn,
        &novel_id,
        None,
        "outline",
        "全篇大纲",
        "修改后的全篇梗概",
        "冲突裁决修改大纲",
    )
    .unwrap();

    let novel = db::novel::get_novel(&conn, &novel_id).unwrap().unwrap();
    assert_eq!(novel.synopsis, "修改后的全篇梗概");

    let changes = db::settings::list_setting_changes(&conn, &novel_id).unwrap();
    let change = changes
        .iter()
        .find(|c| c.target_type == "outline" && c.target_id == novel_id)
        .expect("应产生 target_type='outline' 的留档记录");
    assert_eq!(change.before_content, "初始全篇梗概");
    assert_eq!(change.after_content, "修改后的全篇梗概");

    db::setting_tx::revert_setting_change(&conn, &change.id).unwrap();

    let reverted_novel = db::novel::get_novel(&conn, &novel_id).unwrap().unwrap();
    assert_eq!(reverted_novel.synopsis, "初始全篇梗概");
}

#[test]
fn resolve_plot_revert_restores_status_without_corrupting_content() {
    let (pool, novel_id, _, ch_id) = setup("resolve_plot_revert");
    let conn = pool.get().unwrap();

    let plot = db::settings::save_plot(
        &conn,
        None,
        &novel_id,
        "青铜钥匙",
        "主角在密室拾得青铜钥匙",
        "foreshadow",
        "planted",
        Some(ch_id.clone()),
    )
    .unwrap();

    let resolved = db::foreshadow::resolve_plot(&conn, &plot.id, Some(ch_id)).unwrap();
    assert_eq!(resolved.status, "resolved");
    assert!(resolved.resolved_chapter_id.is_some());

    let changes = db::settings::list_setting_changes(&conn, &novel_id).unwrap();
    let change = changes
        .iter()
        .find(|c| c.target_type == "plot" && c.target_id == plot.id)
        .expect("resolve_plot 应产生留档记录");

    db::setting_tx::revert_setting_change(&conn, &change.id).unwrap();

    let plots = db::settings::list_plots(&conn, &novel_id).unwrap();
    let restored = plots.iter().find(|p| p.id == plot.id).unwrap();
    assert_eq!(restored.status, "planted");
    assert_eq!(restored.resolved_chapter_id, None);
    assert_eq!(restored.content, "主角在密室拾得青铜钥匙");
}

#[test]
fn chapter_summary_update_records_previous_summary_and_reverts() {
    let (pool, novel_id, _, ch_id) = setup("chapter_summary");
    let conn = pool.get().unwrap();

    db::setting_tx::update_chapter_summary_with_reason(
        &conn,
        &novel_id,
        &ch_id,
        "第一章",
        "AI提炼的新章纲",
        "AI提炼章纲梗概",
    )
    .unwrap();

    let ch = db::chapter::get_chapter(&conn, &ch_id).unwrap().unwrap();
    assert_eq!(ch.summary, "AI提炼的新章纲");

    let changes = db::settings::list_setting_changes(&conn, &novel_id).unwrap();
    let change = changes
        .iter()
        .find(|c| c.target_type == "chapter" && c.target_id == ch_id)
        .expect("应产生章节摘要留档");
    assert_eq!(change.before_content, "初始章纲");
    assert_eq!(change.after_content, "AI提炼的新章纲");

    db::setting_tx::revert_setting_change(&conn, &change.id).unwrap();
    let reverted_ch = db::chapter::get_chapter(&conn, &ch_id).unwrap().unwrap();
    assert_eq!(reverted_ch.summary, "初始章纲");
}

#[test]
fn merge_character_candidate_records_accurate_reason() {
    let (pool, novel_id, _, ch_id) = setup("merge_reason");
    let conn = pool.get().unwrap();

    let ep = ExplodedProfile {
        raw_notes: "青云宗弟子".to_string(),
        ..Default::default()
    };
    let ch = db::settings::save_character(
        &conn,
        None,
        &novel_id,
        "林舟",
        "[]",
        &ep.to_json_string(),
    )
    .unwrap();

    let cand = db::extraction::insert_candidate(
        &conn,
        &novel_id,
        &ch_id,
        "agent-character",
        "character",
        "create",
        None,
        "小舟",
        "深夜在后山练剑",
        "小舟独自挥剑",
        "char:小舟",
        Some(&ch.id),
    )
    .unwrap();

    let merged = db::extraction::merge_character_candidate(&conn, &cand.id, &ch.id).unwrap();
    assert!(merged.aliases.contains("小舟"));

    let changes = db::settings::list_setting_changes(&conn, &novel_id).unwrap();
    let merge_change = changes
        .iter()
        .find(|c| c.target_type == "character" && c.target_id == ch.id)
        .expect("合并角色应留下变更记录");
    assert_eq!(merge_change.reason, "AI提炼疑似同人合并");
}

#[test]
fn setting_view_orders_pinned_first_and_matches_aliases() {
    let (pool, novel_id, _, _) = setup("setting_view");
    let conn = pool.get().unwrap();

    let normal_wv =
        db::settings::save_worldview(&conn, None, &novel_id, "地理", "普通设定", "普通内容").unwrap();
    let pinned_wv =
        db::settings::save_worldview(&conn, None, &novel_id, "法则", "核心设定", "硬性内容").unwrap();

    conn.execute(
        "UPDATE worldview_entry SET pinned = 1 WHERE id = ?1",
        rusqlite::params![pinned_wv.id],
    )
    .unwrap();

    let _c1 = db::settings::save_character(
        &conn,
        None,
        &novel_id,
        "沈砚",
        r#"["小沈","沈师兄"]"#,
        "{}",
    )
    .unwrap();

    let view = db::setting_view::load_setting_index(&conn, &novel_id).unwrap();
    assert_eq!(view.worldviews.len(), 2);
    assert_eq!(view.worldviews[0].id, pinned_wv.id);
    assert_eq!(view.worldviews[1].id, normal_wv.id);

    assert_eq!(
        db::setting_view::match_character_presence("沈砚", r#"["小沈"]"#, "小沈推门而入"),
        db::setting_view::PresenceMatch::Alias
    );
    assert_eq!(
        db::setting_view::match_character_presence("沈砚", r#"["小沈"]"#, "沈砚推门而入"),
        db::setting_view::PresenceMatch::Name
    );
    assert_eq!(
        db::setting_view::match_character_presence("沈砚", r#"["小沈"]"#, "路人甲路过"),
        db::setting_view::PresenceMatch::None
    );
}

#[test]
fn manual_outline_updates_record_and_revert_via_setting_tx() {
    let (pool, novel_id, vol_id, ch_id) = setup("manual_outline");
    let conn = pool.get().unwrap();

    // 1. 仅改标题不改大纲内容时，不产生 setting_change 留档
    db::novel::update_novel(&conn, &novel_id, "新书名", "初始全篇梗概").unwrap();
    db::volume::update_volume(&conn, &vol_id, "新卷名", "").unwrap();
    db::chapter::update_chapter_meta(&conn, &ch_id, "新章名", "初始章纲", "editing").unwrap();
    assert!(db::settings::list_setting_changes(&conn, &novel_id)
        .unwrap()
        .is_empty());

    // 2. 修改三层大纲（全篇梗概 / 卷纲 / 章纲）均应留档且可撤回
    db::novel::update_novel(&conn, &novel_id, "新书名", "更新后的全篇梗概").unwrap();
    db::volume::update_volume(&conn, &vol_id, "新卷名", "更新后的卷纲").unwrap();
    db::chapter::update_chapter_meta(&conn, &ch_id, "新章名", "更新后的章纲", "editing").unwrap();

    let changes = db::settings::list_setting_changes(&conn, &novel_id).unwrap();
    let novel_change = changes
        .iter()
        .find(|c| c.target_type == "outline" && c.target_id == novel_id)
        .expect("应留档全篇梗概修改");
    let vol_change = changes
        .iter()
        .find(|c| c.target_type == "volume" && c.target_id == vol_id)
        .expect("应留档卷纲修改");
    let ch_change = changes
        .iter()
        .find(|c| c.target_type == "chapter" && c.target_id == ch_id)
        .expect("应留档章纲修改");

    db::setting_tx::revert_setting_change(&conn, &novel_change.id).unwrap();
    db::setting_tx::revert_setting_change(&conn, &vol_change.id).unwrap();
    db::setting_tx::revert_setting_change(&conn, &ch_change.id).unwrap();

    assert_eq!(
        db::novel::get_novel(&conn, &novel_id).unwrap().unwrap().synopsis,
        "初始全篇梗概"
    );
    assert_eq!(
        db::volume::get_volume(&conn, &vol_id).unwrap().unwrap().summary,
        ""
    );
    assert_eq!(
        db::chapter::get_chapter(&conn, &ch_id).unwrap().unwrap().summary,
        "初始章纲"
    );
}

#[test]
fn extraction_existing_index_uses_setting_view() {
    let (pool, novel_id, _, ch_id) = setup("extraction_index");
    let conn = pool.get().unwrap();

    let normal = db::settings::save_character(&conn, None, &novel_id, "路人甲", "[]", "{}").unwrap();
    let pinned = db::settings::save_character(
        &conn,
        None,
        &novel_id,
        "沈砚",
        r#"["小沈"]"#,
        "{}",
    )
    .unwrap();
    conn.execute(
        "UPDATE character SET pinned = 1 WHERE id = ?1",
        rusqlite::params![pinned.id],
    )
    .unwrap();
    db::settings::save_worldview(&conn, None, &novel_id, "法则", "灵气", "规则").unwrap();
    db::settings::save_plot(
        &conn,
        None,
        &novel_id,
        "伏笔A",
        "内容",
        "foreshadow",
        "planted",
        Some(ch_id),
    )
    .unwrap();

    let index = easy_writing_lib::agent::extraction::build_existing_index(&conn, &novel_id).unwrap();
    let char_entries = &index.iter().find(|(k, _)| k == "character").unwrap().1;
    assert_eq!(char_entries.len(), 2);
    assert_eq!(char_entries[0].0, pinned.id);
    assert_eq!(char_entries[1].0, normal.id);
    assert_eq!(index.iter().find(|(k, _)| k == "worldview").unwrap().1.len(), 1);
    assert_eq!(index.iter().find(|(k, _)| k == "plot").unwrap().1.len(), 1);
}
