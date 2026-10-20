//! 真实 HTTP/WebDAV 集成测试（V0.3.8 补充任务书 P1-1）。
//!
//! ## 这个文件补的是什么
//!
//! 在此之前的备份/恢复测试全部是函数级的：`plan_rotation`、`preflight_backup`、
//! `validate_backup_filename` 都能在不联网的情况下验证。但 `WebDavClient`
//! 的行为**只体现在真实 HTTP 报文上** —— PROPFIND 的 Depth 头、MOVE 的
//! Destination 头、HEAD 的 Content-Length、404 与 5xx 的区别。
//! 这些从来没有被验证过，而恢复流程的数据安全恰恰依赖它们。
//!
//! 这里用 `tests/common/webdav_server.rs` 起一个真实的 `TcpListener`，
//! 按 HTTP/1.1 解析请求、按 WebDAV 语义应答。`WebDavClient` 走真实 socket，
//! 完全不知道自己在被测试 —— 不是 trait mock。
//!
//! ## 覆盖范围与边界（必须如实区分）
//!
//! 本文件覆盖任务书 P1-1 表格中的全部场景，但服务端是**测试专用实现**，
//! 只能证明「客户端发出的请求符合 WebDAV 语义、且能正确处理各类响应」，
//! **不能证明与 Nextcloud / 坚果云等真实服务的兼容性**。
//! 后者由 `tests/webdav_real_server.rs`（env 门控）与 CI 中的容器 job 负责。
//!
//! 「远端图片缺失 → 恢复结果明确报告」这一场景在
//! `tests/restore_safety.rs::missing_remote_image_downgrades_to_partial_success`
//! 中用同一个服务端覆盖，此处不重复。
//!
//! ## 数据安全
//!
//! 服务端只监听 `127.0.0.1` 的随机端口，数据全部在内存中，进程退出即消失。
//! 凭据是假的（`test-user` / `test-password`），不会出现在任何日志里。
//! **不连接任何真实 WebDAV 账号，不触碰用户真实备份目录与真实数据库。**

mod common;

use common::webdav_server::{Faults, TestWebDav};
use easy_writing_lib::db;
use easy_writing_lib::sync::backup;
use easy_writing_lib::sync::manifest;
use easy_writing_lib::sync::restore;
use easy_writing_lib::sync::webdav::{RemoteFetch, WebDavClient};
use shared_types::ManifestCheck;
use std::fs;
use std::path::Path;

const REMOTE_DIR: &str = "ew-it";
const PNG_1PX: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn client(server: &TestWebDav) -> WebDavClient {
    WebDavClient::new(&server.base_url(), "test-user", "test-password")
}

fn remote(server: &TestWebDav, path: &str) -> String {
    server.path_for(&format!("{REMOTE_DIR}/{path}"))
}

fn seeded_pool(app: &Path, titles: &[&str], with_image: bool) -> db::DbPool {
    let pool = db::init_db(app.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    for title in titles {
        let n = db::novel::create_novel(&conn, title, "").unwrap();
        let v = db::volume::create_volume(&conn, &n.id, "第一卷", "").unwrap();
        db::chapter::create_chapter(&conn, &n.id, &v.id, "第一章", "").unwrap();
    }
    if with_image {
        let nid = db::novel::list_novels(&conn).unwrap()[0].id.clone();
        easy_writing_lib::asset::save_image(&conn, app, &nid, "封面", "image/png", PNG_1PX)
            .unwrap();
    }
    drop(conn);
    pool
}

async fn do_backup(server: &TestWebDav, app: &Path, pool: &db::DbPool) -> backup::BackupOutcome {
    backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(server),
        REMOTE_DIR,
        app.to_path_buf(),
    )
    .await
    .expect("测试备份上传应成功")
}

/// 远端目录下所有以 `backup_` 开头的**备份包**文件名（不含清单 sidecar）。
fn managed_backups(server: &TestWebDav) -> Vec<String> {
    server
        .file_names_in(&server.path_for(REMOTE_DIR))
        .into_iter()
        .filter(|n| n.starts_with("backup_") && n.ends_with(".db.gz"))
        .collect()
}

/// 远端目录下残留的清单 sidecar 文件名。
fn remote_manifests(server: &TestWebDav) -> Vec<String> {
    server
        .file_names_in(&server.path_for(REMOTE_DIR))
        .into_iter()
        .filter(|n| n.ends_with(".db.gz.json"))
        .collect()
}

