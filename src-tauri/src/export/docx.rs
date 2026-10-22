//! DOCX 写出器。
//!
//! ## 为什么自己拼 OOXML
//!
//! docx 本质上是一个 zip：几个固定的几件套（`[Content_Types].xml` / `_rels/.rels` /
//! `word/document.xml` / `word/styles.xml` / …）。仓库里没有 `docx-rs` 之类的依赖，
//! 硬塞一个进来会带来体积与版本风险，而这几件套的形状其实很稳定 —— 自己拼反而能
//! 精确控制导出特有的行为：**图片只写一份、关系 Id 复用、章节层级可被 Word 导航窗格识别**。
//!
//! ## 流式
//!
//! `word/document.xml` 是**边写章边写进 zip** 的：`new()` 里就 `start_file` 并写下
//! 文档骨架，每写一章追加一段，最后 `finish()` 收尾。
//! 一部长篇上百万字，先在内存里拼成巨型字符串再压 zip 是不可接受的 —— 那正是
//! [`super::txt`] 那种纯文本写出器要避免的事，docx 只会更糟。
//!
//! zip 条目之间可以任意排序，所以正文写完之后再补写媒体文件、关系表、样式表，
//! 都不影响 Word 读取。
//!
//! ## 底层 writer 必须可 Seek
//!
//! `zip::ZipWriter` 需要 `W: Write + Seek`：它要在条目结束时**回写**本地文件头里的
//! CRC 与长度，并在最后追加中央目录。导出目标是磁盘文件（`std::fs::File`），
//! 天然满足；测试里用 `Cursor<Vec<u8>>` 也满足。

use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::PathBuf;

use shared_types::ParagraphSeparator;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::assets::AssetStore;
use super::doc::{Block, Inline};
use super::{ChapterOutput, ChapterStats, ExportWriter};

/// 96 DPI 下 1 像素 = 9525 EMU。
const EMU_PER_PIXEL: u64 = 9525;

/// 图片显示宽度上限：6 英寸（EMU）。
const MAX_IMAGE_CX: u64 = 5_486_400;

/// 无法识别图片尺寸时的兜底尺寸。
const FALLBACK_IMAGE_SIZE: (u32, u32) = (800, 600);

/// 文档骨架：命名空间声明 + `<w:body>` 开标签。
const DOCUMENT_XML_HEAD: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<w:document",
    " xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"",
    " xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"",
    " xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"",
    " xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"",
    " xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
    "<w:body>",
);

/// 页面设置：A4 + 常规页边距。必须是 `<w:body>` 的最后一个子元素。
const SECT_PR: &str = concat!(
    "<w:sectPr>",
    "<w:pgSz w:w=\"11906\" w:h=\"16838\"/>",
    "<w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"",
    " w:header=\"851\" w:footer=\"992\" w:gutter=\"0\"/>",
    "</w:sectPr>",
);

