//! 拖拽排序后端的回归测试。
//!
//! 拖拽是「前端乐观更新 + 后端落库」两段式：前端立刻重排本地列表，
//! 后端按传入的 id 序列重写 sort_order。因此后端必须保证：
//!
//! 1. reorder_chapters / reorder_volumes 按 id 序列的下标写 sort_order
//! 2. move_chapter 能改 volume_id 并给出新序号（跨卷移动）
//! 3. 排序只影响传入的实体，不波及其他卷的章节
//!
//! 这些性质一旦破坏，用户会看到「拖完弹回去」或「别的卷顺序被弄乱」，
//! 且在不刷新时不易察觉，故用测试固化。
//!
//! Run: cargo test -p easy-writing-core --test reorder

use easy_writing_lib::db;
use rusqlite::Connection;

fn fresh_pool() -> db::DbPool {
    let dir = std::env::temp_dir().join(format!(
        "ew_reorder_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    db::init_db(dir).expect("init db")
}

/// 建卷并在其中建 n 章，返回 (novel_id, volume_id, [chapter_ids])。
fn seed(conn: &Connection, vol_title: &str, n: usize) -> (String, String, Vec<String>) {
    let novel = db::novel::create_novel(conn, "排序测试", "").unwrap();
    let vol = db::volume::create_volume(conn, &novel.id, vol_title, "").unwrap();
    let mut ids = Vec::new();
    for i in 0..n {
        let ch = db::chapter::create_chapter(
            conn,
            &novel.id,
            &vol.id,
            &format!("第 {} 章", i + 1),
            "",
        )
        .unwrap();
        ids.push(ch.id);
    }
    (novel.id, vol.id, ids)
}

/// 按展示顺序读取某卷的章节标题（与前端渲染同一排序规则）。
fn titles_in_order(conn: &Connection, volume_id: &str) -> Vec<String> {
    db::chapter::list_chapter_headers(conn, volume_id)
        .unwrap()
        .into_iter()
        .map(|c| c.title)
        .collect()
}

#[test]
fn reorder_chapters_rewrites_sort_order() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (_, vol_id, ids) = seed(&conn, "卷一", 4);
    assert_eq!(titles_in_order(&conn, &vol_id), vec!["第 1 章", "第 2 章", "第 3 章", "第 4 章"]);

    // 把第 4 章拖到最前：期望顺序 4,1,2,3
    let new_order = vec![ids[3].clone(), ids[0].clone(), ids[1].clone(), ids[2].clone()];
    db::chapter::reorder_chapters(&conn, &new_order).unwrap();

    assert_eq!(titles_in_order(&conn, &vol_id), vec!["第 4 章", "第 1 章", "第 2 章", "第 3 章"]);
}

#[test]
fn reorder_only_affects_target_volume() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (novel_id, vol_a, ids_a) = seed(&conn, "卷一", 3);
    let vol_b = db::volume::create_volume(&conn, &novel_id, "卷二", "").unwrap();
    let b1 = db::chapter::create_chapter(&conn, &novel_id, &vol_b.id, "B-1", "").unwrap();
    let b2 = db::chapter::create_chapter(&conn, &novel_id, &vol_b.id, "B-2", "").unwrap();

    // 只重排卷一
    let reversed: Vec<String> = ids_a.iter().rev().cloned().collect();
    db::chapter::reorder_chapters(&conn, &reversed).unwrap();

    assert_eq!(titles_in_order(&conn, &vol_a), vec!["第 3 章", "第 2 章", "第 1 章"]);
    // 卷二的章节顺序不得被波及
    assert_eq!(titles_in_order(&conn, &vol_b.id), vec!["B-1", "B-2"]);
    // 卷二章节的 id 未变（没被误写成卷一的下标）
    let b_headers = db::chapter::list_chapter_headers(&conn, &vol_b.id).unwrap();
    assert_eq!(b_headers[0].id, b1.id);
    assert_eq!(b_headers[1].id, b2.id);
}

#[test]
fn move_chapter_transfers_between_volumes() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (novel_id, vol_a, ids_a) = seed(&conn, "卷一", 2);
    let vol_b = db::volume::create_volume(&conn, &novel_id, "卷二", "").unwrap();

    // 把卷一的第 1 章移到卷二的最前
    let moved = ids_a[0].clone();
    db::chapter::move_chapter_to_volume(&conn, &moved, &vol_b.id, 0).unwrap();

    assert_eq!(titles_in_order(&conn, &vol_a), vec!["第 2 章"]);
    assert_eq!(titles_in_order(&conn, &vol_b.id), vec!["第 1 章"]);

    // 该章确实换了 volume_id
    let ch = db::chapter::get_chapter(&conn, &moved).unwrap().unwrap();
    assert_eq!(ch.volume_id, vol_b.id);
}

#[test]
fn reorder_volumes_rewrites_sort_order() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "多卷", "").unwrap();
    let v1 = db::volume::create_volume(&conn, &novel.id, "卷一", "").unwrap();
    let v2 = db::volume::create_volume(&conn, &novel.id, "卷二", "").unwrap();
    let v3 = db::volume::create_volume(&conn, &novel.id, "卷三", "").unwrap();

    let titles = |c: &Connection| -> Vec<String> {
        db::volume::list_volumes(c, &novel.id)
            .unwrap()
            .into_iter()
            .map(|v| v.title)
            .collect()
    };
    assert_eq!(titles(&conn), vec!["卷一", "卷二", "卷三"]);

    // 卷三拖到最前
    db::volume::reorder_volumes(&conn, &[v3.id.clone(), v1.id.clone(), v2.id.clone()]).unwrap();
    assert_eq!(titles(&conn), vec!["卷三", "卷一", "卷二"]);
}
