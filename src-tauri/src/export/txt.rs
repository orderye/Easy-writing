//! 纯文本（txt）导出。
//!
//! 逐章流式写：章节一到就写进文件，不在内存里攒完整本书。
//!
//! 段落有两种排布（任务书要求的「段首缩进或空行」）：
//!
//! - `indent`：每段前面加两个全角空格，段与段之间不空行；
//! - `blank_line`：段首不缩进，段与段之间空一行。
//!
//! txt 装不下图片，所以正文里的图片一律跳过（由调用方计入「跳过张数」），
//! 也不写任何占位文字 —— 占了位置就会让「导出字数」与正文对不上。

use std::io::Write;

use shared_types::ParagraphSeparator;

use super::doc::{Block, Inline};
use super::{ChapterOutput, ChapterStats, ExportWriter};

/// 全角空格：中文排版的段首缩进宽度。
const INDENT: &str = "\u{3000}\u{3000}";

/// 分割线在纯文本里的画法。
const RULE: &str = "────────────────────";

pub struct TxtWriter<W: Write> {
    out: W,
    separator: ParagraphSeparator,
    /// 上一个块之后是否需要先空一行。
    pending_blank: bool,
    /// 是否已经写过任何内容。
    started: bool,
}

impl<W: Write> TxtWriter<W> {
    pub fn new(out: W, separator: ParagraphSeparator) -> Self {
        Self {
            out,
            separator,
            pending_blank: false,
            started: false,
        }
    }

    fn write_line(&mut self, line: &str) -> Result<(), String> {
        if self.pending_blank {
            self.out
                .write_all(b"\n")
                .map_err(|e| format!("写入导出文件失败: {}", e))?;
            self.pending_blank = false;
        }
        self.out
            .write_all(line.as_bytes())
            .map_err(|e| format!("写入导出文件失败: {}", e))?;
        self.out
            .write_all(b"\n")
            .map_err(|e| format!("写入导出文件失败: {}", e))?;
        self.started = true;
        Ok(())
    }

    /// 段首缩进模式下的正文行。
    fn write_paragraph(&mut self, text: &str) -> Result<(), String> {
        if text.is_empty() {
            return Ok(());
        }
        match self.separator {
            ParagraphSeparator::Indent => {
                let mut line = String::with_capacity(text.len() + INDENT.len());
                line.push_str(INDENT);
                // 段内硬换行保留，其余缩进只作用于首行。
                for (idx, part) in text.split('\n').enumerate() {
                    if idx > 0 {
                        line.push('\n');
                    }
                    line.push_str(part);
                }
                self.write_line(&line)
            }
            ParagraphSeparator::BlankLine => {
                self.write_line(text)?;
                self.pending_blank = true;
                Ok(())
            }
        }
    }
}

impl<W: Write> ExportWriter for TxtWriter<W> {
    fn write_chapter(&mut self, chapter: &ChapterOutput) -> Result<ChapterStats, String> {
        let mut stats = ChapterStats::default();
        if let Some(volume_title) = &chapter.volume_title {
            if !volume_title.is_empty() {
                self.write_line(volume_title)?;
            }
        }
        if !chapter.title.is_empty() {
            self.write_line(&chapter.title)?;
        }
        for block in &chapter.blocks {
            match block {
                Block::Paragraph { inlines } | Block::Heading { inlines, .. } => {
                    let text = inlines_text(inlines);
                    self.write_paragraph(&text)?;
                }
                Block::Quote { inlines } => {
                    let text = inlines_text(inlines);
                    if !text.is_empty() {
                        self.write_paragraph(&format!("「{}」", text))?;
                    }
                }
                Block::ListItem {
                    ordered,
                    index,
                    inlines,
                } => {
                    let text = inlines_text(inlines);
                    if text.is_empty() {
                        continue;
                    }
                    let prefix = if *ordered {
                        format!("{}. ", index)
                    } else {
                        "- ".to_string()
                    };
                    self.write_paragraph(&format!("{}{}", prefix, text))?;
                }
                // txt 装不下图片：跳过并计数，让作者在导出结果里看到。
                Block::Image { .. } => {
                    stats.skipped_images += 1;
                }
                Block::Rule => {
                    self.write_line(RULE)?;
                }
            }
        }
        // 章与章之间留一个空行。
        self.pending_blank = true;
        Ok(stats)
    }

    fn finish(&mut self) -> Result<(), String> {
        self.out
            .flush()
            .map_err(|e| format!("写入导出文件失败: {}", e))
    }
}

/// 行内片段拼成纯文本。
pub fn inlines_text(inlines: &[Inline]) -> String {
    inlines.iter().map(|i| i.plain()).collect()
}
