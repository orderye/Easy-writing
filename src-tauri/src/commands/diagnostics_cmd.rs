//! 作品诊断与恢复中心的 IPC 入口（V0.4.0 / P2-4）。
//!
//! 只读：命令只读取状态、计数与路径，不删记录、不删文件、不写库。
//! 聚合逻辑在 `crate::diagnostics`，本文件只负责取 app_data_dir 与 DB 连接。

use crate::db::DbPool;
use shared_types::DiagnosticsReport;
use tauri::State;

/// 生成作品诊断报告。
///
/// 单项诊断失败只把那一项标成 error，不影响其余项；
/// 当前架构不支持的四项显式标成 skipped 并写明原因。
#[tauri::command(rename_all = "snake_case")]
pub fn get_diagnostics(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
) -> Result<DiagnosticsReport, String> {
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    Ok(crate::diagnostics::collect_diagnostics(
        &conn,
        &app_data_dir,
        crate::keyring::system(),
    ))
}
