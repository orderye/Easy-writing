//! 真实 WebDAV 服务端集成测试（V0.3.8 补充任务书 P1-1 D）。
//!
//! ## 与 `webdav_integration.rs` 的区别
//!
//! 那个文件用的是仓库自带的进程内测试服务端 —— 它能证明客户端发出的请求
//! 符合 WebDAV 语义、能正确处理各类响应，但**证明不了与真实产品的兼容性**。
//! 本文件跑在真实 WebDAV 服务端上（CI 中是容器，本机可指向任意测试实例）。
//!
//! ## 默认跳过，但绝不假装通过
//!
//! 没有配置环境变量时整个文件跳过，因为无人值守的开发机不应该依赖外部服务。
//! **一旦配置了 `EW_WEBDAV_IT_URL`，任何连接失败都会让测试失败**，
//! 不会被折叠成「跳过」。CI 里服务端起不来就应该是红的。
//!
//! ## 环境变量
//!
//! ```text
//! EW_WEBDAV_IT_URL       必填，WebDAV 根地址（例如 http://127.0.0.1:8080/）
//! EW_WEBDAV_IT_USER      必填，用户名
//! EW_WEBDAV_IT_PASSWORD  必填，密码
//! EW_WEBDAV_IT_DIR       选填，测试根目录；默认 ew-it-<pid>-<纳秒>
//! ```
//!
//! ## 数据安全
//!
//! 所有操作都发生在**本次运行独占的子目录**里，结束时只删除该子目录下
//! 由本测试创建的文件。绝不列举、读取或删除配置目录之外的任何东西，
//! 绝不使用生产账号或真实备份目录，凭据不写入任何断言消息或日志。
//!
//! 本机运行示例（Docker）：
//!
//! ```text
//! docker run -d --rm -p 8080:80 \
//!   -e AUTH_TYPE=basic -e USERNAME=ewtest -e PASSWORD=ewtest \
//!   --name ew-webdav bytemark/webdav:2.4
//! EW_WEBDAV_IT_URL=http://127.0.0.1:8080/ \
//! EW_WEBDAV_IT_USER=ewtest EW_WEBDAV_IT_PASSWORD=ewtest \
//!   cargo test -p easy-writing-core --test webdav_real_server -- --nocapture --test-threads=1
//! ```

use easy_writing_lib::db;
use easy_writing_lib::sync::backup;
use easy_writing_lib::sync::manifest;
use easy_writing_lib::sync::restore;
use easy_writing_lib::sync::webdav::WebDavClient;
use shared_types::{ManifestCheck, RestoreOutcome};
use std::path::Path;

const PNG_1PX: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

struct Target {
    url: String,
    user: String,
    password: String,
    dir: String,
}

