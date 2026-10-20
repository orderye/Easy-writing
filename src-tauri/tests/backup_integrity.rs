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

    let m = manifest::build_manifest(&conn, "0.3.8", 42, "").unwrap();

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

    let m = manifest::build_manifest(&conn, "0.3.8", 1, "").unwrap();
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
    let m = manifest::build_manifest(&conn, "0.3.8", 7, "").unwrap();

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
    let m = manifest::build_manifest(&conn, "0.3.8", 1, "").unwrap();

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

// ---------------------------------------------------------------------------
// P0-1 · 清单交叉校验
//
// 这些用例针对的缺陷是：恢复流程**从未下载远端清单**。旧代码把解压后的
// 临时库路径交给 cross_check_manifest，让它去找 `<临时库>.db.json` ——
// 那个文件永远不存在，于是每次恢复都得到「该备份没有清单文件（历史备份）」，
// 而真正带清单的新备份也一并被降级成只校验结构。
// 下面钉住的是修复后的契约：清单必须被真实读取、真实比对，
// 且「损坏」「版本不支持」「与备份不匹配」都不能退化成「清单不存在」。
// ---------------------------------------------------------------------------

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

/// 一份带图片资产的真实备份：返回（快照库路径, 清单, 压缩包字节）。
fn seed_backup_with_manifest(dir: &Path) -> (PathBuf, manifest::BackupManifest, Vec<u8>) {
    let app = dir.join("app");
    let pool = db::init_db(app.clone()).unwrap();
    {
        let conn = pool.get().unwrap();
        let nid = db::novel::create_novel(&conn, "清单校验作品", "梗概").unwrap().id;
        let vol = db::volume::create_volume(&conn, &nid, "第一卷", "").unwrap();
        db::chapter::create_chapter(&conn, &nid, &vol.id, "第一章", "").unwrap();
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", png).unwrap();
    }

    let snap = dir.join("backup.db");
    backup::snapshot_to_file(&pool, &snap).unwrap();

    let gz = gzip(&fs::read(&snap).unwrap());
    let m = {
        let conn = pool.get().unwrap();
        let mut built = manifest::build_manifest(
            &conn,
            "0.3.8",
            gz.len() as u64,
            &manifest::sha256_hex(&gz),
        )
        .unwrap();
        manifest::reconcile_with_disk(&mut built, &app);
        built
    };
    drop(pool);
    (snap, m, gz)
}

#[test]
fn valid_manifest_passes_cross_check() {
    let dir = temp_dir("mf_ok");
    let (snap, m, gz) = seed_backup_with_manifest(&dir);

    let v = manifest::verify_manifest(
        &m,
        &snap,
        Some(gz.len() as u64),
        Some(&manifest::sha256_hex(&gz)),
        Some(&dir.join("app")),
    )
    .unwrap();

    assert!(v.is_ok(), "匹配的清单不应产生 fatal，实际：{:?}", v.fatal);
    assert!(!m.assets.is_empty(), "样本备份应当含有一个图片资产");
}

#[test]
fn tampered_novel_count_is_rejected() {
    let dir = temp_dir("mf_novel");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    *m.entity_counts.get_mut("novel").unwrap() += 1;

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("novel")),
        "作品数被篡改必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

#[test]
fn tampered_chapter_count_is_rejected() {
    let dir = temp_dir("mf_chapter");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    *m.entity_counts.get_mut("chapter").unwrap() += 1;

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("chapter")),
        "章节数被篡改必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

#[test]
fn tampered_count_of_every_other_entity_is_rejected() {
    // 旧实现只比对 novel 与 chapter 两张表。清单里其余 9 张表被篡改
    // 一律放行，等于清单校验只覆盖了九分之二。这里逐表钉住。
    let dir = temp_dir("mf_all_tables");
    let (snap, base, gz) = seed_backup_with_manifest(&dir);

    let tables: Vec<String> = base.entity_counts.keys().cloned().collect();
    assert!(
        tables.len() >= 9,
        "清单应覆盖全部被统计的实体表，实际只有 {} 张",
        tables.len()
    );

    for table in tables {
        if table == "novel" || table == "chapter" {
            continue; // 已由上面两个用例覆盖
        }
        let mut m = base.clone();
        *m.entity_counts.get_mut(&table).unwrap() += 7;
        let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
        assert!(
            v.fatal.iter().any(|f| f.contains(&table)),
            "{table} 的计数被篡改必须判为 fatal，实际：{:?}",
            v.fatal
        );
    }
}