// ===========================================================================
// 上传 / 下载 / 清单
// ===========================================================================

#[tokio::test]
async fn uploaded_backup_is_downloadable_and_passes_preflight() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_roundtrip");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["集成测试作品"], false);

    let outcome = do_backup(&server, &app, &pool).await;
    assert!(outcome.verified, "备份必须通过大小校验与结构自检");

    // 远端内容必须能被重新下载，并且下载回来的东西确实是可用数据库
    let bytes = server
        .get_file(&remote(&server, &outcome.filename))
        .expect("备份包应存在于远端");
    assert!(!bytes.is_empty());

    let decoded = dir.join("downloaded.db");
    let mut gz = flate2::read::GzDecoder::new(&bytes[..]);
    fs::write(&decoded, {
        use std::io::Read;
        let mut out = Vec::new();
        gz.read_to_end(&mut out).unwrap();
        out
    })
    .unwrap();

    let report = backup::preflight_backup(&decoded).expect("下载回来的备份必须通过预检");
    assert!(report.is_valid);
    assert_eq!(report.novel_count, 1);
    assert_eq!(report.schema_version, db::BASELINE_VERSION);

    assert!(server.hits("PUT") > 0, "必须真的走过 HTTP PUT");
}

#[tokio::test]
async fn uploaded_manifest_matches_uploaded_backup() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_manifest");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["清单匹配作品"], true);

    let outcome = do_backup(&server, &app, &pool).await;
    assert!(outcome.manifest_uploaded, "清单必须上传成功");

    let gz = server
        .get_file(&remote(&server, &outcome.filename))
        .expect("备份包应在远端");
    let manifest_bytes = server
        .get_file(&remote(&server, &format!("{}.json", outcome.filename)))
        .expect("清单 sidecar 必须与备份同名相邻（<backup>.db.gz.json）");

    let m: manifest::BackupManifest =
        serde_json::from_slice(&manifest_bytes).expect("清单必须是合法 JSON");

    // 清单必须真的描述这一份备份包
    assert_eq!(
        m.compressed_size_bytes,
        gz.len() as u64,
        "清单声明的压缩大小必须等于远端实际字节数"
    );
    assert_eq!(
        m.compressed_sha256,
        manifest::sha256_hex(&gz),
        "清单必须通过哈希绑定到这一份备份包"
    );
    assert_eq!(m.schema_version, db::BASELINE_VERSION);
    assert_eq!(m.entity_counts.get("novel").copied(), Some(1));
    assert_eq!(m.assets.len(), 1, "带图备份的清单必须记录该资产");
    assert_eq!(m.manifest_version, manifest::MANIFEST_VERSION);

    // 凭据护栏：清单会上传到用户的 WebDAV，绝不能带秘密
    let text = String::from_utf8_lossy(&manifest_bytes);
    for forbidden in ["test-password", "test-user", "pwd_ref", "api_key"] {
        assert!(
            !text.contains(forbidden),
            "清单中不得出现 {forbidden}，实际：{text}"
        );
    }
}

#[tokio::test]
async fn restore_side_reads_and_verifies_remote_manifest() {
    // 这是 P0-1 的端到端证据：恢复侧**确实**下载了远端 .db.gz.json 并做了校验。
    // 旧实现读的是解压临时库旁边的 sidecar（永远不存在），
    // 于是每次恢复都误报「历史无清单备份」。
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_restore_manifest");
    let src = dir.join("src");
    let pool_src = seeded_pool(&src, &["云端作品"], false);
    let outcome = do_backup(&server, &src, &pool_src).await;

    let app = dir.join("app");
    let pool = seeded_pool(&app, &["本机作品"], false);

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &outcome.filename,
        app.clone(),
    )
    .await
    .expect("预检应成功");

    assert!(
        server.hits("GET") >= 2,
        "必须至少发起两次 GET（备份包 + 清单），实际 {}",
        server.hits("GET")
    );
    match pf.manifest {
        ManifestCheck::Absent => panic!("远端明明有清单，却被判为「历史无清单备份」——这正是 P0-1 的缺陷"),
        ManifestCheck::Verified { asset_count } => assert_eq!(asset_count, 0),
    }
    assert!(pf.asset_problems.is_empty(), "实际：{:?}", pf.asset_problems);
}