/// 样式表。
///
/// `Heading1` / `Heading2` 带 `w:outlineLvl`，Word 的导航窗格据此识别章节层级；
/// 少了它，导出的 docx 在 Word 里就是一篇没有结构的流水账。
const STYLES_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:docDefaults>",
    "<w:rPrDefault><w:rPr>",
    "<w:rFonts w:ascii=\"Calibri\" w:eastAsia=\"SimSun\" w:hAnsi=\"Calibri\" w:cs=\"Times New Roman\"/>",
    "<w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/>",
    "<w:lang w:val=\"en-US\" w:eastAsia=\"zh-CN\"/>",
    "</w:rPr></w:rPrDefault>",
    "<w:pPrDefault><w:pPr>",
    "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"360\" w:lineRule=\"auto\"/>",
    "</w:pPr></w:pPrDefault>",
    "</w:docDefaults>",
    "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">",
    "<w:name w:val=\"Normal\"/><w:qFormat/>",
    "<w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"360\" w:lineRule=\"auto\"/></w:pPr>",
    "<w:rPr><w:rFonts w:ascii=\"Calibri\" w:eastAsia=\"SimSun\" w:hAnsi=\"Calibri\"/>",
    "<w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/></w:rPr>",
    "</w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Title\">",
    "<w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>",
    "<w:pPr><w:spacing w:before=\"240\" w:after=\"240\"/><w:jc w:val=\"center\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"44\"/><w:szCs w:val=\"44\"/></w:rPr>",
    "</w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\">",
    "<w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>",
    "<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"32\"/><w:szCs w:val=\"32\"/></w:rPr>",
    "</w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading2\">",
    "<w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>",
    "<w:pPr><w:keepNext/><w:spacing w:before=\"200\" w:after=\"100\"/><w:outlineLvl w:val=\"1\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"28\"/><w:szCs w:val=\"28\"/></w:rPr>",
    "</w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading3\">",
    "<w:name w:val=\"heading 3\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>",
    "<w:pPr><w:keepNext/><w:spacing w:before=\"160\" w:after=\"80\"/><w:outlineLvl w:val=\"2\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"26\"/><w:szCs w:val=\"26\"/></w:rPr>",
    "</w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading4\">",
    "<w:name w:val=\"heading 4\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>",
    "<w:pPr><w:keepNext/><w:spacing w:before=\"120\" w:after=\"60\"/><w:outlineLvl w:val=\"3\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/></w:rPr>",
    "</w:style>",
    "</w:styles>",
);

/// 已被登记进包内的一张图片。
struct ImageEntry {
    /// 正文里的原始引用（同时是素材缓存的键）。
    src: String,
    /// 关系 Id：`rIdImg1`、`rIdImg2` …
    rel_id: String,
    /// 包内媒体文件名：`image1.png` …
    media_name: String,
    /// 显示宽度（EMU）。
    cx: u64,
    /// 显示高度（EMU）。
    cy: u64,
}

/// DOCX 写出器。
///
/// 正文流式写进 zip：每写一章就往 `word/document.xml` 追加一段，
/// **不在内存里攒整本书**。
pub struct DocxWriter<W: Write + Seek> {
    /// zip 容器。`None` 表示已经收尾关闭。
    zip: Option<ZipWriter<W>>,
    /// 图片素材缓存：同一张图只读一次。
    assets: AssetStore,
    /// 应用数据目录（本地图片存放在 `images/` 下）。
    app_data_dir: PathBuf,
    /// 作品标题（写进 `docProps/core.xml` 的 `dc:title`）。
    title: String,
    /// 段落分隔方式。
    separator: ParagraphSeparator,
    /// 是否把正文里的图片嵌入文档。
    include_images: bool,
    /// 已登记的图片，按首次出现顺序。
    images: Vec<ImageEntry>,
    /// 原始引用 → `images` 下标（同一张图复用同一个关系 Id）。
    image_index: HashMap<String, usize>,
    /// 图片序号：同时决定 `rIdImgN` 与 `imageN.<ext>`。
    image_seq: u32,
    /// 绘图对象序号：`wp:docPr` 的 `id` 必须唯一。
    drawing_seq: u32,
    /// 上一个正文段落后是否需要补一个空段落（`BlankLine` 模式）。
    pending_blank: bool,
}