#[test]
fn missing_entity_count_in_manifest_is_rejected() {
    let dir = temp_dir("mf_missing_count");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.entity_counts.remove("character_relation");

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("character_relation")),
        "清单缺少某张表的计数必须判为 fatal（不能当成 0），实际：{:?}",
        v.fatal
    );
}

#[test]
fn negative_entity_count_is_rejected() {
    let dir = temp_dir("mf_negative");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    *m.entity_counts.get_mut("volume").unwrap() = -5;

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("负数")),
        "负数计数必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

#[test]
fn compressed_size_mismatch_is_rejected() {
    let dir = temp_dir("mf_size");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.compressed_size_bytes = gz.len() as u64 + 1;

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("压缩包")),
        "压缩大小不匹配必须判为 fatal（备份可能被截断），实际：{:?}",
        v.fatal
    );
}

#[test]
fn manifest_belonging_to_another_backup_is_rejected() {
    // 清单里的 compressed_sha256 把清单绑定到具体某一份备份包。
    // 少了这层绑定，把 A 的清单挂到 B 旁边，只要计数恰好相同就查不出来。
    let dir = temp_dir("mf_hash");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.compressed_sha256 = manifest::sha256_hex(b"a completely different backup payload");

    let v = manifest::verify_manifest(
        &m,
        &snap,
        Some(gz.len() as u64),
        Some(&manifest::sha256_hex(&gz)),
        None,
    )
    .unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("不属于该备份")),
        "清单哈希与备份包不符必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

#[test]
fn schema_version_mismatch_is_rejected() {
    let dir = temp_dir("mf_schema");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.schema_version = db::BASELINE_VERSION - 1;

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("schema")),
        "清单声明的 schema 与备份实际不符必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

#[test]
fn unsupported_manifest_version_is_rejected_not_ignored() {
    let dir = temp_dir("mf_version");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.manifest_version = manifest::MANIFEST_VERSION + 1;

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("清单格式版本")),
        "来自更新版本的清单必须被明确拒绝，不能默默忽略版本字段，实际：{:?}",
        v.fatal
    );
}

#[test]
fn corrupt_manifest_json_is_an_error_not_a_legacy_backup() {
    // 这是 P0-1 的核心红线：损坏的清单绝不能被当成「历史无清单备份」，
    // 否则篡改或截断清单就能把恢复降级到只校验结构的兼容分支。
    let dir = temp_dir("mf_corrupt");
    let backup_path = dir.join("backup_20260101_000000Z.db.gz");
    fs::write(&backup_path, b"payload").unwrap();
    fs::write(
        manifest::sidecar_path_for(&backup_path),
        b"{\"manifest_version\": 2, \"created_at\": ",
    )
    .unwrap();

    let err = manifest::read_sidecar(&backup_path)
        .expect_err("损坏的清单必须返回 Err，而不是 Ok(None)");
    assert!(
        err.contains("解析失败"),
        "错误信息应说明清单损坏，实际：{err}"
    );
}

#[test]
fn absent_manifest_is_distinguishable_from_corrupt_manifest() {
    let dir = temp_dir("mf_absent");
    let backup_path = dir.join("backup_20260101_000000Z.db.gz");
    fs::write(&backup_path, b"payload").unwrap();

    // 没有 sidecar = 历史备份的正常兼容路径，必须是 Ok(None)
    assert_eq!(
        manifest::read_sidecar(&backup_path).unwrap(),
        None,
        "清单不存在应返回 Ok(None)，代表历史备份"
    );
}

#[test]
fn legacy_backup_without_manifest_still_passes_structural_preflight() {
    // 历史无清单备份必须照常可恢复，否则新增清单等于宣布所有旧备份作废。
    let dir = temp_dir("mf_legacy");
    let (pool, _, _) = seed_novel(&dir);
    let snap = dir.join("legacy.db");
    backup::snapshot_to_file(&pool, &snap).unwrap();
    drop(pool);

    assert_eq!(manifest::read_sidecar(&snap).unwrap(), None);
    let report = backup::preflight_backup(&snap).unwrap();
    assert!(report.is_valid, "无清单的历史备份应通过结构预检");
    assert_eq!(report.novel_count, 1);
}