#[tokio::test]
async fn remote_move_promotes_tmp_to_final_backup() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_move_ok");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["MOVE 测试"], false);

    let outcome = do_backup(&server, &app, &pool).await;

    assert!(
        server.hits("MOVE") >= 1,
        "备份必须经过 MOVE 从 .tmp 转正，实际 MOVE 次数 {}",
        server.hits("MOVE")
    );
    let names = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert!(
        names.contains(&outcome.filename),
        "正式备份文件应存在，实际：{names:?}"
    );
    assert!(
        !names.iter().any(|n| n.ends_with(".tmp")),
        "转正之后不应残留 .tmp 文件，实际：{names:?}"
    );
}

#[tokio::test]
async fn remote_move_failure_is_reported_and_leaves_no_backup() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_move_fail");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["MOVE 失败"], false);
    server.set_faults(Faults {
        fail_move: true,
        ..Default::default()
    });

    let err = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect_err("MOVE 失败必须让备份失败");

    assert!(
        err.contains("重命名快照备份失败"),
        "失败必须可见，实际：{err}"
    );
    let names = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert!(
        !names.iter().any(|n| n.ends_with(".db.gz")),
        "MOVE 失败时不得留下被当成完整备份的文件，实际：{names:?}"
    );
    assert!(
        !names.iter().any(|n| n.ends_with(".tmp")),
        "失败的临时文件应被清理，实际：{names:?}"
    );
}

#[tokio::test]
async fn interrupted_upload_leaves_no_usable_backup() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_upload_abort");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["上传中断"], false);
    server.set_faults(Faults {
        abort_during_put: true,
        ..Default::default()
    });

    let err = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect_err("上传中断必须让备份失败");

    assert!(!err.is_empty(), "必须给出失败原因，实际：{err}");
    let names = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert!(
        !names.iter().any(|n| n.ends_with(".db.gz")),
        "上传中断绝不能留下被当成完整备份的半成品，实际：{names:?}"
    );
}

#[tokio::test]
async fn truncated_download_is_rejected_by_preflight() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_truncated");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["截断测试"], false);
    let outcome = do_backup(&server, &app, &pool).await;

    let full = server.get_file(&remote(&server, &outcome.filename)).unwrap();
    server.set_faults(Faults {
        truncate_get_to: Some(full.len() / 2),
        ..Default::default()
    });

    let target = dir.join("target");
    let target_pool = seeded_pool(&target, &["本机作品"], false);
    let before = novel_count(&target_pool);

    let err = restore::run_restore_preflight(
        target_pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &outcome.filename,
        target.clone(),
    )
    .await
    .expect_err("截断的备份必须在恢复前被拒绝");

    assert!(
        err.contains("原数据未被改动"),
        "必须明确告知作者现有数据是安全的，实际：{err}"
    );
    assert_eq!(novel_count(&target_pool), before, "本地库不得被改动");
}

fn novel_count(pool: &db::DbPool) -> i64 {
    let conn = pool.get().unwrap();
    conn.query_row("SELECT COUNT(*) FROM novel", [], |r| r.get(0))
        .unwrap()
}

// ===========================================================================
// 大小校验：不能把「没校验」当成「校验通过」
// ===========================================================================

#[tokio::test]
async fn size_mismatch_blocks_backup_and_cleans_tmp() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_size_mismatch");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["大小校验"], false);
    server.set_faults(Faults {
        head_content_length: Some(999_999),
        ..Default::default()
    });

    let err = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect_err("远端大小与本地不一致时必须失败");

    assert!(
        err.contains("上传校验失败"),
        "必须说明是大小校验失败，实际：{err}"
    );
    let names = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert!(
        names.is_empty(),
        "校验失败的备份不得留在远端被当成可用备份，实际：{names:?}"
    );
}

#[tokio::test]
async fn head_refusal_falls_back_to_propfind_and_still_verifies() {
    // 部分反向代理会拦掉 HEAD。客户端必须回退到 PROPFIND，
    // 只要还能拿到大小的真实值，就应该正常完成校验。
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_no_head");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["无 HEAD"], false);
    server.set_faults(Faults {
        refuse_head: true,
        ..Default::default()
    });

    let outcome = do_backup(&server, &app, &pool).await;

    assert!(server.hits("HEAD") >= 1, "应先尝试 HEAD");
    assert!(
        outcome.verified,
        "PROPFIND 能给出真实大小时，校验应当成功（走回退路径）"
    );
}

