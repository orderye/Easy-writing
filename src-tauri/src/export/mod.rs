//! 作品导出（任务书 T02）：txt / docx / epub。
//!
//! ## 流程
//!
//! ```text
//! 前端 ExportModal
//!   ├─ choose_export_file  → 系统「另存为」对话框 → 目标路径
//!   ├─ export_novel        → 本模块的 export_novel()（后台线程执行）
//!   │    ├─ 逐章解析 → 逐章写出 → 逐章上报进度事件
//!   │    └─ 写临时文件 → 成功后 rename 到目标路径
//!   └─ cancel_export       → 置取消位，导出在章边界中止
//! ```
//!
//! ## 三条硬约束
//!
//! 1. **不留半成品**：先写 `<目标名>.ew-export-part`，成功才 rename；
//!    失败或取消一律删掉临时文件。
//! 2. **不阻塞界面**：命令是 async，重活在 `spawn_blocking` 里跑，
//!    前端只通过事件收进度。
//! 3. **字数口径与编辑器一致**：见 [`doc`] 模块的说明。

pub mod assets;
pub mod doc;
pub mod docx;
pub mod epub;
pub mod txt;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use shared_types::{
    ExportFormat, ExportOptions, ExportProgress, ExportSummary, ParagraphSeparator,
};

use crate::db::{self, DbPool};

use doc::Block;

/// 导出取消标志（由 `cancel_export` 命令置位）。
///
/// 用 `Arc` 包一层是为了让命令层能把取消标志**移动**进后台任务
/// （`AtomicBool` 本身不可克隆）。
#[derive(Debug, Default)]
pub struct ExportState {
    pub cancelled: Arc<AtomicBool>,
}

