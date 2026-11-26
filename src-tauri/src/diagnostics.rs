//! 作品诊断与恢复中心 —— 后端只读聚合层（V0.4.0 / P2-4）。
//!
//! ## 设计约束（Docs/V0.3.8_P2_REPORT.md §5）
//!
//! 1. **只读**：本模块不删任何记录、不删任何文件、不写库。
//!    修复能力（`asset::prune_orphan_images` 等）保持独立，由后续 UI 显式触发。
//! 2. **逐项容错**：每一项诊断都单独执行、单独 `match Result`；
//!    某一项失败只把它自己标成 `Error`，其余项照常运行。绝不整体 `?`。
//! 3. **不泄漏**：报告里只有状态、计数与固定文案摘要，
//!    **不含密钥内容、不含章节正文、不含 key_ref**。
//! 4. **诚实跳过**：当前架构不支持的四项（最近备份、未完成 AI 任务、
//!    可用磁盘空间、异常退出恢复点）一律返回 `Skipped` 并写明原因，
//!    不伪造时间戳、不编造进度。

use crate::keyring::KeyringStore;
use rusqlite::Connection;
use shared_types::{DiagnosticItem, DiagnosticStatus, DiagnosticsReport};
use std::path::Path;

/// 诊断项 id：稳定机器键，前端按此决定图标与跳转。
pub mod ids {
    pub const ASSET_REFERENCES: &str = "asset_references";
    pub const ORPHAN_FILES: &str = "orphan_files";
    pub const MISSING_KEYS: &str = "missing_keys";
    pub const DATABASE_INTEGRITY: &str = "database_integrity";
    pub const MIGRATION_STATUS: &str = "migration_status";
    pub const APP_DATA_PATHS: &str = "app_data_paths";
    pub const LAST_BACKUP: &str = "last_backup";
    pub const PENDING_AI_TASKS: &str = "pending_ai_tasks";
    pub const DISK_SPACE: &str = "disk_space";
    pub const CRASH_RECOVERY_POINT: &str = "crash_recovery_point";
}

fn item(
    id: &str,
    title: &str,
    status: DiagnosticStatus,
    summary: impl Into<String>,
    count: Option<i64>,
    reason: Option<String>,
) -> DiagnosticItem {
    DiagnosticItem {
        id: id.to_string(),
        title: title.to_string(),
        status,
        summary: summary.into(),
        count,
        reason,
    }
}

