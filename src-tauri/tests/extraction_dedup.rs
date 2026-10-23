//! T07 提炼去重与成本控制的集成测试。
//!
//! 钉住四件事：
//!   1. 「林舟 / 小舟」这类同人写法能被识别（别名精确命中 + 短名共享字符）；
//!   2. `possible_duplicate_of` 能落库并在待审列表里读回；
//!   3. 合并操作把新名称写入 `aliases`、并入角色卡，且候选被标记为已采纳；
//!   4. 设置项 `extraction_mode` / `extraction_chunk_threshold` 的默认值与解析。
//!
//! 纯函数（分块、single_call 解析、相似度）的单元用例在
//! `src/agent/extraction.rs` 的 `#[cfg(test)]` 里。

use easy_writing_lib::agent::extraction::{
    build_existing_index, detect_possible_duplicate, read_extraction_settings, ExtractionMode,
    SETTING_EXTRACTION_CHUNK_THRESHOLD, SETTING_EXTRACTION_MODE,
};
use easy_writing_lib::db;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_dedup_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    pool: db::DbPool,
    novel_id: String,
    chapter_id: String,
    lin_id: String,
}

fn fixture(tag: &str) -> Fixture {
    let dir = temp_dir(tag);
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "去重之书", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
    let chapter = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "第一章", "").unwrap();

    let profile = shared_types::ExplodedProfile::default().to_json_string();
    let lin = db::settings::save_character(&conn, None, &novel.id, "林舟", "[]", &profile).unwrap();

    Fixture {
        pool,
        novel_id: novel.id,
        chapter_id: chapter.id,
        lin_id: lin.id,
    }
}

fn insert_character_candidate(
    f: &Fixture,
    title: &str,
    payload: &str,
    existing: &easy_writing_lib::agent::extraction::ExistingIndex,
    pool_snapshot: &[(String, String)],
) -> shared_types::ExtractionCandidate {
    let conn = f.pool.get().unwrap();
    let dup = detect_possible_duplicate(title, "character", existing, pool_snapshot);
    db::extraction::insert_candidate(
        &conn,
        &f.novel_id,
        &f.chapter_id,
        "agent-character",
        "character",
        "create",
        None,
        title,
        payload,
        "原文证据",
        &format!("key-{title}"),
        dup.as_deref(),
    )
    .unwrap()
}

#[test]
fn similar_short_name_is_flagged_against_existing_character() {
    // 验收用例：已有角色「林舟」，候选里出现「小舟」→ 疑似同一人物。
    let f = fixture("fuzzy");
    let conn = f.pool.get().unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let cand = insert_character_candidate(&f, "小舟", "林舟的小名。", &existing, &[]);
    assert_eq!(
        cand.possible_duplicate_of.as_deref(),
        Some(f.lin_id.as_str()),
        "小舟应疑似为林舟的同一人物"
    );
}

#[test]
fn alias_hit_is_flagged_even_when_model_claims_create() {
    // 作者已经把「小舟」记为林舟的别名：模型仍声称 create 时，
    // 候选必须带上锚点，面板才能提示「疑似同一人物」。
    let f = fixture("alias");
    let conn = f.pool.get().unwrap();
    let profile = shared_types::ExplodedProfile::default().to_json_string();
    db::settings::save_character(
        &conn,
        Some(f.lin_id.clone()),
        &f.novel_id,
        "林舟",
        r#"["小舟"]"#,
        &profile,
    )
    .unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let cand = insert_character_candidate(&f, "小舟", "小舟在码头等人。", &existing, &[]);
    assert_eq!(cand.possible_duplicate_of.as_deref(), Some(f.lin_id.as_str()));
}

#[test]
fn different_name_is_not_flagged() {
    let f = fixture("different");
    let conn = f.pool.get().unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let cand = insert_character_candidate(&f, "沈砚", "另一位主角。", &existing, &[]);
    assert_eq!(cand.possible_duplicate_of, None, "名字无关时不应干扰作者");
}

#[test]
fn pool_match_points_at_the_other_candidate() {
    // 同一批里先出现「林舟」，后出现「小舟」——即使设定库还没有这个人，
    // 批内也应互相提示，避免一次提炼产出两个重复角色。
    let f = fixture("pool");
    let conn = f.pool.get().unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let first = insert_character_candidate(&f, "周砚", "新登场。", &existing, &[]);
    assert_eq!(first.possible_duplicate_of, None);

    let pool_now = vec![(first.id.clone(), first.title.clone())];
    let second = insert_character_candidate(&f, "周砚之", "又叫周砚之。", &existing, &pool_now);
    assert_eq!(
        second.possible_duplicate_of.as_deref(),
        Some(first.id.as_str()),
        "批内疑似重复应指向另一条候选"
    );
}

