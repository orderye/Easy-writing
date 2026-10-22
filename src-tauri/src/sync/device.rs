//! 设备身份与跨设备同步状态。
//!
//! ## 为什么必须存在「库外」
//!
//! 恢复流程会用远端备份**整体覆盖**本地数据库（`restore.rs` 走 SQLite
//! Online Backup API 把内容拷进目标连接）。因此任何存在库里的
//! 「我是哪台设备」都会在恢复后变成**来源设备**的身份 ——
//! 跨设备保护随即失效，而且失效得毫无痕迹。
//!
//! 所以 `device_id` 与 `last_sync_at` 落在
//! `<app_data_dir>/sync_state.json`：它在数据库之外，恢复覆盖不到。
//!
//! ## 三个文件角色
//!
//! - `sync_state.json`：本机状态（设备身份、上次同步时间、待处理提示）
//! - `<backup>.db.gz.json`：备份**清单**（内容自描述 + 完整性绑定）
//! - `<backup>.db.gz.meta.json`：备份**来源身份**（哪台设备写的、什么时候、库内最大更新时间）
//!
//! 清单与 meta 刻意分文件：清单回答「这份备份完整吗」，meta 回答
//! 「这份备份是谁写的」。合成一个文件会让「格式升级」与「协议升级」互相绑架。

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use shared_types::{BackupConflictInfo, PendingNotice};
use std::path::{Path, PathBuf};

/// 本机同步状态文件名（位于应用数据目录，数据库之外）。
pub const STATE_FILE_NAME: &str = "sync_state.json";

/// 备份来源身份 sidecar 的后缀。
pub const META_SUFFIX: &str = ".meta.json";

/// 冲突提示的机器可判定前缀。
///
/// 与 `KEY_MISSING` 同一套思路：后半句是给作者看的人话，前缀是给
/// 程序看的判据。冲突时后端**拒绝**上传并返回带此前缀的错误，
/// 前端据此弹出「继续上传 / 先恢复 / 取消」三选一。
pub const CONFLICT_CODE: &str = "CONFLICT";

/// 本机同步状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceState {
    /// 本机设备标识（uuid v4）。首次启动生成，之后永不改变。
    pub device_id: String,
    /// 便于作者辨认的设备名（主机名）。
    pub device_name: String,
    /// 本机内容与远端**对齐**的时间点（RFC3339 UTC）。
    ///
    /// 语义是「本机已经包含了远端截至这一刻的全部内容」。
    /// 冲突判定用它比较：远端备份比它更新，说明远端在本机上次同步之后
    /// 发生过变化。
    pub last_sync_at: Option<String>,
    /// 上次退出时因冲突而跳过的同步提示，下次启动时展示给作者。
    pub pending_notice: Option<PendingNotice>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            device_id: uuid::Uuid::new_v4().to_string(),
            device_name: default_device_name(),
            last_sync_at: None,
            pending_notice: None,
        }
    }
}

/// 一次备份的来源身份（`<backup>.db.gz.meta.json` 的内容）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupMeta {
    /// 写这份备份的设备标识。
    pub device_id: String,
    /// 写这份备份的设备名。
    #[serde(default)]
    pub device_name: String,
    /// 备份创建时间（RFC3339 UTC）。
    pub created_at: String,
    /// 备份时刻库内的最大 `updated_at`。
    #[serde(default)]
    pub max_updated_at: String,
    /// 备份所含数据库的 schema 版本。
    pub schema_version: i64,
    /// 生成该备份的应用版本。
    pub app_version: String,
    /// 压缩包 `.db.gz` 的 SHA-256。
    pub sha256: String,
}

/// 计算「库内最大更新时间」所依据的表。
///
/// 刻意只取**内容表**：作品、卷、章、设定、角色、剧情与角色关系。
/// 配置类表（模型接口、提示词模板、WebDAV 配置）的时间戳与
/// 「作者改了多少内容」无关，混进来只会让这个指标失去意义。
const UPDATED_AT_TABLES: &[&str] = &[
    "novel",
    "volume",
    "chapter",
    "worldview_entry",
    "character",
    "plot_item",
    "character_relation",
];