impl<W: Write + Seek> DocxWriter<W> {
    /// 打开一个 DOCX 写出器。
    ///
    /// `novel_title` 会作为文档的第一个段落（`Title` 样式）写入。
    pub fn new(
        out: W,
        novel_title: &str,
        separator: ParagraphSeparator,
        include_images: bool,
        app_data_dir: PathBuf,
    ) -> Result<Self, String> {
        let mut zip = ZipWriter::new(out);
        zip.start_file("word/document.xml", file_options())
            .map_err(|e| format!("初始化 DOCX 包失败（无法创建正文条目）: {e}"))?;

        let mut writer = DocxWriter {
            zip: Some(zip),
            assets: AssetStore::new(),
            app_data_dir,
            title: novel_title.to_string(),
            separator,
            include_images,
            images: Vec::new(),
            image_index: HashMap::new(),
            image_seq: 0,
            drawing_seq: 0,
            pending_blank: false,
        };

        writer.write_str(DOCUMENT_XML_HEAD)?;
        // 作品标题：文档的第一个段落。
        if !novel_title.trim().is_empty() {
            let xml = build_paragraph(Some("Title"), "", &plain_runs(novel_title));
            writer.write_str(&xml)?;
        }
        Ok(writer)
    }

    /// 往正文追加一段 XML。
    fn write_str(&mut self, text: &str) -> Result<(), String> {
        let zip = self
            .zip
            .as_mut()
            .ok_or_else(|| "DOCX 写出器已关闭，无法继续写入正文。".to_string())?;
        zip.write_all(text.as_bytes())
            .map_err(|e| format!("写入 DOCX 正文失败: {e}"))
    }

    /// 写一个正文段落，并处理「段首缩进 / 段间空行」两种排布。
    ///
    /// `ppr_mid` 是这个段落特有的段落属性（如引用的左右缩进）；为空时
    /// `Indent` 模式补上首行缩进两字。
    fn write_body_paragraph(&mut self, ppr_mid: &str, runs: &str) -> Result<(), String> {
        // `BlankLine` 模式：段与段之间补一个空段落。
        if self.separator == ParagraphSeparator::BlankLine && self.pending_blank {
            self.write_str("<w:p/>")?;
        }

        let ppr = if ppr_mid.is_empty() && self.separator == ParagraphSeparator::Indent {
            // 首行缩进两字：`firstLineChars` 以「百分之一字符」为单位，200 即两字；
            // `firstLine` 是没有字号信息时的兜底（两字 ≈ 420 twips）。
            "<w:ind w:firstLineChars=\"200\" w:firstLine=\"420\"/>"
        } else {
            ppr_mid
        };

        let xml = build_paragraph(None, ppr, runs);
        self.write_str(&xml)?;
        self.pending_blank = true;
        Ok(())
    }

    /// 登记一张图片，返回它在 `images` 里的下标。
    ///
    /// 已经登记过的引用直接复用：**媒体文件只写一份**，关系 Id 也不变。
    /// 解析不出素材（外链图 / 坏引用 / 文件已删）时返回 `None`。
    fn register_image(&mut self, src: &str) -> Result<Option<usize>, String> {
        if let Some(&idx) = self.image_index.get(src) {
            return Ok(Some(idx));
        }
        let Some((extension, cx, cy)) = self.resolve_image(src) else {
            return Ok(None);
        };
        self.image_seq += 1;
        let seq = self.image_seq;
        self.images.push(ImageEntry {
            src: src.to_string(),
            rel_id: format!("rIdImg{seq}"),
            media_name: format!("image{seq}.{extension}"),
            cx,
            cy,
        });
        let idx = self.images.len() - 1;
        self.image_index.insert(src.to_string(), idx);
        Ok(Some(idx))
    }

    /// 解析一张图片素材，取出扩展名与显示尺寸。
    fn resolve_image(&mut self, src: &str) -> Option<(String, u64, u64)> {
        let asset = self.assets.resolve(&self.app_data_dir, src)?;
        let extension = normalize_extension(&asset.extension);
        let (cx, cy) = emu_size(&asset.bytes, &extension);
        Some((extension, cx, cy))
    }

    /// 生成一张内联图片所在的段落。
    fn build_image_paragraph(&mut self, idx: usize, alt: &str) -> String {
        self.drawing_seq += 1;
        let doc_pr_id = self.drawing_seq;
        let entry = &self.images[idx];
        build_image_xml(entry, alt, doc_pr_id)
    }
}

