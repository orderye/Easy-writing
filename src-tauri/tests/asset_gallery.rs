//! 图片资产（图库）的行为契约。
//!
//! 这些用例盯三件容易出错的事：按作品隔离、内容哈希去重下「一行不等于一文件」、
//! 以及**删记录与删文件的界限**（本轮一律不删文件，回收交给孤儿清理）。
//!
//! Run: cargo test -p easy-writing-core --test asset_gallery

use easy_writing_lib::{asset, db};
use std::path::Path;

/// 两种不同内容的 base64。本模块只解码字节并算哈希，不校验图片格式，
/// 因此不必放真实图片数据。
const BLOB_A: &str = "aGVsbG8=";
const BLOB_B: &str = "d29ybGQ=";

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ew_asset_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn images_files(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir.join("images"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

#[test]
fn list_assets_is_novel_scoped_and_newest_first() {
    let dir = temp_dir("scope");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let a = db::novel::create_novel(&conn, "A", "").unwrap().id;
    let b = db::novel::create_novel(&conn, "B", "").unwrap().id;

    asset::save_image(&conn, &dir, &a, "first.png", "image/png", BLOB_A).unwrap();
    asset::save_image(&conn, &dir, &a, "second.png", "image/png", BLOB_B).unwrap();
    asset::save_image(&conn, &dir, &b, "other.png", "image/png", BLOB_A).unwrap();

    let la = asset::list_assets(&conn, &a).unwrap();
    let lb = asset::list_assets(&conn, &b).unwrap();

    assert_eq!(la.len(), 2, "作品 A 应只看到自己的两张");
    assert_eq!(lb.len(), 1, "作品 B 不应看到 A 的图片");
    assert!(la.iter().all(|x| x.novel_id == a));
    assert_eq!(lb[0].name, "other.png");
    // 创建时间精确到秒，同秒内靠 rowid 次级排序保证「后传的在前」
    assert_eq!(la[0].name, "second.png");
    assert_eq!(la[1].name, "first.png");
}

#[test]
fn save_image_dedupes_file_but_records_each_upload() {
    let dir = temp_dir("dedupe");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "N", "").unwrap().id;

    let rel1 = asset::save_image(&conn, &dir, &novel, "one.png", "image/png", BLOB_A).unwrap();
    let rel2 = asset::save_image(&conn, &dir, &novel, "again.png", "image/png", BLOB_A).unwrap();

    let list = asset::list_assets(&conn, &novel).unwrap();
    assert_eq!(list.len(), 2, "每次上传都应留一条记录（各自可命名）");
    assert_eq!(rel1, rel2, "同内容映射到同一个文件");
    assert!(list.iter().all(|x| x.rel_path == rel1));
    assert_eq!(images_files(&dir).len(), 1, "同内容只写一个文件");
}

#[test]
fn soft_delete_hides_from_gallery_and_keeps_the_file() {
    let dir = temp_dir("soft");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "N", "").unwrap().id;

    let rel = asset::save_image(&conn, &dir, &novel, "one.png", "image/png", BLOB_A).unwrap();
    let id = asset::list_assets(&conn, &novel).unwrap()[0].id.clone();

    asset::soft_delete_asset(&conn, &id).unwrap();
    assert!(asset::list_assets(&conn, &novel).unwrap().is_empty());
    let deleted = asset::list_deleted_assets(&conn, &novel).unwrap();
    assert_eq!(deleted.len(), 1);
    assert!(deleted[0].deleted_at.is_some());
    assert!(dir.join(&rel).exists(), "软删除绝不能动磁盘文件");
    // 重复移除是幂等的，不报错
    asset::soft_delete_asset(&conn, &id).unwrap();

    asset::restore_asset(&conn, &id).unwrap();
    assert_eq!(asset::list_assets(&conn, &novel).unwrap().len(), 1);
    assert!(asset::list_deleted_assets(&conn, &novel).unwrap().is_empty());
}

