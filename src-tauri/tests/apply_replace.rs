//! T09 批量替换的批次语义：快照、整批撤销、逐字还原。
//!
//! 替换是唯一会一次改动多章正文的操作，所以它的安全网只有两条：
//!   * 执行前为**每章**写快照，且同一批共享 `batch_id`；
//!   * 撤销按 `batch_id` 整批找回，还原后与替换前**逐字**相等。
//!
//! 这两条都在这里钉住。命令层（`search_cmd::apply_replace`）用 `State<DbPool>`，
//! 直接调需要 mock app；这里改测它依赖的同一组底层函数 —— 语义完全一致，
//! 而失败信息更直白。

use easy_writing_lib::db;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_apply_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tiptap(text: &str) -> String {
    serde_json::json!({
        "type": "doc",
        "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }]
    })
    .to_string()
}

fn setup(tag: &str, texts: &[&str]) -> (db::DbPool, String, Vec<String>) {
    let dir = temp_dir(tag);
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "替换之书", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();

    let mut ids = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, &format!("第{}章", i + 1), "")
            .unwrap();
        db::chapter::save_chapter_content(&conn, &ch.id, &tiptap(t), t, t.chars().count() as i32)
            .unwrap();
        ids.push(ch.id);
    }
    (pool, novel.id, ids)
}

/// 执行一次「批量替换」：逐章先快照（带 batch_id）再改写。
///
/// 与 `search_cmd::apply_replace` 的事务内步骤逐条对应；返回 (batch_id, 改动章数)。
fn apply_batch(
    conn: &rusqlite::Connection,
    novel_id: &str,
    chapter_ids: &[String],
    query: &str,
    replacement: &str,
) -> (String, u32) {
    let batch_id = uuid::Uuid::new_v4().to_string();
    let mut changed = 0u32;

    for chapter_id in chapter_ids {
        let ch = db::chapter::get_chapter(conn, chapter_id).unwrap().unwrap();
        let Some(outcome) =
            easy_writing_lib::replace::replace_in_chapter(&ch.content, query, replacement, Default::default())
        else {
            continue;
        };
        if outcome.replacements == 0 {
            continue;
        }

        db::chapter_revision::create_revision_with_batch(
            conn,
            &ch.id,
            novel_id,
            &ch.title,
            &ch.content,
            &ch.content_plain,
            ch.word_count,
            "manual",
            &format!("批量替换前（查找「{query}」）"),
            Some(&batch_id),
        )
        .unwrap();

        db::chapter::save_chapter_content(
            conn,
            &ch.id,
            &outcome.content,
            &outcome.content_plain,
            outcome.word_count,
        )
        .unwrap();
        changed += 1;
    }
    (batch_id, changed)
}

#[test]
fn batch_writes_one_snapshot_per_chapter_sharing_batch_id() {
    let (pool, novel_id, ids) = setup("batch", &["断水剑在此。", "断水剑也在此。", "无关章。"]);
    let conn = pool.get().unwrap();

    let (batch_id, changed) = apply_batch(&conn, &novel_id, &ids, "断水剑", "青霜剑");
    assert_eq!(changed, 2, "只有两章含该词");

    let snaps: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT chapter_id, note FROM chapter_revision
                 WHERE batch_id = ?1 AND revision_type = 'manual'",
            )
            .unwrap();
        stmt.query_map([&batch_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };

    assert_eq!(snaps.len(), 2, "每章一条快照");
    let snapshot_chapters: Vec<&str> = snaps.iter().map(|(c, _)| c.as_str()).collect();
    assert!(snapshot_chapters.contains(&ids[0].as_str()));
    assert!(snapshot_chapters.contains(&ids[1].as_str()));
    for (_, note) in &snaps {
        assert!(note.contains("断水剑"), "快照 note 应记下查找词：{note}");
    }
}