impl<W: Write + Seek> ExportWriter for DocxWriter<W> {
    fn write_chapter(&mut self, chapter: &ChapterOutput) -> Result<ChapterStats, String> {
        let mut stats = ChapterStats::default();

        // 卷标题：Heading1。
        if let Some(volume_title) = &chapter.volume_title {
            if !volume_title.trim().is_empty() {
                let xml = build_paragraph(Some("Heading1"), "", &plain_runs(volume_title));
                self.write_str(&xml)?;
                self.pending_blank = false;
            }
        }

        // 章标题：Heading2。
        if !chapter.title.trim().is_empty() {
            let xml = build_paragraph(Some("Heading2"), "", &plain_runs(&chapter.title));
            self.write_str(&xml)?;
            self.pending_blank = false;
        }

        for block in &chapter.blocks {
            match block {
                Block::Heading { level, inlines } => {
                    // 章内小标题整体下沉两级：卷=1、章=2，章内小标题从 3 起。
                    let style = format!("Heading{}", level.saturating_add(2).min(4));
                    let xml = build_paragraph(Some(&style), "", &build_runs(inlines, false));
                    self.write_str(&xml)?;
                    self.pending_blank = false;
                }
                Block::Paragraph { inlines } => {
                    let runs = build_runs(inlines, false);
                    self.write_body_paragraph("", &runs)?;
                }
                Block::Quote { inlines } => {
                    // 引用：整体左右缩进，文字加斜体。
                    let runs = build_runs(inlines, true);
                    self.write_body_paragraph("<w:ind w:left=\"420\" w:right=\"420\"/>", &runs)?;
                }
                Block::ListItem {
                    ordered,
                    index,
                    inlines,
                } => {
                    // 列表：整体左缩进，序号直接写进正文（不依赖 Word 的编号域，
                    // 免得不同 Word 版本渲染出不同的编号样式）。
                    let prefix = if *ordered {
                        format!("{index}. ")
                    } else {
                        "\u{2022} ".to_string()
                    };
                    let mut runs = plain_runs(&prefix);
                    runs.push_str(&build_runs(inlines, false));
                    self.write_body_paragraph("<w:ind w:left=\"420\"/>", &runs)?;
                }
                Block::Image { src, alt } => {
                    if !self.include_images {
                        stats.skipped_images += 1;
                        continue;
                    }
                    let Some(idx) = self.register_image(src)? else {
                        // 外链图 / 坏引用 / 素材已删：跳过并计数。
                        stats.skipped_images += 1;
                        continue;
                    };
                    // 按**图片块**计数：同一张图出现两次就算两张。
                    stats.included_images += 1;
                    let xml = self.build_image_paragraph(idx, alt);
                    self.write_str(&xml)?;
                    self.pending_blank = false;
                }
                Block::Rule => {
                    let xml = build_paragraph(
                        None,
                        "<w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"auto\"/></w:pBdr>",
                        "",
                    );
                    self.write_str(&xml)?;
                    self.pending_blank = false;
                }
            }
        }

        Ok(stats)
    }