#[test]
fn rename_asset_does_not_touch_file_or_path() {
    let dir = temp_dir("rename");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "N", "").unwrap().id;

    let rel = asset::save_image(&conn, &dir, &novel, "one.png", "image/png", BLOB_A).unwrap();
    let id = asset::list_assets(&conn, &novel).unwrap()[0].id.clone();

    asset::rename_asset(&conn, &id, "  雨夜天台  ").unwrap();
    let after = asset::list_assets(&conn, &novel).unwrap().remove(0);
    assert_eq!(after.name, "雨夜天台", "名称应去掉首尾空白");
    assert_eq!(after.rel_path, rel, "重命名不得改变文件路径");
    assert!(dir.join(&rel).exists());

    assert!(asset::rename_asset(&conn, &id, "   ").is_err(), "空名应被拒绝");
    assert!(asset::rename_asset(&conn, "no-such-id", "x").is_err());
}

#[test]
fn purge_asset_removes_only_the_record_not_the_file() {
    let dir = temp_dir("purge");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "N", "").unwrap().id;

    let rel = asset::save_image(&conn, &dir, &novel, "one.png", "image/png", BLOB_A).unwrap();
    asset::save_image(&conn, &dir, &novel, "two.png", "image/png", BLOB_A).unwrap();

    let list = asset::list_assets(&conn, &novel).unwrap();
    asset::purge_asset(&conn, &list[0].id).unwrap();
    let left = asset::list_assets(&conn, &novel).unwrap();
    assert_eq!(left.len(), 1, "只应删掉被指定的那一条");
    assert_eq!(left[0].rel_path, rel);
    assert!(dir.join(&rel).exists());

    // 记录全部删完，文件仍在 —— 这正是孤儿清理的输入
    asset::purge_asset(&conn, &left[0].id).unwrap();
    assert!(asset::list_assets(&conn, &novel).unwrap().is_empty());
    assert!(dir.join(&rel).exists(), "记录删除不负责删文件");
}

#[test]
fn asset_usage_counts_chapter_and_revision_references() {
    let dir = temp_dir("usage");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "N", "").unwrap().id;
    let vol = db::volume::create_volume(&conn, &novel, "V", "").unwrap().id;
    let chap = db::chapter::create_chapter(&conn, &novel, &vol, "C", "").unwrap().id;

    asset::save_image(&conn, &dir, &novel, "one.png", "image/png", BLOB_A).unwrap();
    asset::save_image(&conn, &dir, &novel, "again.png", "image/png", BLOB_A).unwrap();
    let list = asset::list_assets(&conn, &novel).unwrap();
    let target = list[0].clone();

    let usage = asset::asset_usage(&conn, &target.id).unwrap();
    assert_eq!(usage.shared_records, 1, "同内容的另一条记录应被数出来");
    assert_eq!(usage.referencing_chapters, 0);
    assert_eq!(usage.referencing_revisions, 0);

    // 正文里出现该图（URL 可能被百分号编码，故引用检查按 hash 匹配）
    let content = format!(
        "{{\"type\":\"image\",\"attrs\":{{\"src\":\"asset://localhost/%2Ftmp%2Fimages%2F{}.png\"}}}}",
        target.hash
    );
    db::chapter::save_chapter_content(&conn, &chap, &content, "[图片]", 4).unwrap();
    let usage = asset::asset_usage(&conn, &target.id).unwrap();
    assert_eq!(usage.referencing_chapters, 1);

    // 历史版本同样算引用（版本回退后会用到这张图）
    db::chapter_revision::create_revision(
        &conn, &chap, &novel, "C", &content, "[图片]", 4, "manual", "t",
    )
    .unwrap();
    let usage = asset::asset_usage(&conn, &target.id).unwrap();
    assert_eq!(usage.referencing_revisions, 1);
}

#[test]
fn purge_novel_leaves_image_files_on_disk() {
    // 反向复现：彻底删除作品时 asset 行随外键 CASCADE 消失，但图片文件不会被删。
    // 这就是图库「维护」里孤儿清理存在的理由 —— 把缺陷钉在测试里。
    let dir = temp_dir("orphan");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "N", "").unwrap().id;

    let rel = asset::save_image(&conn, &dir, &novel, "one.png", "image/png", BLOB_A).unwrap();
    assert_eq!(asset::list_assets(&conn, &novel).unwrap().len(), 1);

    db::trash::purge_novel(&conn, &novel).unwrap();

    assert!(
        asset::list_assets(&conn, &novel).unwrap().is_empty(),
        "作品没了，记录应随之 CASCADE 清理"
    );
    assert!(
        dir.join(&rel).exists(),
        "但文件仍留在磁盘上（孤儿），需要「维护」来回收"
    );
}
