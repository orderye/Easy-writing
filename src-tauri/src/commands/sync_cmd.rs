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
    run_silent_sync_with(pool, &client, &remote_dir, app_data_dir).await
}

/// 退出应用时的静默同步（已拿到客户端的版本）。
///
/// 单独拆出来是为了能测：静默同步的关键行为 ——「遇到跨设备冲突时
/// 跳过上传、不阻塞关窗、把冲突记下来等下次启动提示」—— 必须在
/// 真实 HTTP 往返之后才成立，而测试无法凭空造出一个 WebDAV 客户端。
pub async fn run_silent_sync_with(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    app_data_dir: std::path::PathBuf,
) -> Result<String, String> {
    // 退出时的静默同步**不能**弹阻塞窗口：那会把「关掉应用」变成
    // 「必须先处理跨设备冲突」。因此冲突时跳过本次上传、把冲突记在
    // 设备状态里，等下次启动时再提示作者。
    let state = crate::sync::device::load(&app_data_dir);
    if let Some(conflict) = detect_remote_conflict(client, remote_dir, &state).await? {
        let mut state = state;
        state.pending_notice = Some(crate::sync::device::pending_notice_from_conflict(&conflict));
        if let Err(e) = crate::sync::device::save(&app_data_dir, &state) {
            eprintln!("[sync] 写入待处理提示失败: {e}");
        }
        return Ok(format!(
            "{}: {}",
            crate::sync::device::CONFLICT_CODE,
            conflict.remote_filename
        ));
    }

    crate::sync::backup::run_incremental_backup(
        pool,
        client,
        remote_dir,
        app_data_dir,
    )
    .await
}

/// 读取远端最新备份的来源身份，判定是否存在跨设备冲突。
///
/// 只看**最新一份备份**：备份名内嵌 UTC 时间戳，字典序即时间序。
/// 远端最新的一份是本机自己写的 → 本机就是最新的，没有冲突；
/// 是别的设备写的 → 比本机 `last_sync_at` 新才算冲突。
///
/// 只取一份而不是逐份扫描，是刻意的：远端可能有几十份备份，
/// 逐份下载 meta 会在每次备份前都产生几十次网络往返。
pub async fn detect_remote_conflict(
    client: &WebDavClient,
    remote_dir: &str,
    state: &crate::sync::device::DeviceState,
) -> Result<Option<shared_types::BackupConflictInfo>, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    let backups = client.list_files(&clean_dir).await?;

    // 最新一份备份（文件名倒序第一个）
    let Some(newest) = backups.iter().map(|b| b.filename.clone()).max() else {
        return Ok(None);
    };

    let meta_path = format!(
        "{}/{}",
        clean_dir,
        crate::sync::device::meta_sidecar_name(&newest)
    );
    let meta = match client.download_file_optional(&meta_path).await {
        Ok(crate::sync::webdav::RemoteFetch::Found(bytes)) => {
            match serde_json::from_slice::<crate::sync::device::BackupMeta>(&bytes) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[sync] 远端来源信息解析失败，按「无来源信息」处理: {e}");
                    return Ok(None);
                }
            }
        }
        // 没有 meta = 历史备份，兼容现状，不算冲突
        Ok(crate::sync::webdav::RemoteFetch::NotFound) => return Ok(None),
        Err(e) => {
            eprintln!("[sync] 读取远端来源信息失败，按「无法判定」处理: {e}");
            return Ok(None);
        }
    };

    Ok(crate::sync::device::detect_conflict(state, Some(&meta), &newest))
}

/// 跨设备冲突预检（只读）。
///
/// 返回 `None` 表示没有冲突。存在冲突时，后端**拒绝**上传，
/// 由作者在「继续上传 / 先恢复 / 取消」之间显式选择。
#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_backup_preflight(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
) -> Result<Option<shared_types::BackupConflictInfo>, String> {
    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let state = crate::sync::device::load(&app_data_dir);
    detect_remote_conflict(&client, &remote_dir, &state).await
}

/// 本机同步状态（设备标识、上次同步时间、待处理提示）。
#[tauri::command(rename_all = "snake_case")]
pub async fn sync_get_device_state(
    app: tauri::AppHandle,
) -> Result<shared_types::DeviceStateInfo, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let state = crate::sync::device::load(&app_data_dir);
    Ok(shared_types::DeviceStateInfo {
        device_id: state.device_id,
        device_name: state.device_name,
        last_sync_at: state.last_sync_at,
        pending_notice: state.pending_notice,
    })
}

/// 清除待处理提示（作者已经处理过冲突）。
#[tauri::command(rename_all = "snake_case")]
pub async fn sync_clear_device_notice(app: tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let mut state = crate::sync::device::load(&app_data_dir);
    state.pending_notice = None;
    crate::sync::device::save(&app_data_dir, &state)
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

/// 执行一次整库备份。
///
/// `override_conflict` 是**跨设备覆盖保护**的闸门：
///
/// - `false`（默认）：检测到「远端存在来自其他设备、且比本机上次同步更新」
///   的备份时，**拒绝上传**并返回带 `CONFLICT:` 前缀的错误；
/// - `true`：作者已经在「继续上传 / 先恢复 / 取消」里显式选择「继续上传」，
///   此时允许覆盖远端。
///
/// 这道闸门放在后端而不是只靠前端弹窗：前端弹窗可以被绕过，
/// 而「用旧库覆盖新库」是本应用能造成的最坏结果之一。
#[tauri::command(rename_all = "snake_case")]
pub async fn webdav_backup_now(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
    override_conflict: bool,
) -> Result<shared_types::BackupReport, String> {
    let (client, remote_dir) = get_webdav_client_and_remote_dir(pool.inner())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());

    if !override_conflict {
        let state = crate::sync::device::load(&app_data_dir);
        if let Some(conflict) = detect_remote_conflict(&client, &remote_dir, &state).await? {
            return Err(crate::sync::device::conflict_error(&conflict));
        }
    }

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
        meta_uploaded: outcome.meta_uploaded,
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