#[tokio::test]
async fn no_size_source_at_all_is_never_reported_as_verified() {
    // 服务端既不支持 HEAD、PROPFIND 也不返回 getcontentlength。
    // 此时客户端**无法确认**上传结果 —— 必须失败，
    // 而不是像旧实现那样用 `if let Ok(..)` 把「跳过校验」当成「校验通过」。
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_no_size");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["无法确认大小"], false);
    server.set_faults(Faults {
        refuse_head: true,
        propfind_omit_content_length: true,
        ..Default::default()
    });

    let err = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect_err("无法确认远端大小时必须放弃本次备份");

    assert!(
        err.contains("无法确认远端上传结果"),
        "必须明确说明是「无法确认」，实际：{err}"
    );
    let names = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert!(
        !names.iter().any(|n| n.ends_with(".db.gz")),
        "未经验证的备份不得转正，实际：{names:?}"
    );
}

#[tokio::test]
async fn get_file_size_reports_real_length_over_http() {
    let server = TestWebDav::start();
    let c = client(&server);
    c.upload_file("ew-it/probe.bin", vec![7u8; 1234])
        .await
        .unwrap();

    let size = c.get_file_size("ew-it/probe.bin").await.unwrap();
    assert_eq!(size, 1234, "HEAD 必须报告真实字节数");

    server.set_faults(Faults {
        refuse_head: true,
        ..Default::default()
    });
    let size2 = c.get_file_size("ew-it/probe.bin").await.unwrap();
    assert_eq!(size2, 1234, "HEAD 不可用时 PROPFIND 回退必须给出同一个值");
}

// ===========================================================================
// 清单 / 图片上传失败必须对调用方可见
// ===========================================================================

#[tokio::test]
async fn manifest_upload_failure_is_visible_to_caller() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_manifest_fail");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["清单上传失败"], false);
    server.set_faults(Faults {
        fail_put_suffix: Some(".json".to_string()),
        ..Default::default()
    });

    let outcome = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect("清单上传失败不应毁掉数据库备份本体");

    assert!(
        !outcome.manifest_uploaded,
        "调用方必须能得知清单缺失，而不是一次静默的成功"
    );
    assert!(outcome.verified, "数据库备份本体仍然有效");
    assert!(
        server
            .get_file(&remote(&server, &format!("{}.json", outcome.filename)))
            .is_none(),
        "清单确实没有落到远端"
    );

    // 后果必须体现在恢复侧：这份备份只能按历史无清单路径恢复
    let target = dir.join("target");
    let target_pool = seeded_pool(&target, &["本机作品"], false);
    let pf = restore::run_restore_preflight(
        target_pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &outcome.filename,
        target.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        pf.manifest,
        ManifestCheck::Absent,
        "没有清单的备份必须被明确标记为历史兼容路径"
    );
}

#[tokio::test]
async fn image_upload_failure_is_visible_to_caller() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_image_fail");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["图片上传失败"], true);
    server.set_faults(Faults {
        fail_put_suffix: Some(".png".to_string()),
        ..Default::default()
    });

    let outcome = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect("单张图上传失败不应毁掉整次备份");

    assert_eq!(
        outcome.image_upload_failures.len(),
        1,
        "未同步的图片必须逐项列出，实际：{:?}",
        outcome.image_upload_failures
    );
    assert!(outcome.verified, "数据库备份本体不受影响");
}

#[tokio::test]
async fn asset_missing_on_disk_is_reported_by_backup() {
    // 数据库里有记录、但本地文件已经丢失：这类素材根本没进备份，
    // 必须当场告诉作者，而不是等恢复时才发现图没了。
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_asset_missing");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["缺图作品"], true);

    let rel: String = {
        let conn = pool.get().unwrap();
        conn.query_row("SELECT rel_path FROM asset LIMIT 1", [], |r| r.get(0))
            .unwrap()
    };
    fs::remove_file(app.join(&rel)).unwrap();

    let outcome = do_backup(&server, &app, &pool).await;

    assert_eq!(
        outcome.missing_assets,
        vec![rel.clone()],
        "备份时必须报告本地已缺失的素材，实际：{:?}",
        outcome.missing_assets
    );

    // 清单里也必须留痕，恢复时才能追溯
    let manifest_bytes = server
        .get_file(&remote(&server, &format!("{}.json", outcome.filename)))
        .unwrap();
    let m: manifest::BackupManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(m.missing_assets, vec![rel]);
    assert!(
        m.assets.is_empty(),
        "缺文件的资产不应出现在 assets 里（清单只描述真实存在的东西）"
    );
}