/// 读取库内最大 `updated_at`。
///
/// 表名来自本文件的常量白名单，不含任何外部输入。
pub fn max_updated_at(conn: &Connection) -> Result<String, String> {
    let mut max = String::new();
    for table in UPDATED_AT_TABLES {
        let value: String = conn
            .query_row(
                &format!("SELECT COALESCE(MAX(updated_at), '') FROM {table}"),
                [],
                |r| r.get(0),
            )
            .map_err(|e| format!("读取 {table} 的最大更新时间失败: {e}"))?;
        if value > max {
            max = value;
        }
    }
    Ok(max)
}

/// 本机状态文件路径。
pub fn state_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(STATE_FILE_NAME)
}

/// 读取本机同步状态。
///
/// 文件不存在或损坏时**重建**一份新的（新的 device_id），并把这件事
/// 如实记进返回值里 —— 静默地换掉设备身份会让跨设备保护形同虚设，
/// 而作者完全不知情。
pub fn load(app_data_dir: &Path) -> DeviceState {
    let path = state_path(app_data_dir);
    let raw = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let state = DeviceState::default();
            let _ = save(app_data_dir, &state);
            return state;
        }
        Err(e) => {
            eprintln!("[device] 读取同步状态失败（{}），将重建设备身份: {e}", path.display());
            return DeviceState::default();
        }
    };

    match serde_json::from_slice::<DeviceState>(&raw) {
        Ok(state) => state,
        Err(e) => {
            eprintln!("[device] 同步状态文件损坏（{}），将重建设备身份: {e}", path.display());
            DeviceState::default()
        }
    }
}

/// 写回本机同步状态（原子写）。
pub fn save(app_data_dir: &Path, state: &DeviceState) -> Result<(), String> {
    let path = state_path(app_data_dir);
    let text = serde_json::to_vec_pretty(state).map_err(|e| format!("序列化同步状态失败: {e}"))?;
    crate::sync::backup::atomic_write(&path, &text)
}

/// 备份来源身份 sidecar 的远端文件名。
pub fn meta_sidecar_name(backup_filename: &str) -> String {
    format!("{backup_filename}{META_SUFFIX}")
}

/// 判断两个 RFC3339 时间戳的先后。
///
/// 优先按时间解析；解析不了时退回字符串比较 —— 同一格式的 RFC3339
/// 字符串，其字典序与时间序一致。两者都不可用时视为「不更新」，
/// 即不报冲突（宁可少报一次冲突，也不要用坏数据把作者挡在门外）。
pub fn is_newer(candidate: &str, reference: &str) -> bool {
    match (
        DateTime::parse_from_rfc3339(candidate),
        DateTime::parse_from_rfc3339(reference),
    ) {
        (Ok(c), Ok(r)) => c > r,
        _ => candidate > reference,
    }
}

/// 判定跨设备冲突。
///
/// 三个条件同时成立才算冲突：
/// 1. 远端确实有 meta（历史备份没有 meta → 兼容现状，不算冲突）；
/// 2. meta 来自**别的设备**；
/// 3. meta 的创建时间**晚于**本机 `last_sync_at`。
///
/// 第 3 条是「远端在本机上次同步之后又更新过」的判据：只有这种情况
/// 才说明作者可能在用一台旧设备覆盖另一台设备的新成果。
pub fn detect_conflict(
    local: &DeviceState,
    remote_meta: Option<&BackupMeta>,
    remote_filename: &str,
) -> Option<BackupConflictInfo> {
    let meta = remote_meta?;
    if meta.device_id == local.device_id {
        return None;
    }
    let last_sync = local.last_sync_at.as_deref()?;
    if !is_newer(&meta.created_at, last_sync) {
        return None;
    }
    Some(BackupConflictInfo {
        local_device_id: local.device_id.clone(),
        local_device_name: local.device_name.clone(),
        remote_device_id: meta.device_id.clone(),
        remote_device_name: meta.device_name.clone(),
        remote_filename: remote_filename.to_string(),
        remote_created_at: meta.created_at.clone(),
        local_last_sync_at: local.last_sync_at.clone(),
    })
}