#[test]
fn undo_restores_every_chapter_character_for_character() {
    let (pool, novel_id, ids) = setup("undo", &["他握紧断水剑。", "断水剑嗡鸣不止。", "无关章。"]);
    let conn = pool.get().unwrap();

    // 记下替换前的正文，撤销后要逐字比对
    let before: Vec<String> = ids
        .iter()
        .map(|id| db::chapter::get_chapter(&conn, id).unwrap().unwrap().content)
        .collect();

    let (batch_id, changed) = apply_batch(&conn, &novel_id, &ids, "断水剑", "青霜剑");
    assert_eq!(changed, 2);

    // 确认真的改了
    let mid = db::chapter::get_chapter(&conn, &ids[0]).unwrap().unwrap();
    assert!(mid.content.contains("青霜剑"));
    assert!(!mid.content.contains("断水剑"));

    // 整批撤销：按 batch_id 找回每章快照并还原
    let rows: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT chapter_id, id FROM chapter_revision
                 WHERE batch_id = ?1 AND revision_type = 'manual'",
            )
            .unwrap();
        stmt.query_map([&batch_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(rows.len(), 2);
    for (chapter_id, revision_id) in rows {
        db::chapter_revision::revert_chapter_to_revision(&conn, &chapter_id, &revision_id).unwrap();
    }

    for (i, id) in ids.iter().enumerate() {
        let after = db::chapter::get_chapter(&conn, id).unwrap().unwrap().content;
        assert_eq!(after, before[i], "第 {i} 章撤销后必须与替换前逐字一致");
    }
}

#[test]
fn undo_only_touches_its_own_batch() {
    // 两批替换交错：撤销第一批不得动第二批改过的章节。
    let (pool, novel_id, ids) = setup("isolate", &["断水剑。", "断水剑。", "断水剑。"]);
    let conn = pool.get().unwrap();

    let (batch_a, _) = apply_batch(&conn, &novel_id, &ids[0..2], "断水剑", "甲剑");
    let after_a = db::chapter::get_chapter(&conn, &ids[2]).unwrap().unwrap().content.clone();
    let (batch_b, _) = apply_batch(&conn, &novel_id, &ids[2..3], "断水剑", "乙剑");
    assert_ne!(batch_a, batch_b, "两批必须有不同的 batch_id");

    let b_rows: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT chapter_id, id FROM chapter_revision WHERE batch_id = ?1")
            .unwrap();
        stmt.query_map([&batch_a], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    for (chapter_id, revision_id) in b_rows {
        db::chapter_revision::revert_chapter_to_revision(&conn, &chapter_id, &revision_id).unwrap();
    }

    // 第三批（batch_b 改的）必须保持「乙剑」
    let third = db::chapter::get_chapter(&conn, &ids[2]).unwrap().unwrap().content;
    assert!(third.contains("乙剑"), "撤销第一批不应影响第二批：{third}");
    assert!(!third.contains("甲剑"));
    assert_ne!(third, after_a, "第三批确实被第二批改过");
}

#[test]
fn snapshot_is_manual_so_auto_cleanup_will_not_drop_it() {
    // `maybe_auto_snapshot` 的清理只删 revision_type='auto'。
    // 替换快照必须是 manual，否则会在下一次自动快照时被清掉、撤销随之失效。
    let (pool, novel_id, ids) = setup("manual", &["断水剑。"]);
    let conn = pool.get().unwrap();

    let (batch_id, _) = apply_batch(&conn, &novel_id, &ids[..1], "断水剑", "青霜剑");

    let types: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT revision_type FROM chapter_revision WHERE batch_id = ?1")
            .unwrap();
        stmt.query_map([&batch_id], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(types, vec!["manual".to_string()]);
}

#[test]
fn chapter_without_matches_gets_no_snapshot() {
    // 不该给没改动的章留快照 —— 否则版本历史里会堆满「无事发生」。
    let (pool, novel_id, ids) = setup("nomatch", &["风平浪静。", "断水剑。"]);
    let conn = pool.get().unwrap();

    let (batch_id, changed) = apply_batch(&conn, &novel_id, &ids, "断水剑", "青霜剑");
    assert_eq!(changed, 1);

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chapter_revision WHERE batch_id = ?1",
            [&batch_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "只为真正改动的那一章留快照");
}
