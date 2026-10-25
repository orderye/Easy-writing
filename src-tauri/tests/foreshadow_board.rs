//! T10 伏笔看板的集成测试。
//!
//! 钉住四件事：
//!   1. 「距今章数」按全书章序（跨卷正确）计算；
//!   2. `resolve_plot` 落库 + 留档，跨作品章节被拒；
//!   3. 超期未回收高亮、回收后不高亮；
//!   4. 关联角色按本名/别名匹配伏笔内容。
//!
//! 纯函数（`characters_mentioned_in`）的单元用例在 `src/db/foreshadow.rs` 里。

use easy_writing_lib::db;
use shared_types::ExplodedProfile;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_fores_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    pool: db::DbPool,
    novel_id: String,
    /// 全书章序的章节 id（按卷序→章序排好）
    chapter_ids: Vec<String>,
}

/// 造 3 卷各 5 章（全书 15 章）的 fixture。
fn fixture(tag: &str) -> Fixture {
    let dir = temp_dir(tag);
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "伏笔看板测试", "").unwrap();
    let mut chapter_ids = Vec::new();

    for vol_idx in 0..3 {
        let vol = db::volume::create_volume(
            &conn,
            &novel.id,
            &format!("第{}卷", vol_idx + 1),
            "",
        )
        .unwrap();
        for ch_idx in 0..5 {
            let ch = db::chapter::create_chapter(
                &conn,
                &novel.id,
                &vol.id,
                &format!("第{}章", ch_idx + 1),
                "",
            )
            .unwrap();
            chapter_ids.push(ch.id);
        }
    }

    // 15 章 → chapter_ids[0..15] 按全书章序
    Fixture {
        pool,
        novel_id: novel.id,
        chapter_ids,
    }
}

/// 辅助：新建一条伏笔，返回 id。`chapter_id` 传 `Some(...)` 或 `None`。
fn new_plot(f: &Fixture, title: &str, content: &str, chapter_id: Option<&str>) -> String {
    let conn = f.pool.get().unwrap();
    let p = db::settings::save_plot(
        &conn,
        None,
        &f.novel_id,
        title,
        content,
        "foreshadow",
        "planted",
        chapter_id.map(|s| s.to_string()),
    )
    .unwrap();
    p.id
}

#[test]
fn chapters_since_plant_is_cross_volume() {
    // 全书 15 章，伏笔 A 埋在第 3 章（chapter_ids[2]）→ 距今 15 - 3 = 12
    let f = fixture("cross_vol");
    let pid = new_plot(&f, "伏笔A", "沈砚的身世", Some(f.chapter_ids[2].as_str()));

    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();

    assert_eq!(row.chapter_index, Some(2));
    assert_eq!(row.chapters_since_plant, Some(12), "全书 15 章，第 3 章 → 12");
}

#[test]
fn plant_at_last_chapter_is_zero() {
    let f = fixture("last_chapter");
    let pid = new_plot(&f, "末章伏笔", "", Some(f.chapter_ids[14].as_str()));

    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();

    assert_eq!(row.chapters_since_plant, Some(0), "埋设在末章 → 0");
}

#[test]
fn resolved_is_never_overdue() {
    let f = fixture("resolved_not_overdue");
    // 阈值设为 0 让未回收必超期，然后标记回收看 overdue 是否变 false
    let conn = f.pool.get().unwrap();
    db::app_setting::set_setting(&conn, "foreshadow_overdue_chapters", "0").unwrap();
    drop(conn);

    let pid = new_plot(&f, "将回收伏笔", "沈砚的身世", Some(f.chapter_ids[0].as_str()));

    // 回收前：未回收 + 距今 14 > 0 → overdue
    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();
    assert!(row.overdue, "未回收且超阈值 → overdue");

    // 标记回收（回收章节用第 10 章）
    let resolved = db::foreshadow::resolve_plot(&conn, &pid, Some(f.chapter_ids[9].clone())).unwrap();
    assert_eq!(resolved.status, "resolved");
    assert_eq!(resolved.resolved_chapter_id.as_deref(), Some(f.chapter_ids[9].as_str()));

    // 回收后：overdue = false
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();
    assert!(!row.overdue, "已回收 → 不高亮");
}

#[test]
fn overdue_highlight_when_threshold_exceeded() {
    let f = fixture("overdue");
    // 阈值设为 3，埋在第 1 章（距今 14 > 3）→ overdue
    let conn = f.pool.get().unwrap();
    db::app_setting::set_setting(&conn, "foreshadow_overdue_chapters", "3").unwrap();
    drop(conn);

    let pid = new_plot(&f, "超期伏笔", "沈砚的身世", Some(f.chapter_ids[0].as_str()));

    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();
    assert!(row.overdue, "距今 14 > 阈值 3 → overdue");
}

#[test]
fn related_characters_match_by_name_and_alias() {
    let f = fixture("related_chars");
    let conn = f.pool.get().unwrap();
    let profile = ExplodedProfile::default().to_json_string();
    db::settings::save_character(&conn, None, &f.novel_id, "沈砚", "[]", &profile).unwrap();
    db::settings::save_character(&conn, None, &f.novel_id, "阿岚", "[\"岚子\"]", &profile).unwrap();
    drop(conn);

    // 伏笔内容提到沈砚（本名）与岚子（阿岚的别名）
    let pid = new_plot(&f, "关联伏笔", "沈砚与岚子登场", Some(f.chapter_ids[0].as_str()));

    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();
    assert!(row.related_characters.contains(&"沈砚".to_string()));
    assert!(row.related_characters.contains(&"阿岚".to_string()));
}

