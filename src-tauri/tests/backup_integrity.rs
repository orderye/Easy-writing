//! 备份完整性与恢复预检的回归测试（TDD：先钉住失败，再改实现）。
//!
//! 覆盖基线审计中的 C-01 / C-02 / C-03 / C-11 四条已确认缺陷：
//!
//! - C-01：`webdav_restore_backup` 的 filename 未做路径穿越校验；
//! - C-02：恢复前无预检，损坏 / 伪造 / 空库备份可覆盖可用数据库；
//! - C-03：轮转在新备份未验证前删除旧备份；
//! - C-11：图片覆盖写非原子。
//!
//! 全部用例只在 `std::env::temp_dir()` 下的隔离目录中构造数据，
//! **不接触用户真实数据库**。

use easy_writing_lib::db;
use easy_writing_lib::sync::backup;
use easy_writing_lib::sync::manifest;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ew_backup_{}_{}_{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 用当前真实代码造一个「有点内容的」数据库，作为备份源。
fn seed_novel(dir: &Path) -> (db::DbPool, String, String) {
    let pool = db::init_db(dir.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    let novel_id = db::novel::create_novel(&conn, "备份测试作品", "梗概").unwrap().id;
    let vol = db::volume::create_volume(&conn, &novel_id, "第一卷", "卷梗概").unwrap();
    let chap = db::chapter::create_chapter(&conn, &novel_id, &vol.id, "第一章", "章节梗概")
        .unwrap();
    (pool, novel_id, chap.id)
}

// ---------------------------------------------------------------------------
// C-01 · 文件名校验
// ---------------------------------------------------------------------------

#[test]
fn backup_filename_rejects_path_traversal() {
    // 这些输入必须全部被拒绝。把它们放进 URL 会离开 remote_dir。
    for evil in [
        "../../etc/backup_x.db.gz",
        "..%2F..%2Fbackup_x.db.gz",
        "sub/backup_x.db.gz",
        "./backup_x.db.gz",
        "..\\windows\\backup_x.db.gz",
        "/absolute/backup_x.db.gz",
        "a/../../b.db.gz",
    ] {
        assert!(
            backup::validate_backup_filename(evil).is_err(),
            "路径穿越输入必须被拒绝：{evil}"
        );
    }
}

#[test]
fn backup_filename_accepts_legacy_and_current_formats() {
    // 真实存在的历史文件名必须继续可用，否则等于宣布所有旧备份作废。
    for ok in [
        "backup_20260101_000000Z.db.gz",
        "backup_20260101_000000Z.db.gz.json",
        "some-local-manual-copy.db.gz",
        "中文文件名.db.gz",
    ] {
        assert!(
            backup::validate_backup_filename(ok).is_ok(),
            "合法备份文件名不应被拒绝：{ok}"
        );
    }
}

// ---------------------------------------------------------------------------
// C-02 · 恢复预检
// ---------------------------------------------------------------------------

#[test]
fn preflight_rejects_non_sqlite_payload() {
    let dir = temp_dir("preflight_garbage");
    let bogus = dir.join("garbage.db");
    // 一段完全不是 SQLite 的字节流：SQLite 文件头必须是 "SQLite format 3\0"。
    fs::write(&bogus, b"this is definitely not a sqlite database, not even close").unwrap();

    // 关键断言是「必须被拒绝」。SQLite 会延迟打开，因此这些字节流可能
    // 能被 open，但一定过不了完整性检查 —— 两条路径都必须拦下。
    let reason = backup::preflight_backup(&bogus).expect_err("非 SQLite 数据必须在预检阶段被拒绝");
    assert!(
        reason.contains("SQLite") || reason.contains("完整性"),
        "错误信息应说明文件不是可用数据库，实际：{reason}"
    );
}

#[test]
fn preflight_rejects_truncated_database() {
    let dir = temp_dir("preflight_truncated");
    let (pool, _, _) = seed_novel(&dir);
    let full = dir.join("full.db");
    backup::snapshot_to_file(&pool, &full).unwrap();
    drop(pool);

    let bytes = fs::read(&full).unwrap();
    assert!(bytes.len() > 1000, "样本数据库不应过小，实际 {} 字节", bytes.len());

    // 砍掉一半 —— 典型的「上传中断 / 下载截断」
    let truncated = dir.join("truncated.db");
    fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();

    assert!(
        backup::preflight_backup(&truncated).is_err(),
        "截断的数据库必须在预检阶段被拒绝，不能覆盖可用数据"
    );
}

#[test]
fn preflight_accepts_healthy_backup() {
    let dir = temp_dir("preflight_ok");
    let (pool, novel_id, _) = seed_novel(&dir);
    let snap = dir.join("good.db");
    backup::snapshot_to_file(&pool, &snap).unwrap();
    drop(pool);

    let report = backup::preflight_backup(&snap).unwrap();
    assert!(report.is_valid, "健康备份应通过预检");
    assert_eq!(report.novel_count, 1);
    assert!(report.chapter_count >= 1);
    assert_eq!(report.schema_version, db::BASELINE_VERSION);
    let _ = novel_id;
}

#[test]
fn preflight_flags_empty_database_as_suspicious() {
    let dir = temp_dir("preflight_empty");
    // 一个结构完整但一条作品都没有的库。它可能是合法的新备份，
    // 也可能是被清空 / 损坏的产物 —— 预检必须让调用方有机会问一句。
    let empty = dir.join("empty.db");
    {
        let pool = db::init_db(dir.join("seed")).unwrap();
        drop(pool);
        let seeded = dir.join("seed").join("easy-writing.db");
        backup::copy_database(&seeded, &empty).unwrap();
    }

    let report = backup::preflight_backup(&empty).unwrap();
    assert!(report.is_valid, "结构完整即视为格式合法");
    assert!(
        report.is_empty_library,
        "空作品库必须被单独标记，交由上层决定是否确认恢复"
    );
}

// ---------------------------------------------------------------------------
// 清单（manifest）
// ---------------------------------------------------------------------------

#[test]
fn manifest_records_schema_and_entity_counts() {
    let dir = temp_dir("manifest_basic");
    let (pool, _, _) = seed_novel(&dir);
    let conn = pool.get().unwrap();

    let m = manifest::build_manifest(&conn, "0.3.8", 42);

    assert_eq!(m.manifest_version, manifest::MANIFEST_VERSION);
    assert_eq!(m.app_version, "0.3.8");
    assert_eq!(m.schema_version, db::BASELINE_VERSION);
    assert_eq!(m.entity_counts.get("novel").copied(), Some(1));
    assert!(m.entity_counts.contains_key("chapter"));
    assert_eq!(m.compressed_size_bytes, 42);
    Ok::<(), String>(()).unwrap();
}

#[test]
fn manifest_never_contains_secrets() {
    let dir = temp_dir("manifest_secrets");
    let (pool, novel_id, _) = seed_novel(&dir);
    let conn = pool.get().unwrap();

    // 先把「秘密」写进库里，确保清单生成过程真的读到了这些表
    conn.execute(
        "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir)
         VALUES ('default_webdav', 'https://dav.example.com', 'alice', 'webdav_default_pwd', 'ew')
         ON CONFLICT(id) DO UPDATE SET url = excluded.url",
        [],
    )
    .unwrap();
    db::llm_profile::create_profile(
        &conn,
        "gpt",
        "openai",
        "https://api.openai.com/v1",
        "sk-SUPERSECRET-KEY",
        "gpt-4",
        "",
        false,
    )
    .ok();

    let m = manifest::build_manifest(&conn, "0.3.8", 1);
    let json = serde_json::to_string(&m).unwrap();

    assert!(
        !json.contains("SUPERSECRET"),
        "清单绝不能包含 API Key，实际序列化结果：{json}"
    );
    assert!(
        !json.contains("dav.example.com"),
        "清单不能包含 WebDAV 服务器地址"
    );
    assert!(
        !json.contains("webdav_default_pwd"),
        "清单不能包含钥匙串引用键名"
    );
    let _ = novel_id;
}

#[test]
fn manifest_roundtrips_through_json() {
    let dir = temp_dir("manifest_json");
    let (pool, _, _) = seed_novel(&dir);
    let conn = pool.get().unwrap();
    let m = manifest::build_manifest(&conn, "0.3.8", 7);

    let text = serde_json::to_string_pretty(&m).unwrap();
    let parsed: manifest::BackupManifest = serde_json::from_str(&text).unwrap();

    assert_eq!(parsed.manifest_version, m.manifest_version);
    assert_eq!(parsed.schema_version, m.schema_version);
    assert_eq!(parsed.entity_counts, m.entity_counts);
    assert_eq!(parsed.created_at, m.created_at);
}

#[test]
fn manifest_asset_entry_carries_hash_and_size() {
    let dir = temp_dir("manifest_asset");
    let (pool, novel_id, _) = seed_novel(&dir);
    let conn = pool.get().unwrap();

    // 写一个真实图片资产（1×1 PNG）
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let rel = easy_writing_lib::asset::save_image(&conn, &dir, &novel_id, "测试图", "image/png", png).unwrap();
    drop(conn);

    let conn = pool.get().unwrap();
    let m = manifest::build_manifest(&conn, "0.3.8", 1);

    let entry = m
        .assets
        .iter()
        .find(|a| a.rel_path == rel)
        .expect("清单必须包含刚写入的图片资产");
    assert!(
        entry.size > 0,
        "资产大小必须来自真实文件，不能是占位值"
    );
    assert_eq!(entry.hash.len(), 64, "必须是 SHA-256 十六进制长度");
}

// ---------------------------------------------------------------------------
// 轮转（C-03）
// ---------------------------------------------------------------------------

#[test]
fn rotation_never_drops_below_verified_backup_count() {
    // 轮转策略的核心不变量：无论新备份是否通过校验，
    // 远端保有的「已验证备份」数量都不会下降。
    let names = vec![
        "backup_20260103_000000Z.db.gz".to_string(),
        "backup_20260102_000000Z.db.gz".to_string(),
        "backup_20260101_000000Z.db.gz".to_string(),
    ];

    // 校验通过时：保留最新 2 个，删除最旧的 1 个
    let to_delete = backup::plan_rotation(&names, true, 2);
    assert_eq!(to_delete.len(), 1, "3 份保留 2 份，应删除 1 份");
    assert_eq!(names.len() - to_delete.len(), 2, "校验通过时应保留 2 份");

    // 新备份未通过校验 —— 此时不应该发生任何删除
    let unverified = backup::plan_rotation(&names, false, 2);
    assert!(
        unverified.is_empty(),
        "新备份未验证时不得删除任何旧备份，实际：{unverified:?}"
    );
}

#[test]
fn rotation_only_considers_recognised_backup_names() {
    // 用户手动放进远端目录的其他 .db.gz 文件不应被当成备份参与轮转删除。
    let names = vec![
        "backup_20260103_000000Z.db.gz".to_string(),
        "backup_20260102_000000Z.db.gz".to_string(),
        "backup_20260101_000000Z.db.gz".to_string(),
        "somebody-elses-file.db.gz".to_string(),
    ];
    let delete = backup::plan_rotation(&names, true, 2);
    assert_eq!(delete.len(), 1);
    assert!(
        delete[0].starts_with("backup_"),
        "只应删除自动备份，实际计划删除：{delete:?}"
    );
    assert!(
        !delete.iter().any(|n| n.contains("somebody")),
        "绝不能删除非本应用创建的备份"
    );
}

// ---------------------------------------------------------------------------
// 原子写（C-11）
// ---------------------------------------------------------------------------

#[test]
fn atomic_write_replaces_file_without_truncation_window() {
    let dir = temp_dir("atomic_write");
    let target = dir.join("image.bin");
    fs::write(&target, b"old-content").unwrap();

    backup::atomic_write(&target, b"new-content-that-is-longer").unwrap();

    assert_eq!(
        fs::read(&target).unwrap(),
        b"new-content-that-is-longer",
        "原子写应完整替换内容"
    );
    // 不应留下临时文件残留
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "原子写后不应残留临时文件：{leftovers:?}");
}

#[test]
fn snapshot_is_consistent_under_wal() {
    // SQLite Backup API 在 WAL 下也必须给出完整快照。
    let dir = temp_dir("wal_snapshot");
    let (pool, _, _) = seed_novel(&dir);
    let snap = dir.join("snap.db");
    backup::snapshot_to_file(&pool, &snap).unwrap();

    let journal = format!("{}-wal", snap.display().to_string());
    assert!(
        !Path::new(&journal).exists(),
        "快照落盘后不应残留 WAL 旁文件"
    );

    let conn = Connection::open(&snap).unwrap();
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok", "快照自身必须通过完整性检查");
}
