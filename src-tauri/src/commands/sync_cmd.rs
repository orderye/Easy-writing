use crate::db::DbPool;
use crate::sync::webdav::WebDavClient;
use shared_types::{BackupFileInfo, WebdavConfig};
use tauri::{Manager, State};

fn get_webdav_client_and_remote_dir(
    pool: &DbPool,
) -> Result<(WebDavClient, String), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT id, url, username, pwd_ref, remote_dir FROM webdav_config WHERE id = 'default_webdav'")
        .map_err(|e| e.to_string())?;

    let row = stmt
        .query_row([], |r| {
            Ok(WebdavConfig {
                id: r.get(0)?,
                url: r.get(1)?,
                username: r.get(2)?,
                pwd_ref: r.get(3)?,
                remote_dir: r.get(4)?,
            })
        })
        .map_err(|_| "未找到 WebDAV 配置，请先在系统设置中完成配置".to_string())?;

    let password = crate::keyring::get_secret(&row.pwd_ref)
        .map_err(|_| "未在系统钥匙串中找到 WebDAV 密码凭据".to_string())?;

    let client = WebDavClient::new(&row.url, &row.username, &password);
    Ok((client, row.remote_dir))
}

pub fn is_webdav_configured(pool: &DbPool) -> bool {
    if let Ok(conn) = pool.get() {
        conn.query_row(
            "SELECT 1 FROM webdav_config WHERE id = 'default_webdav' AND length(url) > 0",
            [],
            |_| Ok(()),
        )
        .is_ok()
    } else {
        false
    }
}

pub async fn run_silent_sync_if_configured(
    pool: DbPool,
    app_data_dir: std::path::PathBuf,
) -> Result<String, String> {
    let (client, remote_dir) = get_webdav_client_and_remote_dir(&pool)?;
    crate::sync::backup::run_incremental_backup(
        pool,
        &client,
        &remote_dir,
        app_data_dir,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub fn webdav_save_config(
    pool: State<DbPool>,
    url: String,
    username: String,
    password: Option<String>,
    remote_dir: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let pwd_ref = "webdav_default_pwd";

    if let Some(ref pwd) = password {
        if !pwd.trim().is_empty() {
            crate::keyring::set_secret(pwd_ref, pwd)
                .map_err(|e| format!("保存密码至系统钥匙串失败: {}", e))?;
        }
    }

    conn.execute(
        "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir, updated_at)
         VALUES ('default_webdav', ?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
            url = excluded.url,
            username = excluded.username,
            pwd_ref = excluded.pwd_ref,
            remote_dir = excluded.remote_dir,
            updated_at = datetime('now')",
        rusqlite::params![url, username, pwd_ref, remote_dir],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn webdav_get_config(pool: State<DbPool>) -> Result<Option<WebdavConfig>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT id, url, username, pwd_ref, remote_dir FROM webdav_config WHERE id = 'default_webdav'")
        .map_err(|e| e.to_string())?;

    let config_res = stmt.query_row([], |r| {
        Ok(WebdavConfig {
            id: r.get(0)?,
            url: r.get(1)?,
            username: r.get(2)?,
            pwd_ref: r.get(3)?,
            remote_dir: r.get(4)?,
        })
    });

    match config_res {
        Ok(c) => Ok(Some(c)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_test_connection(pool: State<'_, DbPool>) -> Result<(), String> {
    let (client, _) = get_webdav_client_and_remote_dir(pool.inner())?;
    client.test_connection().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_backup_now(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
) -> Result<shared_types::BackupReport, String> {
    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());

    let outcome = crate::sync::backup::run_incremental_backup_detailed(
        pool.inner().clone(),
        &client,
        &remote_dir,
        app_data_dir,
    )
    .await?;

    // 完整性缺口必须越过 IPC 边界：旧实现只回传 filename，
    // 「清单没传上去 / 图片没同步 / 素材本地就缺」全部被吞在后端。
    Ok(shared_types::BackupReport {
        filename: outcome.filename,
        verified: outcome.verified,
        manifest_uploaded: outcome.manifest_uploaded,
        image_upload_failures: outcome.image_upload_failures,
        missing_assets: outcome.missing_assets,
        rotated_deleted: outcome.rotated_deleted,
        retention: outcome.retention,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_list_backups(
    pool: State<'_, DbPool>,
) -> Result<Vec<BackupFileInfo>, String> {
    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    crate::sync::restore::list_remote_backups(&client, &remote_dir).await
}

/// 保留策略 dry-run 预览。
///
/// **只读**：只列出「如果现在执行清理会删掉哪些」，不删除任何远端文件。
/// 保留策略是本应用唯一会主动删除远端数据的机制，让作者先看后删。
#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_preview_retention(
    pool: State<'_, DbPool>,
) -> Result<shared_types::RetentionPreview, String> {
    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    let policy = read_retention_policy(pool.inner());
    crate::sync::backup::preview_retention(&client, &remote_dir, policy).await
}

/// 读取当前保留策略。读取失败时回落到默认值，绝不让「读设置失败」变成
/// 「按空白策略清理远端」。
fn read_retention_policy(pool: &DbPool) -> shared_types::RetentionPolicy {
    match pool.get() {
        Ok(conn) => crate::sync::retention::policy_from_settings(&conn),
        Err(e) => {
            eprintln!("[sync] 读取保留策略失败，改用默认策略: {e}");
            shared_types::RetentionPolicy::default()
        }
    }
}

/// 恢复预检：下载并校验候选备份，返回覆盖数据库之前作者需要看到的全部事实。
///
/// **本命令不改动现有数据库**，因此可以被前端安全地反复调用。
/// 返回的 `preflight_token` 是候选备份的 SHA-256，
/// [`webdav_restore_backup`] 凭它找回同一份候选并重新校验。
#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_restore_preflight(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
    filename: String,
) -> Result<crate::sync::restore::RestorePreflight, String> {
    crate::sync::backup::validate_backup_filename(&filename)?;

    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());

    crate::sync::restore::run_restore_preflight(
        pool.inner().clone(),
        &client,
        &remote_dir,
        &filename,
        app_data_dir,
    )
    .await
}

/// 执行恢复。必须先成功调用过 [`webdav_restore_preflight`]。
///
/// `confirm_empty_overwrite` 是空库覆盖闸门：本机有作品而候选备份为空时，
/// 未显式确认则拒绝执行，且**不产生任何数据库、配置或素材变更**。
#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_restore_backup(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
    filename: String,
    preflight_token: String,
    confirm_empty_overwrite: bool,
) -> Result<crate::sync::restore::RestoreReport, String> {
    // 在最靠近入口处挡住路径穿越：filename 会直接拼进 WebDAV URL，
    // 而 build_url 只做字符串拼接、不做规范化。
    crate::sync::backup::validate_backup_filename(&filename)?;

    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());

    crate::sync::restore::run_consistency_restore(
        pool.inner().clone(),
        &client,
        &remote_dir,
        &filename,
        app_data_dir,
        preflight_token,
        confirm_empty_overwrite,
        crate::sync::restore::RestoreHooks::default(),
    )
    .await
}
