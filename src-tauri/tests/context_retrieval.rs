//! 长文上下文检索的验收测试（P2-1）。
//!
//! 守的是那份方案里最硬的一条结论：
//! 旧实现是 take(15)/take(12)/take(10) 的硬编码截断，
//! 写到几百章之后，早期登场但排序靠后的角色档案根本进不了 prompt。
//!
//! 这个用例刻意把主角放在角色列表的**最后一位**（超出旧的 take(12) 窗口），
//! 并断言其档案文本出现在上下文里 —— 旧实现必然失败，新实现必然通过。
use easy_writing_lib::ai::context;
use easy_writing_lib::db;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_ctx_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 造一部「长篇」：300 章 + 20 个角色，主角排在最后。
///
/// 返回 (章节数, 主角 id)，主角档案里埋了一句只存在于 character 表的特征文本。
const PROTAGONIST_PROFILE: &str = "出身寒门，佩断水剑，左眉有一道旧疤";

fn build_long_novel(tag: &str) -> (db::DbPool, shared_types::Novel, String, String) {
    // tag 必须每个用例不同：cargo test 默认并发跑，共用一个库会让
    // init_db 的版本写入互相冲突（这个坑是被本文件真实踩出来的）。
    let dir = temp_dir(tag);
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "长篇测试", "一颗被逐出师门的棋子").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();

    // 早登场但排序靠后的主角：先建 19 个配角，再建主角，使其 sort_order 最大。
    for i in 0..19 {
        db::settings::save_character(
            &conn,
            None,
            &novel.id,
            &format!("配角{:02}", i),
            "[]",
            "{\"一句话\":\"路人甲\"}",
        )
        .unwrap();
    }
    let protagonist = db::settings::save_character(
        &conn,
        None,
        &novel.id,
        "沈砚",
        r#"["小沈","沈师兄"]"#,
        &format!("{{\"简介\":\"{}\"}}", PROTAGONIST_PROFILE),
    )
    .unwrap();

    // 300 章；只有第 1 章和第 300 章让主角出场
    let mut last_chapter_id = String::new();
    for i in 0..300 {
        let ch = db::chapter::create_chapter(
            &conn,
            &novel.id,
            &vol.id,
            &format!("第{}章", i + 1),
            "",
        )
        .unwrap();
        let text = if i == 0 {
            "沈砚第一次出场，小沈还很年轻。"
        } else if i == 299 {
            "多年之后，沈师兄重新踏进这座城。"
        } else {
            "本章无事发生。"
        };
        db::chapter::save_chapter_content(&conn, &ch.id, "{}", text, text.chars().count() as i32)
            .unwrap();
        last_chapter_id = ch.id;
    }

    (pool, novel, protagonist.id, last_chapter_id)
}

#[test]
fn early_protagonist_survives_into_chapter_300_context() {
    let (pool, novel, protagonist_id, last_chapter_id) = build_long_novel("survive");
    let conn = pool.get().unwrap();

    let assembled = context::assemble_writing_context(
        &conn,
        &novel,
        Some(&last_chapter_id),
        "你是写作助手",
    )
    .unwrap();

    // 1) 角色数超过旧的 take(12) 窗口，确保这个用例真的能区分新旧实现
    let all_chars = db::settings::list_characters(&conn, &novel.id).unwrap();
    assert!(
        all_chars.len() > context::LEGACY_CHARACTER_TAKE,
        "角色数必须多于旧截断窗口，否则这个用例无法区分新旧实现"
    );
    let pos = all_chars
        .iter()
        .position(|c| c.id == protagonist_id)
        .expect("主角必须在角色列表里");
    assert!(
        pos >= context::LEGACY_CHARACTER_TAKE,
        "主角必须排在旧 take 窗口之外，否则这个用例无法区分新旧实现（实际位置 {}）",
        pos
    );

    // 2) 关键断言：只存在于 character 表的档案文本必须进入上下文。
    //    第 300 章正文里只有"沈师兄"，没有这句档案 ——
    //    所以它能出现，只可能是因为按别名命中了角色并被取了完整档案。
    assert!(
        assembled.context_text.contains(PROTAGONIST_PROFILE),
        "第 300 章的上下文里没有早期角色的完整档案 —— 长文检索又退化了",
    );
    assert!(assembled.context_text.contains("沈砚"));
}

#[test]
fn context_respects_the_budget() {
    let (pool, novel, _pid, last_chapter_id) = build_long_novel("budget");
    let conn = pool.get().unwrap();

    let budget = 1_500usize;
    let assembled = context::assemble_writing_context_with_budget(
        &conn,
        &novel,
        Some(&last_chapter_id),
        "你是写作助手",
        budget,
    )
    .unwrap();

    assert!(
        assembled.context_text.chars().count() <= budget,
        "上下文 {} 字符超过了预算 {} —— 预算形同虚设",
        assembled.context_text.chars().count(),
        budget
    );
    // 预算再小也必须保留最高优先级的作品信息，不能被饿死
    assert!(assembled.context_text.contains("长篇测试"));
}

#[test]
fn characters_absent_from_the_chapter_are_downgraded_not_dropped() {
    let (pool, novel, _pid, _last) = build_long_novel("absent");
    let conn = pool.get().unwrap();

    // 不带 current_chapter_id：没有任何角色"出场"
    let assembled = context::assemble_writing_context(&conn, &novel, None, "").unwrap();

    // 未出场角色仍应留下名单线索，证明它们没有被整体丢弃
    assert!(assembled.context_text.contains("其他已登场角色"));
    // 但也确实不该把所有人的完整档案都塞进来
    assert!(!assembled.context_text.contains(PROTAGONIST_PROFILE));
}