// ===========================================================================
// 轮转
// ===========================================================================

#[tokio::test]
async fn rotation_only_deletes_managed_backups_over_real_http() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_rotation");
    let app = dir.join("app");

    // 预置 25 份旧备份（每份都带清单 sidecar）+ 1 个用户自己放进来的文件。
    // KEEP = 20，因此本次备份后应删除最旧的 6 份，且绝不能碰那个外来文件。
    for i in 1..=25 {
        let name = format!("backup_2025{:02}01_000000Z.db.gz", i);
        server.put_file(&remote(&server, &name), format!("old backup {i}").into_bytes());
        server.put_file(
            &remote(&server, &format!("{name}.json")),
            br#"{"manifest_version":2}"#.to_vec(),
        );
    }
    let foreign = "someone-elses-important-file.db.gz";
    server.put_file(&remote(&server, foreign), b"not ours".to_vec());
    assert_eq!(managed_backups(&server).len(), 25);
    assert_eq!(remote_manifests(&server).len(), 25);

    let pool = seeded_pool(&app, &["轮转测试"], false);
    let outcome = do_backup(&server, &app, &pool).await;
    assert!(outcome.verified, "只有已验证的新备份才允许触发轮转");

    let remaining = managed_backups(&server);
    assert_eq!(
        remaining.len(),
        20,
        "轮转后应恰好保留 20 份受管备份，实际 {} 份：{:?}",
        remaining.len(),
        remaining
    );
    assert!(
        server
            .get_file(&remote(&server, foreign))
            .is_some(),
        "绝不能删除用户手动放进远端目录的非本应用文件"
    );
    assert!(
        remaining.contains(&outcome.filename),
        "刚上传的备份必须保留"
    );
    assert!(
        !remaining.contains(&"backup_20250101_000000Z.db.gz".to_string()),
        "最旧的备份应被删除"
    );
    assert!(server.hits("DELETE") >= 6, "必须真的发起过 DELETE 请求");

    // 被删备份的清单必须一起删掉。
    // 少了这一步，每次轮转都会在用户的 WebDAV 上留下一份孤儿 .json，
    // 而 list_files 永远看不到它，因此不会有任何后续机制去清理。
    let manifests = remote_manifests(&server);
    assert_eq!(
        manifests.len(),
        20,
        "每份保留的备份对应一份清单，不得留下孤儿清单，实际：{manifests:?}"
    );
    for name in &remaining {
        assert!(
            manifests.contains(&format!("{name}.json")),
            "保留的备份 {name} 应仍有配套清单"
        );
    }
    assert!(
        !manifests.contains(&"backup_20250101_000000Z.db.gz.json".to_string()),
        "已删除备份的清单必须一并删除"
    );
}

#[tokio::test]
async fn rotation_failure_is_surfaced_not_swallowed() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_rotation_fail");
    let app = dir.join("app");

    for i in 1..=25 {
        server.put_file(
            &remote(&server, &format!("backup_2025{:02}01_000000Z.db.gz", i)),
            format!("old backup {i}").into_bytes(),
        );
    }
    server.set_faults(Faults {
        fail_delete: true,
        ..Default::default()
    });

    let pool = seeded_pool(&app, &["轮转失败"], false);
    let err = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect_err("删除失败必须向上冒泡");

    assert!(
        err.contains("删除失败"),
        "必须说明是轮转删除失败（远端会持续增长），实际：{err}"
    );
}

// ===========================================================================
// 历史无清单备份
// ===========================================================================