/// 构造冲突提示。
pub fn pending_notice_from_conflict(conflict: &BackupConflictInfo) -> PendingNotice {
    PendingNotice {
        kind: "backup_conflict".to_string(),
        remote_filename: conflict.remote_filename.clone(),
        remote_device_id: conflict.remote_device_id.clone(),
        remote_device_name: conflict.remote_device_name.clone(),
        remote_created_at: conflict.remote_created_at.clone(),
        detected_at: Utc::now().to_rfc3339(),
    }
}

/// 生成冲突错误串。带 `CONFLICT:` 前缀供前端机器判定。
pub fn conflict_error(conflict: &BackupConflictInfo) -> String {
    format!(
        "{CONFLICT_CODE}: 远端存在来自其他设备（{}）且比本机上次同步更新的备份「{}」（{}）。\
         继续上传会用本机内容覆盖远端；请确认后再试。",
        if conflict.remote_device_name.is_empty() {
            conflict.remote_device_id.as_str()
        } else {
            conflict.remote_device_name.as_str()
        },
        conflict.remote_filename,
        conflict.remote_created_at
    )
}

/// 主机名（拿不到就退回「未知设备」）。
fn default_device_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "未知设备".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(device_id: &str, last_sync_at: Option<&str>) -> DeviceState {
        DeviceState {
            device_id: device_id.to_string(),
            device_name: "测试机".to_string(),
            last_sync_at: last_sync_at.map(|s| s.to_string()),
            pending_notice: None,
        }
    }

    fn meta(device_id: &str, created_at: &str) -> BackupMeta {
        BackupMeta {
            device_id: device_id.to_string(),
            device_name: "别的机器".to_string(),
            created_at: created_at.to_string(),
            max_updated_at: "2026-01-01 00:00:00".to_string(),
            schema_version: 3,
            app_version: "0.3.8".to_string(),
            sha256: "a".repeat(64),
        }
    }

    #[test]
    fn same_device_is_never_a_conflict() {
        let local = state("dev-a", Some("2026-01-01T00:00:00+00:00"));
        let remote = meta("dev-a", "2026-02-01T00:00:00+00:00");
        assert!(detect_conflict(&local, Some(&remote), "b.db.gz").is_none());
    }

    #[test]
    fn other_device_newer_than_last_sync_is_a_conflict() {
        let local = state("dev-a", Some("2026-01-01T00:00:00+00:00"));
        let remote = meta("dev-b", "2026-02-01T00:00:00+00:00");
        let c = detect_conflict(&local, Some(&remote), "b.db.gz").expect("应当判定为冲突");
        assert_eq!(c.remote_device_id, "dev-b");
        assert_eq!(c.local_device_id, "dev-a");
    }

    #[test]
    fn other_device_older_than_last_sync_is_not_a_conflict() {
        let local = state("dev-a", Some("2026-03-01T00:00:00+00:00"));
        let remote = meta("dev-b", "2026-02-01T00:00:00+00:00");
        assert!(detect_conflict(&local, Some(&remote), "b.db.gz").is_none());
    }

    #[test]
    fn no_last_sync_means_no_conflict() {
        // 本机从未同步过：没有「对齐点」可比，此时报冲突只会挡住
        // 作者第一次连 WebDAV 的正常流程。
        let local = state("dev-a", None);
        let remote = meta("dev-b", "2026-02-01T00:00:00+00:00");
        assert!(detect_conflict(&local, Some(&remote), "b.db.gz").is_none());
    }

    #[test]
    fn missing_meta_is_not_a_conflict() {
        let local = state("dev-a", Some("2026-01-01T00:00:00+00:00"));
        assert!(detect_conflict(&local, None, "b.db.gz").is_none());
    }

    #[test]
    fn timestamp_comparison_handles_both_forms() {
        assert!(is_newer("2026-02-01T00:00:00Z", "2026-01-01T00:00:00+00:00"));
        assert!(!is_newer("2026-01-01T00:00:00Z", "2026-02-01T00:00:00+00:00"));
        // 解析不了时退回字典序
        assert!(is_newer("2026-02-01", "2026-01-01"));
    }

    #[test]
    fn meta_sidecar_name_is_adjacent_to_the_backup() {
        assert_eq!(
            meta_sidecar_name("backup_20260101_000000Z.db.gz"),
            "backup_20260101_000000Z.db.gz.meta.json"
        );
    }
}