impl ExportState {
    /// 请求取消。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// 是否已被取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 复位（一次导出开始前调用）。
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

/// 一章的导出素材（已渲染标题、已解析正文）。
pub struct ChapterOutput {
    /// 卷标题（渲染后）。只有该卷的第一章会带它。
    pub volume_title: Option<String>,
    /// 章标题（渲染后）。
    pub title: String,
    pub blocks: Vec<Block>,
}

/// 一章写完后由写出器回报的统计。
#[derive(Debug, Clone, Copy, Default)]
pub struct ChapterStats {
    /// 成功写入的图片数。
    pub included_images: u32,
    /// 跳过的图片数（格式不支持、开关关闭或素材缺失）。
    pub skipped_images: u32,
}

/// 写出器抽象：三种格式只需实现「写一章」与「收尾」。
pub trait ExportWriter {
    /// 写一章（含标题与正文）。
    fn write_chapter(&mut self, chapter: &ChapterOutput) -> Result<ChapterStats, String>;
    /// 收尾：写目录 / 清单等索引文件并关闭容器。
    fn finish(&mut self) -> Result<(), String>;
}

/// 从库里读出的一章原始数据。
struct SourceChapter {
    volume_id: String,
    volume_title: String,
    volume_index: usize,
    chapter_title: String,
    chapter_index: usize,
    content: String,
    content_plain: String,
    novel_title: String,
}

/// 一次导出的执行结果。
pub struct ExportOutcome {
    pub summary: ExportSummary,
    /// 最终落盘路径（被取消时为空）。
    pub path: PathBuf,
}

/// 执行一次导出。
///
/// * `target_path` —— 最终文件路径；扩展名会按 `options.format` 纠正。
/// * `progress` —— 进度回调（命令层用它发事件）。
/// * `cancel` —— 取消标志。
pub fn export_novel(
    pool: &DbPool,
    app_data_dir: &Path,
    target_path: &Path,
    options: &ExportOptions,
    progress: &dyn Fn(ExportProgress),
    cancel: &AtomicBool,
) -> Result<ExportOutcome, String> {
    let started = std::time::Instant::now();

    let target_path = normalize_target_path(target_path, options.format);
    let temp_path = temp_path_of(&target_path);

    let chapters = load_chapters(pool, options)?;
    if chapters.is_empty() {
        return Err("没有可导出的章节（请检查导出范围）。".to_string());
    }

    let total = chapters.len() as u32;
    progress(ExportProgress {
        done: 0,
        total,
        current_title: String::new(),
        phase: "preparing".to_string(),
    });

    // 先清掉可能残留的临时文件（上一次崩溃留下的）。
    let _ = std::fs::remove_file(&temp_path);

    // 写出器写到临时文件，成功后才 rename 到目标路径：
    // 崩溃或取消都不会在目标位置留下半成品。
    let mut writer = open_writer(&temp_path, options, app_data_dir, &chapters[0].novel_title)?;

    let mut done = 0u32;
    let mut word_count = 0i64;
    let mut text_chars = 0i64;
    let mut included_images = 0u32;
    let mut skipped_images = 0u32;
    let mut degraded_chapters: Vec<String> = Vec::new();
    let mut volume_count = 0u32;
    let mut last_volume: Option<String> = None;

    for chapter in &chapters {
        if cancel.load(Ordering::SeqCst) {
            let _ = std::fs::remove_file(&temp_path);
            return Ok(ExportOutcome {
                summary: ExportSummary {
                    path: String::new(),
                    format: options.format,
                    volume_count,
                    chapter_count: done,
                    word_count,
                    exported_text_chars: text_chars,
                    included_images,
                    skipped_images,
                    degraded_chapters,
                    cancelled: true,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
                path: PathBuf::new(),
            });
        }

        // 换卷时输出一次卷标题。
        let volume_title = if last_volume.as_deref() != Some(chapter.volume_id.as_str()) {
            volume_count += 1;
            last_volume = Some(chapter.volume_id.clone());
            Some(render_template(
                &options.volume_title_template,
                &TemplateVars {
                    novel: &chapters[0].novel_title,
                    volume_index: chapter.volume_index,
                    volume_title: &chapter.volume_title,
                    chapter_index: chapter.chapter_index,
                    chapter_title: &chapter.chapter_title,
                },
            ))
        } else {
            None
        };

        let parsed = doc::parse_chapter(&chapter.content, &chapter.content_plain);
        if parsed.degraded {
            degraded_chapters.push(chapter.chapter_title.clone());
        }
        text_chars += parsed.text_chars;
        // 字数用**导出时实测**的口径（`doc::tiptap_char_count`），
        // 而不是库里的 `word_count` 列：那一列是保存正文时写进去的快照，
        // 历史数据、导入数据都可能与当前正文对不上。任务书要求
        // 「导出字数与应用统计一致」，那就用同一套规则现算一遍。
        word_count += parsed.char_count;

        let stats = writer.write_chapter(&ChapterOutput {
            volume_title,
            title: render_template(
                &options.chapter_title_template,
                &TemplateVars {
                    novel: &chapters[0].novel_title,
                    volume_index: chapter.volume_index,
                    volume_title: &chapter.volume_title,
                    chapter_index: chapter.chapter_index,
                    chapter_title: &chapter.chapter_title,
                },
            ),
            blocks: parsed.blocks,
        })?;
        included_images += stats.included_images;
        skipped_images += stats.skipped_images;

        done += 1;
        progress(ExportProgress {
            done,
            total,
            current_title: chapter.chapter_title.clone(),
            phase: "writing".to_string(),
        });
    }

    writer.finish()?;

    progress(ExportProgress {
        done,
        total,
        current_title: String::new(),
        phase: "finalizing".to_string(),
    });

    // 临时文件 → 目标文件。
    std::fs::rename(&temp_path, &target_path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!("导出文件落盘失败: {}", e)
    })?;

    progress(ExportProgress {
        done,
        total,
        current_title: String::new(),
        phase: "done".to_string(),
    });

    Ok(ExportOutcome {
        summary: ExportSummary {
            path: target_path.to_string_lossy().to_string(),
            format: options.format,
            volume_count,
            chapter_count: done,
            word_count,
            exported_text_chars: text_chars,
            included_images,
            skipped_images,
            degraded_chapters,
            cancelled: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
        path: target_path,
    })
}

/// 模板渲染用的变量。
#[derive(Debug, Clone, Copy)]
pub struct TemplateVars<'a> {
    pub novel: &'a str,
    pub volume_index: usize,
    pub volume_title: &'a str,
    pub chapter_index: usize,
    pub chapter_title: &'a str,
}

/// 渲染标题模板。
///
/// 支持的占位符：`{novel}` `{volume_index}` `{volume_title}`
/// `{chapter_index}` `{chapter_title}`。
/// 未知占位符原样保留 —— 模板是作者自己写的拼写错误时，
/// 静默吞掉会让「模板没生效」变成一件查不出来的事。
pub fn render_template(template: &str, vars: &TemplateVars<'_>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                let replacement = match key {
                    "novel" => Some(vars.novel.to_string()),
                    "volume_index" => Some(vars.volume_index.to_string()),
                    "volume_title" => Some(vars.volume_title.to_string()),
                    "chapter_index" => Some(vars.chapter_index.to_string()),
                    "chapter_title" => Some(vars.chapter_title.to_string()),
                    _ => None,
                };
                match replacement {
                    Some(text) => out.push_str(&text),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(rest);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 目标路径：扩展名按格式纠正。
fn normalize_target_path(path: &Path, format: ExportFormat) -> PathBuf {
    let mut p = path.to_path_buf();
    p.set_extension(format.extension());
    p
}

/// 临时文件路径。
fn temp_path_of(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "export".to_string());
    name.push_str(".ew-export-part");
    target.with_file_name(name)
}

/// 打开对应格式的写出器。
fn open_writer(
    target: &Path,
    options: &ExportOptions,
    app_data_dir: &Path,
    novel_title: &str,
) -> Result<Box<dyn ExportWriter>, String> {
    match options.format {
        ExportFormat::Txt => {
            let file = std::fs::File::create(target)
                .map_err(|e| format!("无法创建导出文件: {}", e))?;
            Ok(Box::new(txt::TxtWriter::new(
                file,
                options.paragraph_separator,
            )))
        }
        ExportFormat::Docx => {
            let file = std::fs::File::create(target)
                .map_err(|e| format!("无法创建导出文件: {}", e))?;
            Ok(Box::new(docx::DocxWriter::new(
                file,
                novel_title,
                options.paragraph_separator,
                options.include_images,
                app_data_dir.to_path_buf(),
            )?))
        }
        ExportFormat::Epub => {
            let file = std::fs::File::create(target)
                .map_err(|e| format!("无法创建导出文件: {}", e))?;
            Ok(Box::new(epub::EpubWriter::new(
                file,
                novel_title,
                options.paragraph_separator,
                options.include_images,
                app_data_dir.to_path_buf(),
            )?))
        }
    }
}

/// 从库里读出待导出的章节（已按范围过滤）。
fn load_chapters(pool: &DbPool, options: &ExportOptions) -> Result<Vec<SourceChapter>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let novel = db::novel::get_novel(&conn, &options.novel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "作品不存在。".to_string())?;

