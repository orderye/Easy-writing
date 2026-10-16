//! 作品删除 / 恢复 / 彻底删除的回归测试。
//!
//! 背景：`delete_novel` 长期未被前端调用，而它原本只把 novel 标记为已删除、
//! 不处理其下的卷与章节。既然回收站以「当前作品」为作用域，被删的作品也永远
//! 查不出来——等于「删了就永久丢失」。这组测试锁定修复后的完整语义：
//!
//! 1. 删除作品时级联软删除其卷与章节（否则恢复后作品是空的）
//! 2. 已删除作品能通过跨作品作用域的入口查到
//! 3. 恢复作品时卷与章节一并回来
//! 4. 彻底删除作品会连带清理全部从属数据（依赖外键 ON DELETE CASCADE）
//!
//! Run: cargo test -p easy-writing-core --test novel_trash

use easy_writing_lib::db;
use rusqlite::Connection;

static POOL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn fresh_pool() -> db::DbPool {
    let seq = POOL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ew_novel_trash_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    db::init_db(dir).expect("init db")
}

/// 造一个含 1 卷 2 章的作品。
fn seed_novel(conn: &Connection, title: &str) -> (String, String) {
    let novel = db::novel::create_novel(conn, title, "简介").unwrap();
    let vol = db::volume::create_volume(conn, &novel.id, "第一卷", "").unwrap();
    db::chapter::create_chapter(conn, &novel.id, &vol.id, "第一章", "").unwrap();
    db::chapter::create_chapter(conn, &novel.id, &vol.id, "第二章", "").unwrap();
    (novel.id, vol.id)
}

fn count(conn: &Connection, sql: &str, id: &str) -> i64 {
    conn.query_row(sql, rusqlite::params![id], |r| r.get(0)).unwrap()
}

#[test]
fn soft_delete_novel_cascades_to_volumes_and_chapters() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (novel_id, vol_id) = seed_novel(&conn, "待删作品");

    db::novel::delete_novel(&conn, &novel_id).unwrap();

    // 作品本身被标记
    let novel_deleted = count(
        &conn,
        "SELECT COUNT(*) FROM novel WHERE id = ?1 AND deleted_at IS NOT NULL",
        &novel_id,
    );
    assert_eq!(novel_deleted, 1, "作品应被软删除");

    // 关键：卷与章节也必须被级联标记，否则恢复后作品是空壳
    let vol_deleted = count(
        &conn,
        "SELECT COUNT(*) FROM volume WHERE id = ?1 AND deleted_at IS NOT NULL",
        &vol_id,
    );
    assert_eq!(vol_deleted, 1, "卷应被级联软删除");

    let ch_deleted = count(
        &conn,
        "SELECT COUNT(*) FROM chapter WHERE novel_id = ?1 AND deleted_at IS NOT NULL",
        &novel_id,
    );
    assert_eq!(ch_deleted, 2, "两章都应被级联软删除");

    // 未删除的列表里不应再出现
    let visible = db::novel::list_novels(&conn).unwrap();
    assert!(
        visible.iter().all(|n| n.id != novel_id),
        "已删除作品不应出现在 list_novels"
    );
}

#[test]
fn deleted_novel_is_discoverable_across_novel_scope() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (kept_id, _) = seed_novel(&conn, "保留作品");
    let (doomed_id, _) = seed_novel(&conn, "待删作品");

    db::novel::delete_novel(&conn, &doomed_id).unwrap();

    // 以「保留作品」为作用域的常规回收站**查不到**被删作品——
    // 这正是必须新增跨作品入口的原因
    let scoped = db::trash::list_trash_items(&conn, &kept_id).unwrap();
    assert!(
        scoped.iter().all(|t| t.item_type != "novel"),
        "常规回收站不应出现 novel 类型条目"
    );

    // 跨作品入口能查到
    let deleted = db::trash::list_deleted_novels(&conn).unwrap();
    assert_eq!(deleted.len(), 1, "应能查到 1 个已删除作品");
    assert_eq!(deleted[0].id, doomed_id);
    assert_eq!(deleted[0].item_type, "novel");
    assert_eq!(deleted[0].title, "待删作品");
}

#[test]
fn restore_novel_brings_back_its_content() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (novel_id, _) = seed_novel(&conn, "待恢复作品");

    db::novel::delete_novel(&conn, &novel_id).unwrap();
    assert!(db::trash::list_deleted_novels(&conn).unwrap().len() == 1);

    db::trash::restore_trash_item(&conn, "novel", &novel_id).unwrap();

    // 作品回到未删除列表
    let visible = db::novel::list_novels(&conn).unwrap();
    assert!(
        visible.iter().any(|n| n.id == novel_id),
        "恢复后作品应回到 list_novels"
    );

    // 内容必须一并回来（这是级联删除后恢复的核心保证）
    let vols = db::volume::list_volumes(&conn, &novel_id).unwrap();
    assert_eq!(vols.len(), 1, "恢复后应能看到原卷");

    let chs = db::chapter::list_all_chapter_headers_for_novel(&conn, &novel_id).unwrap();
    assert_eq!(chs.len(), 2, "恢复后应能看到原两章");

    // 回收站里不再有它
    assert!(db::trash::list_deleted_novels(&conn).unwrap().is_empty());
}

#[test]
fn purge_novel_removes_all_dependent_rows() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (novel_id, vol_id) = seed_novel(&conn, "待彻底删除");

    db::novel::delete_novel(&conn, &novel_id).unwrap();
    db::trash::purge_trash_item(&conn, "novel", &novel_id).unwrap();

    let novel_rows = count(&conn, "SELECT COUNT(*) FROM novel WHERE id = ?1", &novel_id);
    assert_eq!(novel_rows, 0, "novel 行应被删除");

    let vol_rows = count(&conn, "SELECT COUNT(*) FROM volume WHERE id = ?1", &vol_id);
    assert_eq!(vol_rows, 0, "外键 CASCADE 应清理卷");

    let ch_rows = count(
        &conn,
        "SELECT COUNT(*) FROM chapter WHERE novel_id = ?1",
        &novel_id,
    );
    assert_eq!(ch_rows, 0, "外键 CASCADE 应清理章节");

    assert!(db::trash::list_deleted_novels(&conn).unwrap().is_empty());
}

#[test]
fn deleting_one_novel_leaves_others_untouched() {
    let pool = fresh_pool();
    let conn = pool.get().unwrap();
    let (keep_id, keep_vol) = seed_novel(&conn, "A 作品");
    let (drop_id, _) = seed_novel(&conn, "B 作品");

    db::novel::delete_novel(&conn, &drop_id).unwrap();

    // 未删的作品内容必须毫发无损
    assert_eq!(db::volume::list_volumes(&conn, &keep_id).unwrap().len(), 1);
    assert_eq!(
        db::chapter::list_all_chapter_headers_for_novel(&conn, &keep_id)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM volume WHERE id = ?1 AND deleted_at IS NULL",
            &keep_vol,
        ),
        1,
        "另一个作品的卷不应被误标记"
    );
}