#[test]
fn manifest_asset_path_traversal_is_rejected() {
    let dir = temp_dir("mf_traversal");
    let (snap, m, gz) = seed_backup_with_manifest(&dir);

    for evil in [
        "../../../etc/passwd",
        "/etc/passwd",
        "images/../../secret.png",
        "C:\\images\\a.png",
    ] {
        let mut tampered = m.clone();
        tampered.assets.push(manifest::AssetEntry {
            rel_path: evil.to_string(),
            size: 1,
            hash: "0".repeat(64),
        });
        let v =
            manifest::verify_manifest(&tampered, &snap, Some(gz.len() as u64), None, None).unwrap();
        assert!(
            v.fatal.iter().any(|f| f.contains("资产路径非法") || f.contains("清单声明了资产")),
            "非法资产路径必须被拒绝：{evil}，实际：{:?}",
            v.fatal
        );
        assert!(
            manifest::resolve_asset_path(&dir, evil).is_err(),
            "路径解析必须拒绝越界：{evil}"
        );
    }
}

#[test]
fn duplicate_asset_paths_in_manifest_are_rejected() {
    let dir = temp_dir("mf_dup");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    let first = m.assets[0].clone();
    m.assets.push(first);

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("重复")),
        "重复资产路径必须被检测并报告，实际：{:?}",
        v.fatal
    );
}

#[test]
fn invalid_asset_hash_format_is_rejected() {
    let dir = temp_dir("mf_badhash");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.assets[0].hash = "not-a-sha256".to_string();

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("SHA-256")),
        "非法哈希格式必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

#[test]
fn unknown_entity_table_in_manifest_is_rejected_without_being_queried() {
    // 清单来自远端，属于不可信输入。如果直接把它给的表名拼进
    // `SELECT COUNT(*) FROM <表名>`，一份被篡改的清单就能注入 SQL。
    let dir = temp_dir("mf_injection");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.entity_counts
        .insert("novel; DROP TABLE novel--".to_string(), 1);

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("不认识的实体表")),
        "白名单之外的表名必须被拒绝，实际：{:?}",
        v.fatal
    );

    // 并且目标库必须完好无损（证明那条注入语句从未被执行）
    let conn = Connection::open(&snap).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM novel", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "novel 表必须仍然存在且数据完好");
}

#[test]
fn manifest_declaring_asset_absent_from_database_is_rejected() {
    // 清单声明了一个数据库里不存在的资产 —— 说明这份清单不是为这个库生成的。
    let dir = temp_dir("mf_orphan_asset");
    let (snap, mut m, gz) = seed_backup_with_manifest(&dir);
    m.assets.push(manifest::AssetEntry {
        rel_path: "images/does-not-exist-in-db.png".to_string(),
        size: 10,
        hash: manifest::sha256_hex(b"whatever"),
    });

    let v = manifest::verify_manifest(&m, &snap, Some(gz.len() as u64), None, None).unwrap();
    assert!(
        v.fatal.iter().any(|f| f.contains("清单与备份不匹配")),
        "清单与数据库的资产集合不一致必须判为 fatal，实际：{:?}",
        v.fatal
    );
}

// ---------------------------------------------------------------------------
// P0-1 D · 清单生成的错误传播
// ---------------------------------------------------------------------------

#[test]
fn schema_stat_failure_makes_manifest_generation_fail() {
    // 旧实现用 unwrap_or(0)：统计查询失败会生成一份计数全为 0 的清单。
    // 那样的清单在恢复交叉校验时**必然通过**（因为备份里也读不出数据），
    // 于是「校验通过」变成了「校验的是伪造数据」。
    let dir = temp_dir("mf_gen_fail");
    let (pool, _, _) = seed_novel(&dir);
    {
        let conn = pool.get().unwrap();
        conn.execute("DROP TABLE novel_lexicon", []).unwrap();
    }
    let conn = pool.get().unwrap();
    let err = manifest::build_manifest(&conn, "0.3.8", 1, "")
        .expect_err("统计表读不出来时，清单生成必须失败");
    assert!(
        err.contains("novel_lexicon"),
        "错误信息应指明是哪张表统计失败，实际：{err}"
    );
}