#[tokio::test]
async fn legacy_backup_without_manifest_uses_compatibility_path() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_legacy");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["历史备份"], false);
    let outcome = do_backup(&server, &app, &pool).await;

    // 模拟一份「清单从未上传过」的历史备份：远端确实返回 404
    server.remove_file(&remote(&server, &format!("{}.json", outcome.filename)));

    let c = client(&server);
    match c
        .download_file_optional(&format!("{REMOTE_DIR}/{}.json", outcome.filename))
        .await
        .unwrap()
    {
        RemoteFetch::NotFound => {}
        RemoteFetch::Found(_) => panic!("清单已删除，服务端必须返回 404"),
    }

    let target = dir.join("target");
    let target_pool = seeded_pool(&target, &["本机作品"], false);
    let pf = restore::run_restore_preflight(
        target_pool.clone(),
        &c,
        REMOTE_DIR,
        &outcome.filename,
        target.clone(),
    )
    .await
    .expect("历史无清单备份必须仍可恢复");

    assert_eq!(pf.manifest, ManifestCheck::Absent);
    assert!(pf.integrity_ok, "仍须通过 SQLite 完整性检查");
    assert_eq!(pf.schema_version, db::BASELINE_VERSION);

    let report = restore::run_consistency_restore(
        target_pool.clone(),
        &c,
        REMOTE_DIR,
        &outcome.filename,
        target.clone(),
        pf.preflight_token,
        false,
        restore::RestoreHooks::default(),
    )
    .await
    .unwrap();
    assert_eq!(report.outcome, shared_types::RestoreOutcome::Success);
    assert_eq!(report.summary.manifest, ManifestCheck::Absent);
}

// ===========================================================================
// 认证与权限
// ===========================================================================

#[tokio::test]
async fn wrong_credentials_fail_loudly_everywhere() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("it_auth");
    let app = dir.join("app");
    let pool = seeded_pool(&app, &["认证失败"], false);
    server.set_faults(Faults {
        unauthorized: true,
        ..Default::default()
    });

    let c = client(&server);

    let err = c.test_connection().await.expect_err("凭据错误必须失败");
    assert!(
        err.contains("鉴权失败"),
        "必须明确说明是鉴权问题，实际：{err}"
    );

    let err = backup::run_incremental_backup_detailed(
        pool.clone(),
        &c,
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .expect_err("凭据错误时备份必须失败");
    assert!(!err.is_empty(), "实际：{err}");

    let err = c
        .download_file_optional(&format!("{REMOTE_DIR}/anything.db.gz.json"))
        .await
    .expect_err("凭据错误不得被当成「文件不存在」");
    assert!(
        err.contains("不能据此判定为历史备份"),
        "401 绝不能被折叠成 404，否则会把有清单的备份降级成历史备份，实际：{err}"
    );
}

// ===========================================================================
// 目录创建与列举
// ===========================================================================

#[tokio::test]
async fn ensure_dir_creates_nested_remote_directories() {
    let server = TestWebDav::start();
    let c = client(&server);
    c.ensure_dir("ew-it/nested/deep").await.unwrap();
    assert!(server.hits("MKCOL") >= 3, "应逐级创建目录");

    // 重复创建必须幂等（服务端返回 405，客户端应容忍）
    c.ensure_dir("ew-it/nested/deep").await.unwrap();
}

#[tokio::test]
async fn list_files_returns_only_backup_packages_newest_first() {
    let server = TestWebDav::start();
    let c = client(&server);
    for name in [
        "backup_20260101_000000Z.db.gz",
        "backup_20260103_000000Z.db.gz",
        "backup_20260102_000000Z.db.gz",
    ] {
        c.upload_file(&format!("{REMOTE_DIR}/{name}"), vec![1, 2, 3])
            .await
            .unwrap();
    }
    // 清单与图片不应出现在备份列表里
    c.upload_file(&format!("{REMOTE_DIR}/backup_20260103_000000Z.db.gz.json"), b"{}".to_vec())
        .await
        .unwrap();
    c.upload_file(&format!("{REMOTE_DIR}/images/a.png"), vec![9])
        .await
        .unwrap();

    let list = c.list_files(REMOTE_DIR).await.unwrap();
    let names: Vec<String> = list.iter().map(|b| b.filename.clone()).collect();
    assert_eq!(
        names,
        vec![
            "backup_20260103_000000Z.db.gz",
            "backup_20260102_000000Z.db.gz",
            "backup_20260101_000000Z.db.gz",
        ],
        "列表应只含备份包且按时间倒序，实际：{names:?}"
    );
    assert_eq!(list[0].size_bytes, 3, "必须报告真实文件大小");
}

#[tokio::test]
async fn delete_of_missing_file_is_idempotent() {
    let server = TestWebDav::start();
    let c = client(&server);
    // 404 视为已删除：轮转时重复删除不应报错
    c.delete_file(&format!("{REMOTE_DIR}/never-existed.db.gz"))
        .await
        .expect("删除不存在的文件应视为成功");
}