#[test]
fn related_characters_empty_when_no_match() {
    let f = fixture("no_match");
    let pid = new_plot(&f, "无关联伏笔", "无关内容", Some(f.chapter_ids[0].as_str()));

    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();
    assert!(row.related_characters.is_empty());
}

#[test]
fn missing_plant_chapter_is_none_not_overdue() {
    let f = fixture("missing_chapter");
    // 人工新建伏笔时 chapter_id = None（数据缺口）
    let pid = new_plot(&f, "无埋设章", "沈砚的身世", None);

    let conn = f.pool.get().unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let row = rows.iter().find(|r| r.id == pid).unwrap();

    assert_eq!(row.chapter_index, None);
    assert_eq!(row.chapters_since_plant, None, "埋设章缺失 → None，不编数");
    assert!(!row.overdue, "埋设章缺失 → 不高亮");
}

#[test]
fn resolve_plot_rejects_cross_novel_chapter() {
    let f = fixture("cross_novel");
    let pid = new_plot(&f, "跨作品回收测试", "", Some(f.chapter_ids[0].as_str()));

    // 造另一个作品的章节
    let other_chapter = {
        let conn = f.pool.get().unwrap();
        let n = db::novel::create_novel(&conn, "另一作品", "").unwrap();
        let v = db::volume::create_volume(&conn, &n.id, "卷", "").unwrap();
        let ch = db::chapter::create_chapter(&conn, &n.id, &v.id, "章", "").unwrap();
        ch.id
    };

    let conn = f.pool.get().unwrap();
    let err = db::foreshadow::resolve_plot(&conn, &pid, Some(other_chapter)).unwrap_err();
    assert!(
        err.to_string().contains("不属于该作品"),
        "跨作品章节必须被拒，实际：{}",
        err
    );
}

#[test]
fn resolve_plot_records_setting_change() {
    let f = fixture("record_change");
    let pid = new_plot(&f, "留档测试", "", Some(f.chapter_ids[0].as_str()));

    let conn = f.pool.get().unwrap();
    db::foreshadow::resolve_plot(&conn, &pid, Some(f.chapter_ids[5].clone())).unwrap();

    // 留档：target_type = "plot"
    let changes = db::settings::list_setting_changes(&conn, &f.novel_id).unwrap();
    let plot_change = changes
        .iter()
        .find(|c| c.target_id == pid && c.target_type == "plot")
        .expect("标记回收必须留档");
    assert!(plot_change.before_content.contains("planted"));
    assert!(plot_change.after_content.contains("resolved"));
}

#[test]
fn save_plot_does_not_clear_resolved_chapter() {
    let f = fixture("save_not_clear");
    let pid = new_plot(&f, "不破坏回收章", "", Some(f.chapter_ids[0].as_str()));

    let conn = f.pool.get().unwrap();
    db::foreshadow::resolve_plot(&conn, &pid, Some(f.chapter_ids[3].clone())).unwrap();

    // 编辑标题/内容（save_plot 走 ON CONFLICT DO UPDATE，不更新 resolved_chapter_id）
    let updated = db::settings::save_plot(
        &conn,
        Some(pid.clone()),
        &f.novel_id,
        "改标题",
        "改内容",
        "foreshadow",
        "resolved",
        Some(f.chapter_ids[0].clone()),
    )
    .unwrap();

    assert_eq!(
        updated.resolved_chapter_id.as_deref(),
        Some(f.chapter_ids[3].as_str()),
        "编辑标题/内容时不能把回收章节抹掉"
    );
}

#[test]
fn overdue_threshold_parsing() {
    let f = fixture("threshold");
    let conn = f.pool.get().unwrap();

    // 未设置 → 默认 30
    assert_eq!(db::foreshadow::overdue_threshold_chars(&conn), 30);

    // 非法值 → 回落 30
    db::app_setting::set_setting(&conn, "foreshadow_overdue_chapters", "abc").unwrap();
    assert_eq!(db::foreshadow::overdue_threshold_chars(&conn), 30);

    // 合法值
    db::app_setting::set_setting(&conn, "foreshadow_overdue_chapters", "7").unwrap();
    assert_eq!(db::foreshadow::overdue_threshold_chars(&conn), 7);
}

#[test]
fn board_sorts_unresolved_first_then_by_chapters_since_plant() {
    let f = fixture("sort");
    let conn = f.pool.get().unwrap();

    // A 埋第 1 章（距今 14），B 埋第 5 章（距今 10），C 埋第 10 章（距今 5）
    let a = new_plot(&f, "A", "", Some(f.chapter_ids[0].as_str()));
    let b = new_plot(&f, "B", "", Some(f.chapter_ids[4].as_str()));
    let c = new_plot(&f, "C", "", Some(f.chapter_ids[9].as_str()));

    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();

    // 未回收在前，按距今章数降序
    assert_eq!(titles, vec!["A", "B", "C"], "未回收按距今章数降序");

    // 把 A 标记回收 → A 排到最后
    db::foreshadow::resolve_plot(&conn, &a, Some(f.chapter_ids[14].clone())).unwrap();
    let rows = db::foreshadow::list_foreshadow_board(&conn, &f.novel_id).unwrap();
    let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(titles, vec!["B", "C", "A"], "已回收排到最后");
}