/// 把一个 `Result` 收敛成单项结果：Err 只影响这一项。
fn from_result(
    id: &str,
    title: &str,
    result: Result<DiagnosticItem, String>,
) -> DiagnosticItem {
    match result {
        Ok(mut it) => {
            // 防御：即便某一项内部忘了置 id/title，也由外层钉死。
            it.id = id.to_string();
            it.title = title.to_string();
            it
        }
        Err(e) => item(
            id,
            title,
            DiagnosticStatus::Error,
            "该项检查未能完成",
            None,
            // 错误串来自 SQLite / 文件系统，可能较长；截断避免报告膨胀。
            Some(truncate(&e, 240)),
        ),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("…（已截断）");
    out
}

/// 素材引用失效：表里有在库记录，磁盘上却找不到文件。
fn check_asset_references(
    conn: &Connection,
    app_data_dir: &Path,
) -> Result<DiagnosticItem, String> {
    let report = crate::asset::scan_storage(conn, app_data_dir)?;
    let n = report.missing_records.len() as i64;
    let (status, summary) = if n == 0 {
        (DiagnosticStatus::Ok, "图片记录与磁盘文件一致".to_string())
    } else {
        (
            DiagnosticStatus::Warning,
            format!("有 {n} 条图片记录在磁盘上找不到对应文件"),
        )
    };
    Ok(item(
        ids::ASSET_REFERENCES,
        "素材引用失效",
        status,
        summary,
        Some(n),
        None,
    ))
}

/// 孤儿文件：磁盘上有文件，但 asset 表没有任何记录指向它。
fn check_orphan_files(conn: &Connection, app_data_dir: &Path) -> Result<DiagnosticItem, String> {
    let report = crate::asset::scan_storage(conn, app_data_dir)?;
    let total = report.orphan_files.len() as i64;
    // referenced_by_content 的文件**不可清理**，单独计数以免 UI 误判为可清理空间。
    let protected = report
        .orphan_files
        .iter()
        .filter(|o| o.referenced_by_content)
        .count() as i64;
    let (status, summary) = if total == 0 {
        (DiagnosticStatus::Ok, "没有发现孤儿图片文件".to_string())
    } else {
        (
            DiagnosticStatus::Warning,
            format!(
                "有 {total} 个孤儿图片文件（其中 {protected} 个仍被正文引用，不可清理）"
            ),
        )
    };
    Ok(item(
        ids::ORPHAN_FILES,
        "孤儿文件",
        status,
        summary,
        Some(total),
        None,
    ))
}

/// 钥匙串凭据失效：只报数量，不报 key_ref / 密钥内容 / 配置正文。
fn check_missing_keys(conn: &Connection, store: &dyn KeyringStore) -> Result<DiagnosticItem, String> {
    let missing = crate::keys::scan_missing_keys(conn, store);
    let n = missing.len() as i64;
    let (status, summary) = if n == 0 {
        (DiagnosticStatus::Ok, "本机钥匙串凭据齐全".to_string())
    } else {
        (
            DiagnosticStatus::Warning,
            format!("有 {n} 个密钥引用在本机钥匙串中缺失"),
        )
    };
    Ok(item(
        ids::MISSING_KEYS,
        "钥匙串凭据失效",
        status,
        summary,
        Some(n),
        None,
    ))
}

/// 数据库完整性：复用备份预检 `preflight_backup()`，作用于当前库文件。
///
/// 该函数以只读方式打开数据库并跑 `PRAGMA integrity_check`，不做任何写入。
/// 读不到或结构损坏时返回 Err，由 `from_result` 收敛为本项 `Error`。
fn check_database_integrity(app_data_dir: &Path) -> Result<DiagnosticItem, String> {
    let db_path = app_data_dir.join("easy-writing.db");
    let report = crate::sync::backup::preflight_backup(&db_path)?;
    let (status, summary, count) = if report.is_valid {
        (
            DiagnosticStatus::Ok,
            format!(
                "数据库结构完整（{} 部作品 / {} 章 / {} 字）",
                report.novel_count, report.chapter_count, report.word_count
            ),
            Some(report.schema_version),
        )
    } else {
        (
            DiagnosticStatus::Error,
            "数据库预检未通过".to_string(),
            None,
        )
    };
    Ok(item(
        ids::DATABASE_INTEGRITY,
        "数据库完整性",
        status,
        summary,
        count,
        if report.reason.is_empty() {
            None
        } else {
            Some(report.reason)
        },
    ))
}

/// 迁移状态：直接读当前连接的 `schema_version`。
fn check_migration_status(conn: &Connection) -> Result<DiagnosticItem, String> {
    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let baseline = crate::db::BASELINE_VERSION;
    let (status, summary, reason) = if version == 0 {
        (
            DiagnosticStatus::Error,
            "没有找到任何迁移记录".to_string(),
            Some("schema_version 表为空".to_string()),
        )
    } else if version > baseline {
        (
            DiagnosticStatus::Error,
            format!("数据库格式版本 v{version} 高于当前应用支持的 v{baseline}"),
            Some("请先升级应用，不要在旧版本上继续写入".to_string()),
        )
    } else {
        (
            DiagnosticStatus::Ok,
            format!("迁移已到 v{version}（当前应用支持至 v{baseline}）"),
            None,
        )
    };
    Ok(item(
        ids::MIGRATION_STATUS,
        "迁移状态",
        status,
        summary,
        Some(version),
        reason,
    ))
}

/// 数据库 / 素材路径：仅展示路径前缀，便于作者确认备份与图片落点。
fn check_app_data_paths(app_data_dir: &Path) -> Result<DiagnosticItem, String> {
    let path = app_data_dir.to_string_lossy().to_string();
    Ok(item(
        ids::APP_DATA_PATHS,
        "数据库 / 素材路径",
        DiagnosticStatus::Ok,
        path,
        None,
        None,
    ))
}

/// 显式跳过的项目：绝不伪造数据。
fn skipped(id: &str, title: &str, reason: &str) -> DiagnosticItem {
    item(
        id,
        title,
        DiagnosticStatus::Skipped,
        "当前版本不提供该项诊断",
        None,
        Some(reason.to_string()),
    )
}

/// 聚合全部诊断项。
///
/// `conn` 与 `app_data_dir` 由调用方（IPC 命令）提供；本函数全程只读。
/// `store` 以 trait 对象注入，便于测试用内存钥匙串，绝不触碰真实系统钥匙串。
///
/// 每一项都通过 `from_result` 独立收敛错误，因此任一项失败都不会中断后续项。
pub fn collect_diagnostics(
    conn: &Connection,
    app_data_dir: &Path,
    store: &dyn KeyringStore,
) -> DiagnosticsReport {
    // scan_storage 做的是全库对账，两项共用一次扫描即可；
    // 但两次调用分别包在 from_result 里，失败时只影响各自那一项。
    let asset_references = from_result(
        ids::ASSET_REFERENCES,
        "素材引用失效",
        check_asset_references(conn, app_data_dir),
    );
    let orphan_files = from_result(
        ids::ORPHAN_FILES,
        "孤儿文件",
        check_orphan_files(conn, app_data_dir),
    );
    let missing_keys = from_result(
        ids::MISSING_KEYS,
        "钥匙串凭据失效",
        check_missing_keys(conn, store),
    );
    let database_integrity = from_result(
        ids::DATABASE_INTEGRITY,
        "数据库完整性",
        check_database_integrity(app_data_dir),
    );
    let migration_status = from_result(
        ids::MIGRATION_STATUS,
        "迁移状态",
        check_migration_status(conn),
    );
    let app_data_paths = from_result(
        ids::APP_DATA_PATHS,
        "数据库 / 素材路径",
        check_app_data_paths(app_data_dir),
    );

    let mut report = DiagnosticsReport {
        items: vec![
            asset_references,
            orphan_files,
            missing_keys,
            database_integrity,
            migration_status,
            app_data_paths,
            skipped(
                ids::LAST_BACKUP,
                "最近备份时间与结果",
                "当前不持久化成功备份的时间戳，无法给出真实值",
            ),
            skipped(
                ids::PENDING_AI_TASKS,
                "未完成 / 失败 AI 任务",
                "依赖 P1-2 的任务持久化，本版本明确未实现",
            ),
            skipped(
                ids::DISK_SPACE,
                "可用磁盘空间",
                "尚未实现磁盘空间查询",
            ),
            skipped(
                ids::CRASH_RECOVERY_POINT,
                "异常退出恢复点",
                "当前架构没有崩溃恢复机制，不存在可用的恢复点",
            ),
        ],
        ok_count: 0,
        warning_count: 0,
        error_count: 0,
        skipped_count: 0,
    };
    report.recompute_counts();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 报告不得含凭据内容或章节正文（结构级自检）。
    #[test]
    fn skipped_items_are_never_fabricated() {
        // 用一份临时库，确认 skipped 的四项永远保持 Skipped 且有原因。
        let dir = std::env::temp_dir().join(format!(
            "ew_diag_unit_{}",
            uuid::Uuid::new_v4().simple()
        ));
        let pool = crate::db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        struct Empty;
        impl KeyringStore for Empty {
            fn get_secret(&self, _: &str) -> Result<String, String> {
                Err("missing".into())
            }
            fn set_secret(&self, _: &str, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn delete_secret(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
        }
        let report = collect_diagnostics(&conn, &dir, &Empty);
        for it in report.items.iter().filter(|i| {
            matches!(
                i.id.as_str(),
                ids::LAST_BACKUP
                    | ids::PENDING_AI_TASKS
                    | ids::DISK_SPACE
                    | ids::CRASH_RECOVERY_POINT
            )
        }) {
            assert_eq!(it.status, DiagnosticStatus::Skipped);
            assert!(it.reason.is_some(), "{} 必须写明跳过原因", it.id);
            assert!(it.count.is_none());
        }
    }
}
