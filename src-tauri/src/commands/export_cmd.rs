//! 导出命令（任务书 T02）。
//!
//! 三个命令，刻意拆开而不是一个大命令：
//!
//! 1. `choose_export_file` —— 只负责弹系统「另存为」对话框；
//! 2. `export_novel` —— 只负责执行导出，中途用事件报进度；
//! 3. `cancel_export` —— 只负责置取消位。
//!
//! 拆开的原因：路径是**用户的选择**，而执行是**可能很慢的**。
//! 合在一起的话，用户点「取消」时无法区分「取消导出」和「关掉对话框」，
//! 前端也就没法在对话框关闭后继续显示进度。

use std::path::PathBuf;

use shared_types::{ExportFormat, ExportOptions, ExportProgress, ExportSummary};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::DbPool;
use crate::export::ExportState;

/// 导出进度事件名。
pub const PROGRESS_EVENT: &str = "novel-export://progress";

/// 弹出系统「另存为」对话框。
///
/// 返回 `None` 表示用户取消。**不生成任何文件** —— 路径由作者选，
/// 应用不该替他决定写到哪儿。
#[tauri::command(rename_all = "snake_case")]
pub async fn choose_export_file(
    app: AppHandle,
    file_name: String,
    format: String,
) -> Result<Option<String>, String> {
    let format = match format.as_str() {
        "docx" => ExportFormat::Docx,
        "epub" => ExportFormat::Epub,
        _ => ExportFormat::Txt,
    };

    // 对话框没有扩展名时补上格式对应的扩展名，否则作者存出来的文件
    // 在别的程序里打不开。
    let mut suggested = file_name.trim().to_string();
    if suggested.is_empty() {
        suggested = "未命名作品".to_string();
    }
    if PathBuf::from(&suggested)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
        != Some(format.extension())
    {
        suggested = format!("{}.{}", suggested, format.extension());
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    let filter_label = match format {
        ExportFormat::Txt => "纯文本",
        ExportFormat::Docx => "Word 文档",
        ExportFormat::Epub => "EPUB 电子书",
    };
    let extension = format.extension();

    app.dialog()
        .file()
        .set_file_name(&suggested)
        .add_filter(filter_label, &[extension])
        .save_file(move |chosen| {
            let _ = tx.send(chosen);
        });

    let chosen = rx
        .await
        .map_err(|_| "保存对话框未返回结果".to_string())?;
    // `into_path()` 可能失败（例如作者选了一个目录而不是文件）。
    // 这属于「保存失败」而不是「取消」，要分开报 —— 前端才能给出
    // 「换一个文件名」而不是「已取消」这种错误提示。
    match chosen {
        Some(p) => match p.into_path() {
            Ok(path) => Ok(Some(path.to_string_lossy().to_string())),
            // 保存路径取不到（作者选了一个目录而不是文件等）属于
            // 「保存失败」而不是「取消」，要分开报 —— 前端才能给出
            // 「换一个文件名」而不是「已取消」这种错误提示。
            Err(e) => Err(format!("无法确定保存路径: {}", e)),
        },
        None => Ok(None),
    }
}

/// 执行导出。
///
/// 导出是「重活」（300 章 / 百万字要读库、解析正文、写文件），
/// 因此命令体只做准备工作，真正的活丢进 `spawn_blocking`，
/// 前端在此期间不会被卡住，进度靠事件回传。
#[tauri::command(rename_all = "snake_case")]
pub async fn export_novel(
    app: AppHandle,
    pool: State<'_, DbPool>,
    state: State<'_, ExportState>,
    path: String,
    options: ExportOptions,
) -> Result<ExportSummary, String> {
    if path.trim().is_empty() {
        return Err("导出路径为空。".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {}", e))?;

    let pool = pool.inner().clone();
    let cancel = state.cancelled.clone();
    // 新的一次导出先清掉上一次的取消标记。
    state.reset();

    let emitter = app.clone();
    let target = PathBuf::from(path);

    let outcome = tokio::task::spawn_blocking(move || {
        crate::export::export_novel(
            &pool,
            &app_data_dir,
            &target,
            &options,
            &|progress: ExportProgress| {
                let _ = emitter.emit(PROGRESS_EVENT, progress);
            },
            &cancel,
        )
    })
    .await
    .map_err(|e| format!("导出任务异常终止: {}", e))?;

    Ok(outcome?.summary)
}

/// 请求取消当前导出。
///
/// 取消是**协作式**的：导出循环在章边界检查取消位，
/// 因此取消后最多多写完一章，且临时文件会被删除。
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_export(state: State<'_, ExportState>) -> Result<(), String> {
    state.cancel();
    Ok(())
}