    fn finish(&mut self) -> Result<(), String> {
        // 正文收尾：`w:sectPr` 必须是 `<w:body>` 的最后一个子元素。
        self.write_str(SECT_PR)?;
        self.write_str("</w:body></w:document>")?;

        // 文档属性里的作品标题要在解构 `self` **之前**取出来：
        // 解构是按字段移动，之后再读 `self` 就变成了「已移动值」。
        let title = self.novel_title().to_string();

        let DocxWriter {
            zip,
            assets,
            app_data_dir,
            images,
            ..
        } = self;

        // 媒体文件：同一张图只写一份（登记时已经去重）。
        for entry in images.iter() {
            let Some(asset) = assets.resolve(app_data_dir.as_path(), &entry.src) else {
                continue;
            };
            let Some(writer) = zip.as_mut() else {
                return Err("DOCX 写出器已关闭，无法写入媒体文件。".to_string());
            };
            writer
                .start_file(format!("word/media/{}", entry.media_name), file_options())
                .map_err(|e| format!("写入图片条目失败（{}）: {e}", entry.media_name))?;
            writer
                .write_all(&asset.bytes)
                .map_err(|e| format!("写入图片字节失败（{}）: {e}", entry.media_name))?;
        }

        // 关系表：样式 + 每张图片一个关系。
        let rels = build_document_rels(images);
        write_entry(&mut *zip, "word/_rels/document.xml.rels", rels.as_bytes())?;
        write_entry(&mut *zip, "word/styles.xml", STYLES_XML.as_bytes())?;
        write_entry(
            &mut *zip,
            "[Content_Types].xml",
            content_types_xml().as_bytes(),
        )?;
        write_entry(&mut *zip, "_rels/.rels", PACKAGE_RELS_XML.as_bytes())?;

        // 文档属性。
        let core = core_properties_xml(&title);
        write_entry(&mut *zip, "docProps/core.xml", core.as_bytes())?;
        write_entry(&mut *zip, "docProps/app.xml", APP_PROPERTIES_XML.as_bytes())?;

        // 收尾：写中央目录，把底层 writer 还回来。
        let writer = zip
            .take()
            .ok_or_else(|| "DOCX 写出器已关闭，无法收尾。".to_string())?;
        let mut inner = writer
            .finish()
            .map_err(|e| format!("收尾 DOCX 压缩包失败: {e}"))?;
        inner
            .flush()
            .map_err(|e| format!("写完 DOCX 压缩包失败: {e}"))?;
        Ok(())
    }
}

impl<W: Write + Seek> DocxWriter<W> {
    /// 作品标题（写进 `docProps/core.xml` 的 `dc:title`）。
    fn novel_title(&self) -> &str {
        &self.title
    }
}

/// 压缩方式：deflate。媒体文件（png/jpg）压不动多少，但正文压得动。
fn file_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// 写一个 zip 条目。
fn write_entry<W: Write + Seek>(
    zip: &mut Option<ZipWriter<W>>,
    name: &str,
    content: &[u8],
) -> Result<(), String> {
    let writer = zip
        .as_mut()
        .ok_or_else(|| format!("DOCX 写出器已关闭，无法写入条目 {name}。"))?;
    writer
        .start_file(name, file_options())
        .map_err(|e| format!("写入 DOCX 条目 {name} 失败: {e}"))?;
    writer
        .write_all(content)
        .map_err(|e| format!("写入 DOCX 条目 {name} 失败: {e}"))?;
    Ok(())
}

/// 生成一个段落的完整 XML。
///
/// `ppr_mid` 放段落属性里除 `w:pStyle` 之外的内容；`w:pPr` 的子元素有固定顺序
/// （`pStyle` → `pBdr` → `spacing` → `ind` → `jc`），顺序错了 Word 会拒绝打开。
fn build_paragraph(style: Option<&str>, ppr_mid: &str, runs: &str) -> String {
    let mut out = String::with_capacity(runs.len() + ppr_mid.len() + 96);
    out.push_str("<w:p>");
    let mut ppr = String::new();
    if let Some(style) = style {
        ppr.push_str(&format!("<w:pStyle w:val=\"{}\"/>", escape_xml(style)));
    }
    ppr.push_str(ppr_mid);
    if !ppr.is_empty() {
        out.push_str("<w:pPr>");
        out.push_str(&ppr);
        out.push_str("</w:pPr>");
    }
    out.push_str(runs);
    out.push_str("</w:p>");
    out
}

/// 生成一段不带样式的 run。
fn plain_runs(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 48);
    out.push_str("<w:r><w:t xml:space=\"preserve\">");
    out.push_str(&escape_xml(text));
    out.push_str("</w:t></w:r>");
    out
}