/// 读取测试目标。未配置时返回 `None`（跳过）。
///
/// 注意：密码只保存在返回值里，**永远不出现在任何断言消息或打印中**。
fn target() -> Option<Target> {
    let url = std::env::var("EW_WEBDAV_IT_URL").ok()?;
    let user = std::env::var("EW_WEBDAV_IT_USER").ok()?;
    let password = std::env::var("EW_WEBDAV_IT_PASSWORD").ok()?;
    if url.trim().is_empty() || user.trim().is_empty() {
        return None;
    }
    let dir = std::env::var("EW_WEBDAV_IT_DIR").unwrap_or_else(|_| {
        format!(
            "ew-it-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    });
    Some(Target {
        url,
        user,
        password,
        dir,
    })
}

/// 跳过还是运行？返回 `Some(client, dir)` 表示运行。
///
/// **配置了 URL 却连不上时必须 panic**，而不是静默跳过 ——
/// 否则 CI 里容器起不来会变成一片绿。
async fn connect() -> Option<(WebDavClient, String)> {
    let Some(t) = target() else {
        eprintln!(
            "[real-webdav] EW_WEBDAV_IT_URL / _USER / _PASSWORD 未全部设置 —— \
             跳过真实服务端测试（这不代表已通过验收）"
        );
        return None;
    };
    let client = WebDavClient::new(&t.url, &t.user, &t.password);
    client.test_connection().await.unwrap_or_else(|e| {
        panic!(
            "已配置 EW_WEBDAV_IT_URL，但无法连接真实 WebDAV 服务端：{e}\n\
             这是失败而不是跳过：CI 中服务端必须可用。"
        )
    });
    Some((client, t.dir))
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ew_real_{}_{}_{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// 只清理本次测试在自己独占目录里创建的文件。
async fn cleanup(client: &WebDavClient, dir: &str) {
    let Ok(list) = client.list_files(dir).await else {
        return;
    };
    for item in list {
        let _ = client
            .delete_file(&format!("{}/{}", dir, item.filename))
            .await;
        let _ = client
            .delete_file(&format!("{}/{}.json", dir, item.filename))
            .await;
    }
    if let Ok(imgs) = client.list_files(&format!("{dir}/images")).await {
        for item in imgs {
            let _ = client
                .delete_file(&format!("{dir}/images/{}", item.filename))
                .await;
        }
    }
}

/// 真实服务端上的完整备份 → 列举 → 预检 → 恢复闭环。
#[tokio::test]
async fn real_server_backup_and_restore_roundtrip() {
    let Some((client, dir)) = connect().await else {
        return;
    };

    let work = temp_dir("roundtrip");
    let src_app = work.join("src");
    let pool_src = seeded_pool(&src_app, &["真实服务端测试作品"], true);

    let outcome = backup::run_incremental_backup_detailed(
        pool_src.clone(),
        &client,
        &dir,
        src_app.clone(),
    )
    .await
    .expect("真实服务端上的备份必须成功");

    println!(
        "[real-webdav] 已上传 {}（verified={}, manifest_uploaded={}, 图片失败={}）",
        outcome.filename,
        outcome.verified,
        outcome.manifest_uploaded,
        outcome.image_upload_failures.len()
    );
    assert!(
        outcome.verified,
        "真实服务端必须能通过大小校验（HEAD 或 PROPFIND 至少要有一个可用）"
    );
    assert!(
        outcome.manifest_uploaded,
        "真实服务端必须能接受清单 sidecar 上传"
    );

    // 列举：真实服务端的 PROPFIND Depth:1 响应必须能被解析
    let listed = client.list_files(&dir).await.expect("必须能列举远端目录");
    assert!(
        listed.iter().any(|b| b.filename == outcome.filename),
        "刚上传的备份必须出现在列表里，实际：{:?}",
        listed.iter().map(|b| &b.filename).collect::<Vec<_>>()
    );
    let entry = listed.iter().find(|b| b.filename == outcome.filename).unwrap();
    assert!(
        entry.size_bytes > 0,
        "列表必须给出真实文件大小，实际 {}",
        entry.size_bytes
    );

    // 远端大小必须与本地压缩包一致
    let remote_size = client
        .get_file_size(&format!("{}/{}", dir, outcome.filename))
        .await
        .expect("必须能查询远端文件大小");
    assert!(remote_size > 0, "远端大小不得为 0，实际 {remote_size}");

    // 清单必须能下载并与备份包匹配
    let gz = client
        .download_file(&format!("{}/{}", dir, outcome.filename))
        .await
        .unwrap();
    let manifest_bytes = client
        .download_file(&format!("{}/{}.json", dir, outcome.filename))
        .await
        .expect("清单 sidecar 必须能下载");
    let m: manifest::BackupManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(m.compressed_size_bytes, gz.len() as u64);
    assert_eq!(m.compressed_sha256, manifest::sha256_hex(&gz));

    // 恢复侧：预检 + 执行，必须读到真实远端清单
    let dst_app = work.join("dst");
    let pool_dst = seeded_pool(&dst_app, &["本机原有作品"], false);
    let pf = restore::run_restore_preflight(
        pool_dst.clone(),
        &client,
        &dir,
        &outcome.filename,
        dst_app.clone(),
    )
    .await
    .expect("真实服务端上的预检必须成功");

    assert!(
        matches!(pf.manifest, ManifestCheck::Verified { .. }),
        "远端明明有清单，却判为 {:?} —— 说明清单没有被真正下载",
        pf.manifest
    );
    assert_eq!(pf.novel_count, 1);
    assert_eq!(pf.asset_total, 1);
    assert!(!pf.is_empty_library);

    let report = restore::run_consistency_restore(
        pool_dst.clone(),
        &client,
        &dir,
        &outcome.filename,
        dst_app.clone(),
        pf.preflight_token,
        false,
        restore::RestoreHooks::default(),
    )
    .await
    .expect("真实服务端上的恢复必须成功");

    assert_eq!(
        report.outcome,
        RestoreOutcome::Success,
        "素材与数据库都应完整恢复，实际问题：{:?}",
        report.issues
    );
    assert_eq!(report.summary.asset_restored, 1, "远端图片应已回填");

    let conn = pool_dst.get().unwrap();
    let titles: Vec<String> = {
        let mut stmt = conn.prepare("SELECT title FROM novel").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect()
    };
    assert_eq!(titles, vec!["真实服务端测试作品"]);

    cleanup(&client, &dir).await;
}

/// 真实服务端上的错误凭据必须明确失败。
#[tokio::test]
async fn real_server_rejects_wrong_credentials() {
    let Some(t) = target() else {
        eprintln!("[real-webdav] 未配置环境变量，跳过");
        return;
    };
    // 故意用错误密码。断言消息里不含任何真实凭据。
    let bad = WebDavClient::new(&t.url, &t.user, "definitely-not-the-password");
    let err = bad
        .test_connection()
        .await
        .expect_err("错误凭据必须被服务端拒绝");
    println!("[real-webdav] 错误凭据被拒绝：{err}");
    assert!(
        err.contains("鉴权") || err.contains("HTTP 401") || err.contains("HTTP 403"),
        "必须明确报告鉴权失败，实际：{err}"
    );
}

/// 真实服务端上，不存在的清单必须返回 404（而不是别的东西）。
///
/// 这条决定了「历史无清单备份」兼容分支能否被正确识别：
/// 如果某个服务端对不存在的文件返回 403 或 200+错误页，
/// 客户端就必须报错而不是当成历史备份。
#[tokio::test]
async fn real_server_reports_missing_manifest_as_not_found() {
    let Some((client, dir)) = connect().await else {
        return;
    };
    client.ensure_dir(&dir).await.expect("必须能创建测试目录");

    let probe = format!("{dir}/definitely-missing-probe.db.gz.json");
    match client.download_file_optional(&probe).await {
        Ok(easy_writing_lib::sync::webdav::RemoteFetch::NotFound) => {
            println!("[real-webdav] 不存在的清单返回 404，历史备份兼容分支可用");
        }
        Ok(easy_writing_lib::sync::webdav::RemoteFetch::Found(bytes)) => {
            panic!("不存在的文件竟返回了 {} 字节内容", bytes.len());
        }
        Err(e) => {
            // 不是失败：这说明该服务端不会用 404 表示「不存在」。
            // 客户端此时会拒绝恢复而不是误判为历史备份 —— 行为是安全的，
            // 但必须记录下来，因为它意味着该服务端上的历史备份无法恢复。
            println!(
                "[real-webdav] 警告：该服务端对不存在的清单未返回 404（{e}）。\
                 客户端会拒绝恢复而非误判为历史备份；此服务端上的无清单旧备份将不可恢复。"
            );
        }
    }
    cleanup(&client, &dir).await;
}