#[test]
fn schema_version_read_failure_makes_manifest_generation_fail() {
    let dir = temp_dir("mf_schema_fail");
    let db_path = dir.join("no_schema_version.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("CREATE TABLE novel (id TEXT PRIMARY KEY);")
        .unwrap();

    let err = manifest::build_manifest(&conn, "0.3.8", 1, "")
        .expect_err("schema 版本读不出来时，清单生成必须失败");
    assert!(
        err.contains("schema"),
        "错误信息应说明是 schema 版本读取失败，实际：{err}"
    );
}

#[test]
fn asset_query_failure_is_not_reported_as_no_assets() {
    // 资产表读不出来 ≠ 这个库没有图片。后者会让恢复认为「素材齐全」。
    let dir = temp_dir("mf_asset_fail");
    let (pool, _, _) = seed_novel(&dir);
    {
        let conn = pool.get().unwrap();
        conn.execute("DROP TABLE asset", []).unwrap();
    }
    let conn = pool.get().unwrap();
    let err = manifest::build_manifest(&conn, "0.3.8", 1, "")
        .expect_err("资产表无法读取时必须报错，不得伪装成「没有资产」");
    assert!(
        err.contains("资产"),
        "错误信息应指明资产表读取失败，实际：{err}"
    );
}

#[test]
fn asset_missing_on_disk_is_recorded_not_silently_dropped() {
    // reconcile_with_disk 会把缺文件的资产从 assets 里剔除（清单只应描述
    // 真实存在于备份中的东西）。但剔除动作本身不能让「备份时就已经丢图」
    // 这件事消失 —— 它必须留在 missing_assets 里，并回报给调用方。
    let dir = temp_dir("mf_missing_asset");
    let app = dir.join("app");
    let snap = dir.join("backup.db");
    let mut m = {
        let pool = db::init_db(app.clone()).unwrap();
        let conn = pool.get().unwrap();
        let nid = db::novel::create_novel(&conn, "缺图作品", "").unwrap().id;
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", png).unwrap();
        backup::snapshot_to_file(&pool, &snap).unwrap();
        let built = manifest::build_manifest(&conn, "0.3.8", 1, "").unwrap();
        drop(pool);
        built
    };

    assert_eq!(m.assets.len(), 1, "文件存在时资产应进入清单");
    assert!(m.missing_assets.is_empty());

    // 现在把文件删掉，模拟「数据库有记录但本地文件已丢失」
    let deleted_rel = m.assets[0].rel_path.clone();
    fs::remove_file(app.join(&deleted_rel)).unwrap();
    let missing = manifest::reconcile_with_disk(&mut m, &app);

    assert_eq!(missing, vec![deleted_rel.clone()], "必须回报缺失的资产");
    assert!(m.assets.is_empty(), "缺文件的资产不应留在 assets 里");
    assert_eq!(
        m.missing_assets,
        vec![deleted_rel.clone()],
        "缺失必须永久记录在清单中，恢复时才能告知作者"
    );

    // 恢复侧也必须能看见：数据库仍引用该资产，而清单里没有它
    let v = manifest::verify_manifest(&m, &snap, None, None, None).unwrap();
    assert!(
        v.is_ok(),
        "备份时就已缺失的素材属于已知情况，不应拒绝恢复，实际：{:?}",
        v.fatal
    );
    assert!(
        v.notes.iter().any(|n| n.contains(&deleted_rel)),
        "必须产生可追踪的校验结果，指明哪个素材从一开始就没进备份，实际：{:?}",
        v.notes
    );
}

#[test]
fn local_image_content_mismatch_is_surfaced_not_silently_accepted() {
    // 本机已有同名文件但内容不同（备份来自另一台机器）时，
    // 必须明确暴露，而不是按文件名判断「已存在」就算通过。
    let dir = temp_dir("mf_local_mismatch");
    let app = dir.join("app");
    let (snap, m) = {
        let pool = db::init_db(app.clone()).unwrap();
        let conn = pool.get().unwrap();
        let nid = db::novel::create_novel(&conn, "配图作品", "").unwrap().id;
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", png).unwrap();
        let snap = dir.join("backup.db");
        backup::snapshot_to_file(&pool, &snap).unwrap();
        let m = manifest::build_manifest(&conn, "0.3.8", 1, "").unwrap();
        drop(pool);
        (snap, m)
    };

    // 先确认内容一致时没有任何提示
    let clean = manifest::verify_manifest(&m, &snap, None, None, Some(&app)).unwrap();
    assert!(
        clean.notes.iter().all(|n| !n.contains("内容与清单哈希不符")),
        "内容一致时不应产生哈希不符提示，实际：{:?}",
        clean.notes
    );

    // 把本地文件换成同样长度的不同内容
    let rel = &m.assets[0].rel_path;
    let original = fs::read(app.join(rel)).unwrap();
    let mut tampered = original.clone();
    tampered[0] ^= 0xFF;
    fs::write(app.join(rel), &tampered).unwrap();

    let v = manifest::verify_manifest(&m, &snap, None, None, Some(&app)).unwrap();
    assert!(
        v.notes.iter().any(|n| n.contains("内容与清单哈希不符")),
        "本机文件内容与清单哈希不符必须被明确指出，实际：{:?}",
        v.notes
    );
}