/// 生成行内内容的 run 序列。
///
/// `force_italic` 用于引用块：引文整体加斜体，与行内自带的斜体标记无关。
fn build_runs(inlines: &[Inline], force_italic: bool) -> String {
    let mut out = String::new();
    for inline in inlines {
        let Inline::Text {
            text,
            bold,
            italic,
            underline,
        } = inline;
        if text.is_empty() {
            continue;
        }
        let italic = *italic || force_italic;
        out.push_str("<w:r>");
        if *bold || italic || *underline {
            out.push_str("<w:rPr>");
            if *bold {
                out.push_str("<w:b/>");
            }
            if italic {
                out.push_str("<w:i/>");
            }
            if *underline {
                out.push_str("<w:u w:val=\"single\"/>");
            }
            out.push_str("</w:rPr>");
        }
        out.push_str("<w:t xml:space=\"preserve\">");
        out.push_str(&escape_xml(text));
        out.push_str("</w:t></w:r>");
    }
    out
}

/// 生成一张内联图片的段落。
fn build_image_xml(entry: &ImageEntry, alt: &str, doc_pr_id: u32) -> String {
    let descr = escape_xml(alt);
    let name = escape_xml(&entry.media_name);
    let rel = escape_xml(&entry.rel_id);
    let mut out = String::with_capacity(1024);
    out.push_str("<w:p><w:r><w:drawing>");
    out.push_str("<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">");
    out.push_str(&format!(
        "<wp:extent cx=\"{}\" cy=\"{}\"/>",
        entry.cx, entry.cy
    ));
    out.push_str("<wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>");
    out.push_str(&format!(
        "<wp:docPr id=\"{doc_pr_id}\" name=\"{name}\" descr=\"{descr}\"/>"
    ));
    out.push_str(
        "<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>",
    );
    out.push_str("<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">");
    out.push_str("<pic:pic><pic:nvPicPr>");
    out.push_str(&format!(
        "<pic:cNvPr id=\"{doc_pr_id}\" name=\"{name}\" descr=\"{descr}\"/><pic:cNvPicPr/>"
    ));
    out.push_str("</pic:nvPicPr><pic:blipFill>");
    out.push_str(&format!("<a:blip r:embed=\"{rel}\"/>"));
    out.push_str("<a:stretch><a:fillRect/></a:stretch></pic:blipFill>");
    out.push_str("<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/>");
    out.push_str(&format!(
        "<a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        entry.cx, entry.cy
    ));
    out.push_str("<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>");
    out.push_str("</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>");
    out
}

/// `word/_rels/document.xml.rels`：样式关系 + 每张图片一个关系。
fn build_document_rels(images: &[ImageEntry]) -> String {
    let mut out = String::with_capacity(256 + images.len() * 128);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    out.push_str("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    out.push_str(
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    );
    for entry in images {
        out.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/{}\"/>",
            escape_xml(&entry.rel_id),
            escape_xml(&entry.media_name)
        ));
    }
    out.push_str("</Relationships>");
    out
}

/// `[Content_Types].xml`。
///
/// 图片的 `Default` 必须按扩展名逐个声明：Word 不看字节内容，只按扩展名决定解码器。
fn content_types_xml() -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    out.push_str("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    out.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>");
    out.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    out.push_str("<Default Extension=\"png\" ContentType=\"image/png\"/>");
    out.push_str("<Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>");
    out.push_str("<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>");
    out.push_str("<Default Extension=\"webp\" ContentType=\"image/webp\"/>");
    out.push_str("<Default Extension=\"gif\" ContentType=\"image/gif\"/>");
    out.push_str("<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>");
    out.push_str("<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>");
    out.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
    out.push_str("<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>");
    out.push_str("</Types>");
    out
}

/// 包级关系：正文 + 文档属性。
const PACKAGE_RELS_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
    "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
    "<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>",
    "</Relationships>",
);