#[test]
fn duplicate_anchor_roundtrips_through_pending_list() {
    let f = fixture("roundtrip");
    let conn = f.pool.get().unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let cand = insert_character_candidate(&f, "小舟", "小名。", &existing, &[]);
    assert!(cand.possible_duplicate_of.is_some());

    let pending = db::extraction::list_pending_candidates(&conn, &f.novel_id).unwrap();
    let listed = pending.iter().find(|c| c.id == cand.id).expect("应能读回");
    assert_eq!(listed.possible_duplicate_of, cand.possible_duplicate_of);
}

#[test]
fn merge_appends_alias_into_profile_and_marks_candidate_accepted() {
    let f = fixture("merge");
    let conn = f.pool.get().unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let cand = insert_character_candidate(&f, "小舟", "小舟在码头等人。", &existing, &[]);
    assert_eq!(cand.possible_duplicate_of.as_deref(), Some(f.lin_id.as_str()));

    let merged = db::extraction::merge_character_candidate(&conn, &cand.id, &f.lin_id).unwrap();

    // 1) 名字进了 aliases
    let aliases: Vec<String> = serde_json::from_str(&merged.aliases).unwrap();
    assert!(aliases.contains(&"小舟".to_string()), "别名应包含小舟：{aliases:?}");
    // 2) 描述并入角色卡
    let ep = shared_types::ExplodedProfile::parse_from_str(&merged.profile);
    assert!(ep.raw_notes.contains("小舟在码头等人。"), "角色卡应吸收候选描述");
    // 3) 候选已采纳，不再出现在待审列表
    let pending = db::extraction::list_pending_candidates(&conn, &f.novel_id).unwrap();
    assert!(!pending.iter().any(|c| c.id == cand.id), "合并后不应仍是待审");
    // 4) 没有新建重复角色
    let chars = db::settings::list_characters(&conn, &f.novel_id).unwrap();
    assert_eq!(chars.len(), 1, "合并不得凭空多出一个角色：{chars:?}");
}

#[test]
fn merge_is_idempotent_for_aliases() {
    let f = fixture("merge-twice");
    let conn = f.pool.get().unwrap();
    let existing = build_existing_index(&conn, &f.novel_id).unwrap();

    let cand = insert_character_candidate(&f, "小舟", "小名。", &existing, &[]);
    db::extraction::merge_character_candidate(&conn, &cand.id, &f.lin_id).unwrap();

    // 同名候选再来一条（模型重复输出），别名不应堆积重复项
    let cand2 = insert_character_candidate(&f, "小舟", "还是小名。", &existing, &[]);
    let merged = db::extraction::merge_character_candidate(&conn, &cand2.id, &f.lin_id).unwrap();
    let aliases: Vec<String> = serde_json::from_str(&merged.aliases).unwrap();
    assert_eq!(aliases.iter().filter(|a| *a == "小舟").count(), 1);
}

#[test]
fn merge_refuses_non_character_candidates() {
    let f = fixture("merge-kind");
    let conn = f.pool.get().unwrap();

    let cand = db::extraction::insert_candidate(
        &conn,
        &f.novel_id,
        &f.chapter_id,
        "agent-worldview",
        "worldview",
        "create",
        None,
        "东京",
        "都城",
        "",
        "key-worldview",
        None,
    )
    .unwrap();

    let err = db::extraction::merge_character_candidate(&conn, &cand.id, &f.lin_id)
        .expect_err("世界观候选不能合并到角色");
    assert!(err.contains("角色"), "错误应说明原因：{err}");
}

#[test]
fn settings_default_to_parallel4_with_12k_threshold() {
    let f = fixture("settings-default");
    let conn = f.pool.get().unwrap();

    let s = read_extraction_settings(&conn);
    assert_eq!(s.mode, ExtractionMode::Parallel4);
    assert_eq!(s.chunk_threshold_chars, 12_000);
}

#[test]
fn settings_read_single_call_and_custom_threshold() {
    let f = fixture("settings-custom");
    let conn = f.pool.get().unwrap();

    db::app_setting::set_setting(&conn, SETTING_EXTRACTION_MODE, "single_call").unwrap();
    db::app_setting::set_setting(&conn, SETTING_EXTRACTION_CHUNK_THRESHOLD, "8000").unwrap();

    let s = read_extraction_settings(&conn);
    assert_eq!(s.mode, ExtractionMode::SingleCall);
    assert_eq!(s.chunk_threshold_chars, 8_000);

    // 非法阈值（过小）退回默认，而不是把章节切成一堆碎片
    db::app_setting::set_setting(&conn, SETTING_EXTRACTION_CHUNK_THRESHOLD, "10").unwrap();
    assert_eq!(
        read_extraction_settings(&conn).chunk_threshold_chars,
        12_000
    );
}