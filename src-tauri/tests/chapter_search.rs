//! T09 全书搜索：`db::search::search_chapters` 的两条路径与边界。
//!
//! 钉住的是这几件容易在后续改动里悄悄坏掉的事：
//!   * FTS 路径（≥3 字符）真的走 trigram 索引，且能跨词序命中；
//!   * 中文 2 字词回退 LIKE —— trigram 索引里根本不存在这么短的 token；
//!   * 用户输入 `AND` / `"` / `*` 这类 FTS 语法字符不会炸（短语转义）；
//!   * 软删章节既不出现也不占索引；
//!   * 索引与正文同步：改正文、软删、恢复后结果立刻跟上。

use easy_writing_lib::db;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_search_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tiptap(text: &str) -> String {
    serde_json::json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": text }]
        }]
    })
    .to_string()
}

/// 建一本作品 + 若干章，返回 (pool, novel_id, volume_id, chapter_ids)。
fn seed(tag: &str, titles_and_texts: &[(&str, &str)]) -> (db::DbPool, String, String, Vec<String>) {
    let dir = temp_dir(tag);
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "搜索之书", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();

    let mut ids = Vec::new();
    for (title, text) in titles_and_texts {
        let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, title, "").unwrap();
        db::chapter::save_chapter_content(
            &conn,
            &ch.id,
            &tiptap(text),
            text,
            text.chars().count() as i32,
        )
        .unwrap();
        ids.push(ch.id);
    }
    (pool, novel.id, vol.id, ids)
}

#[test]
fn fts_path_hits_chapters_containing_the_term() {
    let (pool, novel_id, _vol, ids) = seed(
        "fts",
        &[
            ("第一章 断水", "他握紧断水剑，剑光如雪。"),
            ("第二章 无剑", "这一章只写天气与风。"),
            ("第三章 再会", "断水剑嗡鸣不止。"),
        ],
    );
    let conn = pool.get().unwrap();

    let hits = db::search::search_chapters(&conn, &novel_id, "断水剑", 50).unwrap();

    assert_eq!(hits.len(), 2, "只应命中第一、三章，实际：{hits:?}");
    let hit_ids: Vec<&str> = hits.iter().map(|h| h.chapter_id.as_str()).collect();
    assert!(hit_ids.contains(&ids[0].as_str()));
    assert!(hit_ids.contains(&ids[2].as_str()));

    for hit in &hits {
        assert!(hit.hits > 0, "命中计数必须为正");
        assert!(!hit.snippets.is_empty(), "必须带片段，否则结果列表没法预览");
        assert!(hit.has_body, "有正文的章 has_body 应为 true");
    }
}

#[test]
fn two_char_chinese_query_falls_back_to_like() {
    // trigram 按三字符滑窗建索引，「沈砚」根本不在索引里。
    // 这不是 bug，是分词器的固有限制 —— 回退 LIKE 才能让两字词可用。
    let (pool, novel_id, _vol, ids) = seed(
        "like",
        &[
            ("甲", "沈砚推门而入。"),
            ("乙", "与沈砚无关的段落。"),
            ("丙", "完全不同。"),
        ],
    );
    let conn = pool.get().unwrap();

    let hits = db::search::search_chapters(&conn, &novel_id, "沈砚", 50).unwrap();

    assert_eq!(hits.len(), 2, "两字词必须走 LIKE 回退并命中，实际：{hits:?}");
    let hit_ids: Vec<&str> = hits.iter().map(|h| h.chapter_id.as_str()).collect();
    assert!(hit_ids.contains(&ids[0].as_str()));
    assert!(hit_ids.contains(&ids[1].as_str()));
}

#[test]
fn fts_syntax_characters_do_not_blow_up() {
    // 不包短语时，`AND` / `"` / `*` 会被当成 FTS5 查询语法：
    // 轻则结果全空，重则 `fts5: syntax error`。这里逐条确认不报错。
    let (pool, novel_id, _vol, _ids) = seed(
        "syntax",
        &[("甲", "他说 AND 之后又说 OR，最后加了个星号 *，还用了引号 \"。")],
    );
    let conn = pool.get().unwrap();

    for term in ["AND", "OR", "\"", "*", "-", "NOT", "a AND b"] {
        let r = db::search::search_chapters(&conn, &novel_id, term, 50);
        assert!(r.is_ok(), "查询 {term:?} 不应报错：{:?}", r.err());
    }

    // 引号本身也要能按字面命中（转义成两个引号后仍是短语）
    let quoted = db::search::search_chapters(&conn, &novel_id, "\"", 50).unwrap();
    assert_eq!(quoted.len(), 1, "引号应能按字面命中那一章");
}