/// 扩展属性。页数与字数留 0：导出时算不准，写个假的反而误导人。
const APP_PROPERTIES_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"",
    " xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">",
    "<Application>Easy-writing</Application>",
    "<Pages>0</Pages><Words>0</Words>",
    "</Properties>",
);

/// 核心属性：标题、作者与创建/修改时间。
fn core_properties_xml(novel_title: &str) -> String {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut out = String::with_capacity(768);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    out.push_str("<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"");
    out.push_str(" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"");
    out.push_str(" xmlns:dcterms=\"http://purl.org/dc/terms/\"");
    out.push_str(" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\"");
    out.push_str(" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">");
    out.push_str(&format!("<dc:title>{}</dc:title>", escape_xml(novel_title)));
    out.push_str("<dc:creator>Easy-writing</dc:creator>");
    out.push_str("<cp:lastModifiedBy>Easy-writing</cp:lastModifiedBy>");
    out.push_str(&format!(
        "<dcterms:created xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:created>"
    ));
    out.push_str(&format!(
        "<dcterms:modified xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:modified>"
    ));
    out.push_str("</cp:coreProperties>");
    out
}

/// XML 文本转义。
///
/// 正文里的 `&`、`<`、`>` 一旦不转义，Word 会直接判定文件损坏 —— 作者写个
/// 「Tom & Jerry」就能让整本导出报废。
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 读取图片的像素尺寸。
///
/// 只认 png 与 jpeg：这两种的头部信息位置固定、能可靠解析。
/// 其它格式（webp / gif）返回 `None`，由调用方套用兜底尺寸 ——
/// 与其猜一个可能离谱的比例，不如给一个中性的尺寸。
pub fn image_size(bytes: &[u8], extension: &str) -> Option<(u32, u32)> {
    match extension.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => png_size(bytes),
        "jpg" | "jpeg" => jpeg_size(bytes),
        _ => None,
    }
}

/// PNG 尺寸：IHDR 块里，宽高各 4 字节大端。
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    // 8 字节签名 + 4 字节长度 + 4 字节 "IHDR" + 4 字节宽 + 4 字节高。
    if bytes.len() < 24 {
        return None;
    }
    if bytes[0..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    if bytes[12..16] != *b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((width, height))
}

/// JPEG 尺寸：扫描到 SOF0 / SOF1 / SOF2 帧头即得宽高。
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        // 填充字节：连续的 0xFF 属于同一个标记。
        if marker == 0xFF {
            i += 1;
            continue;
        }
        // 无载荷标记：直接跳过标记本身。
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            i += 2;
            continue;
        }
        let length = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) {
            // 帧头：1 字节精度 + 2 字节高 + 2 字节宽。
            if i + 9 > bytes.len() {
                return None;
            }
            let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return Some((width, height));
        }
        if length < 2 {
            return None;
        }
        i += 2 + length;
    }
    None
}

/// 像素尺寸 → 显示尺寸（EMU）。
///
/// 宽度上限 6 英寸，超出按原始宽高比缩放高度；宽或高为 0 时用兜底尺寸。
fn emu_size(bytes: &[u8], extension: &str) -> (u64, u64) {
    let (width, height) = image_size(bytes, extension)
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or(FALLBACK_IMAGE_SIZE);
    let cx = width as u64 * EMU_PER_PIXEL;
    let cy = height as u64 * EMU_PER_PIXEL;
    if cx > MAX_IMAGE_CX {
        // 用 u128 兜住乘法：极端长宽比下 `cy * MAX / cx` 会溢出 u64。
        let scaled = ((cy as u128) * (MAX_IMAGE_CX as u128) / (cx as u128)) as u64;
        (MAX_IMAGE_CX, scaled)
    } else {
        (cx, cy)
    }
}

/// 扩展名清洗：只保留字母数字，防止素材文件名里混进路径分隔符。
fn normalize_extension(extension: &str) -> String {
    let ext = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        ext
    } else {
        "png".to_string()
    }
}