    let volumes = db::volume::list_volumes(&conn, &options.novel_id).map_err(|e| e.to_string())?;
    let volume_index: std::collections::HashMap<String, (usize, String)> = volumes
        .iter()
        .enumerate()
        .map(|(idx, v)| (v.id.clone(), (idx + 1, v.title.clone())))
        .collect();

    let all = db::chapter::list_chapters_for_export(&conn, &options.novel_id)
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    let mut chapter_index_by_volume: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for chapter in all {
        let Some((volume_idx, volume_title)) = volume_index.get(&chapter.volume_id) else {
            continue;
        };
        if !options.chapter_ids.is_empty() {
            if !options.chapter_ids.iter().any(|id| id == &chapter.id) {
                continue;
            }
        } else if !options.volume_ids.is_empty() {
            if !options.volume_ids.iter().any(|id| id == &chapter.volume_id) {
                continue;
            }
        }
        let counter = chapter_index_by_volume.entry(chapter.volume_id.clone()).or_insert(0);
        *counter += 1;
        out.push(SourceChapter {
            volume_id: chapter.volume_id.clone(),
            volume_title: volume_title.clone(),
            volume_index: *volume_idx,
            chapter_title: chapter.title.clone(),
            chapter_index: *counter,
            content: chapter.content.clone(),
            content_plain: chapter.content_plain.clone(),
            novel_title: novel.title.clone(),
        });
    }

    Ok(out)
}

/// 段落分隔方式（供命令层与前端共用默认值）。
pub fn default_separator() -> ParagraphSeparator {
    ParagraphSeparator::Indent
}