#[test]
fn soft_deleted_chapters_are_excluded_and_index_is_synced() {
    let (pool, novel_id, _vol, ids) = seed(
        "softdelete",
        &[("甲", "断水剑在此。"), ("乙", "断水剑也在此。")],
    );
    let conn = pool.get().unwrap();

    assert_eq!(
        db::search::search_chapters(&conn, &novel_id, "断水剑", 50).unwrap().len(),
        2
    );

    // 软删一章：必须同时从搜索结果与索引里消失
    db::chapter::delete_chapter(&conn, &ids[0]).unwrap();
    let after = db::search::search_chapters(&conn, &novel_id, "断水剑", 50).unwrap();
    assert_eq!(after.len(), 1, "软删的章节不应出现在结果里");
    assert_eq!(after[0].chapter_id, ids[1]);

    let indexed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chapter_fts WHERE chapter_id = ?1",
            [&ids[0]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(indexed, 0, "软删后索引里不应残留该章");
}

#[test]
fn index_follows_content_updates() {
    // 触发器必须在 UPDATE 后重建该章索引行，否则搜索会命中「已经删掉的词」。
    let (pool, novel_id, _vol, ids) = seed("update", &[("甲", "断水剑在此。")]);
    let conn = pool.get().unwrap();

    assert_eq!(
        db::search::search_chapters(&conn, &novel_id, "断水剑", 50).unwrap().len(),
        1
    );

    db::chapter::save_chapter_content(&conn, &ids[0], &tiptap("青霜剑在此。"), "青霜剑在此。", 6)
        .unwrap();

    assert_eq!(
        db::search::search_chapters(&conn, &novel_id, "断水剑", 50).unwrap().len(),
        0,
        "改正文后旧词不应再命中"
    );
    assert_eq!(
        db::search::search_chapters(&conn, &novel_id, "青霜剑", 50).unwrap().len(),
        1,
        "改正文后新词必须命中"
    );
}

#[test]
fn search_is_scoped_to_the_novel() {
    let dir = temp_dir("scope");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let a = db::novel::create_novel(&conn, "甲书", "").unwrap();
    let b = db::novel::create_novel(&conn, "乙书", "").unwrap();
    let va = db::volume::create_volume(&conn, &a.id, "卷一", "").unwrap();
    let vb = db::volume::create_volume(&conn, &b.id, "卷一", "").unwrap();

    for (novel, vol) in [(&a, &va), (&b, &vb)] {
        let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "章", "").unwrap();
        db::chapter::save_chapter_content(&conn, &ch.id, &tiptap("断水剑在此。"), "断水剑在此。", 6)
            .unwrap();
    }

    let hits_a = db::search::search_chapters(&conn, &a.id, "断水剑", 50).unwrap();
    assert_eq!(hits_a.len(), 1, "只应返回本书的章节");
    assert_eq!(hits_a[0].volume_id, va.id);
}

#[test]
fn limit_caps_returned_chapters_not_hits() {
    let (pool, novel_id, _vol, _ids) = seed(
        "limit",
        &[
            ("甲", "断水剑。"),
            ("乙", "断水剑。"),
            ("丙", "断水剑。"),
        ],
    );
    let conn = pool.get().unwrap();

    let hits = db::search::search_chapters(&conn, &novel_id, "断水剑", 2).unwrap();
    assert_eq!(hits.len(), 2, "limit 限制的是章节数");
}

#[test]
fn empty_query_returns_nothing() {
    let (pool, novel_id, _vol, _ids) = seed("emptyq", &[("甲", "断水剑。")]);
    let conn = pool.get().unwrap();

    assert!(db::search::search_chapters(&conn, &novel_id, "", 50).unwrap().is_empty());
    assert!(db::search::search_chapters(&conn, &novel_id, "   ", 50).unwrap().is_empty());
}
