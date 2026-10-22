//! EPUB 写出器。
//!
//! ## 为什么自己拼 EPUB
//!
//! EPUB 本质上是一个 zip：几件固定的几件套（`mimetype` / `META-INF/container.xml` /
//! OPF / NCX / 各章 xhtml）。仓库里没有 `epub-builder` 之类的依赖，硬塞一个进来
//! 会带来体积与版本风险，而这些文件的形状其实很稳定 —— 自己拼反而能精确控制
//! 导出特有的行为：**图片只写一份、目录层级与卷结构一一对应、产物能被本仓库的
//! EPUB 导入器读回来**。
//!
//! 最后一条是关键：导出器与导入器在同一个仓库里，导出的 EPUB 如果连自己的
//! 导入器都读不回来，那它多半也读不进别的阅读器。所以测试直接拿
//! `crate::import::epub::parse_import_epub` 去读产物，而不是只断言「zip 能解开」。
//!
//! ## 包内结构
//!
//! ```text
//! mimetype                       第一个条目，且不压缩（EPUB 规范硬要求）
//! META-INF/container.xml         指向 OPF
//! EPUB/content.opf               清单与书脊
//! EPUB/nav.xhtml                 EPUB 3 目录
//! EPUB/toc.ncx                   EPUB 2 目录（阅读器兼容用）
//! EPUB/text/volume-0001.xhtml    卷分隔页
//! EPUB/text/chapter-0001.xhtml   章节
//! EPUB/images/img-0001.png        图片
//! ```
//!
//! ## 路径基准
//!
//! OPF 与 NCX 里的路径都相对 `EPUB/`（如 `text/chapter-0001.xhtml`），
//! 章节 xhtml 里的图片路径相对 `EPUB/text/`（如 `../images/img-0001.png`）。
//! 这不是随手写的 —— 本仓库的导入器（`import/epub.rs::resolve_zip_path`）
//! 就是按「基准目录 + 相对路径」拼的，写成绝对路径会拼出 `EPUB/EPUB/...`。
//!
//! ## 内存取舍
//!
//! 图片字节在 `finish()` 之前缓存在内存里（`AssetStore` 已按引用去重）。
//! 代价是「长篇 + 大量插图」时占用可观；换来的是同一张图在正文里出现多次
//! 也只写一份、也不必在写章节时反复解压素材。DOCX 写出器是同一取舍。

use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::PathBuf;

use shared_types::ParagraphSeparator;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::assets::AssetStore;
use super::doc::{Block, Inline};
use super::{ChapterOutput, ChapterStats, ExportWriter};

/// `mimetype` 条目的内容。EPUB 规范规定它必须是压缩包里的第一个条目，
/// 而且**不能压缩** —— 很多阅读器靠读它的头几个字节来嗅探格式。
const MIMETYPE_CONTENT: &str = "application/epub+zip";

/// OPF 所在目录。清单里的路径都相对它。
const OPF_DIR: &str = "EPUB";

/// OPF 文件名。
const OPF_PATH: &str = "EPUB/content.opf";

/// 已登记的一章。
struct ChapterEntry {
    /// manifest 里的 id。
    id: String,
    /// 相对 `EPUB/` 的路径。
    href: String,
    /// 章标题（进目录）。
    title: String,
}

/// 已登记的一卷。
struct VolumeEntry {
    id: String,
    href: String,
    title: String,
    /// 该卷第一章在 `chapters` 里的下标。
    ///
    /// 目录要按卷分组，就得有「哪一章是这一卷的开头」这个信息。
    /// 登记卷时顺手记下来，比事后反推可靠。
    first_chapter: usize,
}

/// 已登记的一张图片。
#[derive(Clone)]
struct ImageEntry {
    id: String,
    /// 相对 `EPUB/` 的路径。
    href: String,
    /// 原始引用（正文里的 `src`）。
    src: String,
    mime: String,
}

pub struct EpubWriter<W: Write + Seek> {
    zip: Option<ZipWriter<W>>,
    assets: AssetStore,
    app_data_dir: PathBuf,
    title: String,
    separator: ParagraphSeparator,
    include_images: bool,
    /// 书的唯一标识。EPUB 要求 `dc:identifier` 全局唯一，
    /// 这里用「标题 + 导出时刻」派生 —— 同一本书导两次会得到两个不同标识，
    /// 这正是我们想要的（两次导出是两份不同的书）。
    identifier: String,
    chapters: Vec<ChapterEntry>,
    volumes: Vec<VolumeEntry>,
    images: Vec<ImageEntry>,
    image_index: HashMap<String, usize>,
    /// 当前卷标题。用于判断「这一章是不是新卷的第一章」。
    current_volume: Option<String>,
    chapter_seq: u32,
    volume_seq: u32,
}

impl<W: Write + Seek> EpubWriter<W> {
    pub fn new(
        out: W,
        novel_title: &str,
        separator: ParagraphSeparator,
        include_images: bool,
        app_data_dir: PathBuf,
    ) -> Result<Self, String> {
        let mut zip = ZipWriter::new(out);

        // `mimetype` 必须是**第一个**条目且不压缩。
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored)
            .map_err(|e| format!("创建 EPUB mimetype 条目失败: {e}"))?;
        zip.write_all(MIMETYPE_CONTENT.as_bytes())
            .map_err(|e| format!("写入 EPUB mimetype 失败: {e}"))?;

        // `container.xml` 不依赖章节内容，可以现在就写。
        zip.start_file("META-INF/container.xml", deflated())
            .map_err(|e| format!("创建 EPUB container 条目失败: {e}"))?;
        zip.write_all(CONTAINER_XML.as_bytes())
            .map_err(|e| format!("写入 EPUB container.xml 失败: {e}"))?;

        let identifier = format!(
            "urn:easy-writing:{}:{}",
            sanitize_identifier(novel_title),
            chrono::Utc::now().timestamp_millis()
        );

        Ok(Self {
            zip: Some(zip),
            assets: AssetStore::new(),
            app_data_dir,
            title: novel_title.to_string(),
            separator,
            include_images,
            identifier,
            chapters: Vec::new(),
            volumes: Vec::new(),
            images: Vec::new(),
            image_index: HashMap::new(),
            current_volume: None,
            chapter_seq: 0,
            volume_seq: 0,
        })
    }

    /// 写入一个 zip 条目。
    fn write_entry(&mut self, name: &str, content: &[u8]) -> Result<(), String> {
        let zip = self
            .zip
            .as_mut()
            .ok_or_else(|| "EPUB 写出器已关闭，无法继续写入。".to_string())?;
        zip.start_file(name, deflated())
            .map_err(|e| format!("创建 EPUB 条目失败（{name}）: {e}"))?;
        zip.write_all(content)
            .map_err(|e| format!("写入 EPUB 条目失败（{name}）: {e}"))
    }

    /// 登记一张图片，返回它在 `images` 里的下标。
    ///
    /// 同一张图在正文里出现多次只登记一次：媒体文件写一份，
    /// manifest 里也只登记一个条目。
    fn register_image(&mut self, src: &str) -> Result<Option<usize>, String> {
        if let Some(&idx) = self.image_index.get(src) {
            return Ok(Some(idx));
        }
        let Some(asset) = self.assets.resolve(&self.app_data_dir, src) else {
            return Ok(None);
        };
        let extension = normalize_extension(&asset.extension);
        let mime = if asset.mime.is_empty() {
            mime_of(&extension)
        } else {
            asset.mime.clone()
        };
        let idx = self.images.len();
        self.images.push(ImageEntry {
            id: format!("img-{}", idx + 1),
            href: format!("images/img-{:04}.{}", idx + 1, extension),
            src: src.to_string(),
            mime,
        });
        self.image_index.insert(src.to_string(), idx);
        Ok(Some(idx))
    }
}

impl<W: Write + Seek> ExportWriter for EpubWriter<W> {
    fn write_chapter(&mut self, chapter: &ChapterOutput) -> Result<ChapterStats, String> {
        let mut stats = ChapterStats::default();

        // 换卷：写一张卷分隔页。
        //
        // 卷标题**不**放进章节 xhtml —— 放在那里的话，导入器会把「卷标题」
        // 当成这一章的标题，卷结构就丢了。单独一张只有卷名的分隔页，
        // 既保住了目录里的层级，也不会污染章节内容。
        if let Some(volume_title) = &chapter.volume_title {
            let volume_title = volume_title.trim();
            if !volume_title.is_empty() && self.current_volume.as_deref() != Some(volume_title) {
                self.volume_seq += 1;
                let seq = self.volume_seq;
                let href = format!("text/volume-{:04}.xhtml", seq);
                let body = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE html>\n\
                     <html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"zh-CN\" lang=\"zh-CN\">\n\
                     <head><meta charset=\"utf-8\"/><title>{}</title></head>\n\
                     <body><h1 class=\"volume-title\">{}</h1></body>\n\
                     </html>\n",
                    escape_xml(volume_title),
                    escape_xml(volume_title),
                );
                self.write_entry(&format!("{}/{}", OPF_DIR, href), body.as_bytes())?;
                self.volumes.push(VolumeEntry {
                    id: format!("vol-{seq}"),
                    href,
                    title: volume_title.to_string(),
                    first_chapter: self.chapters.len(),
                });
                self.current_volume = Some(volume_title.to_string());
            }
        }

        self.chapter_seq += 1;
        let seq = self.chapter_seq;
        let href = format!("text/chapter-{:04}.xhtml", seq);
        let body = self.build_chapter_xhtml(chapter, &mut stats)?;
        self.write_entry(&format!("{}/{}", OPF_DIR, href), body.as_bytes())?;
        self.chapters.push(ChapterEntry {
            id: format!("ch-{seq}"),
            href,
            title: chapter.title.clone(),
        });

        Ok(stats)
    }

    fn finish(&mut self) -> Result<(), String> {
        // 图片：同一张图只写一份。
        for entry in self.images.clone() {
            let Some(asset) = self.assets.resolve(&self.app_data_dir, &entry.src) else {
                continue;
            };
            let path = format!("{}/{}", OPF_DIR, entry.href);
            let zip = self
                .zip
                .as_mut()
                .ok_or_else(|| "EPUB 写出器已关闭，无法写入图片。".to_string())?;
            zip.start_file(&path, deflated())
                .map_err(|e| format!("创建 EPUB 图片条目失败（{}）: {e}", entry.href))?;
            zip.write_all(&asset.bytes)
                .map_err(|e| format!("写入 EPUB 图片失败（{}）: {e}", entry.href))?;
        }

        // 目录与清单。
        let nav = self.build_nav_xhtml();
        self.write_entry(&format!("{}/nav.xhtml", OPF_DIR), nav.as_bytes())?;
        let ncx = self.build_ncx();
        self.write_entry(&format!("{}/toc.ncx", OPF_DIR), ncx.as_bytes())?;
        let opf = self.build_opf();
        self.write_entry(OPF_PATH, opf.as_bytes())?;

        // 收尾：写中央目录，把底层 writer 还回来。
        let writer = self
            .zip
            .take()
            .ok_or_else(|| "EPUB 写出器已关闭，无法收尾。".to_string())?;
        let mut inner = writer
            .finish()
            .map_err(|e| format!("收尾 EPUB 压缩包失败: {e}"))?;
        inner
            .flush()
            .map_err(|e| format!("写完 EPUB 压缩包失败: {e}"))?;
        Ok(())
    }
}

impl<W: Write + Seek> EpubWriter<W> {
    /// 生成一章的 xhtml。
    fn build_chapter_xhtml(
        &mut self,
        chapter: &ChapterOutput,
        stats: &mut ChapterStats,
    ) -> Result<String, String> {
        let mut out = String::with_capacity(4096);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<!DOCTYPE html>\n");
        out.push_str(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"zh-CN\" lang=\"zh-CN\">\n",
        );
        out.push_str("<head>\n");
        out.push_str("<meta charset=\"utf-8\"/>\n");
        out.push_str(&format!("<title>{}</title>\n", escape_xml(&chapter.title)));
        out.push_str(&format!("<style type=\"text/css\">\n{}</style>\n", self.css()));
        out.push_str("</head>\n<body>\n");
        if !chapter.title.trim().is_empty() {
            out.push_str(&format!(
                "<h1 class=\"chapter-title\">{}</h1>\n",
                escape_xml(&chapter.title)
            ));
        }

        // 列表：连续的列表项合并进同一个 <ol>/<ul>。
        let mut open_list: Option<bool> = None;
        for block in &chapter.blocks {
            match block {
                Block::Heading { level, inlines } => {
                    close_list(&mut out, &mut open_list);
                    // 章标题占了 h1，章内小标题从 h2 起。
                    let tag = (*level).saturating_add(1).clamp(2, 6);
                    out.push_str(&format!(
                        "<h{tag}>{}</h{tag}>\n",
                        escape_xml(&inlines_text(inlines))
                    ));
                }
                Block::Paragraph { inlines } => {
                    close_list(&mut out, &mut open_list);
                    let text = inlines_text(inlines);
                    if !text.is_empty() {
                        out.push_str(&format!("<p>{}</p>\n", self.runs(inlines)));
                    }
                }
                Block::Quote { inlines } => {
                    close_list(&mut out, &mut open_list);
                    let text = inlines_text(inlines);
                    if !text.is_empty() {
                        out.push_str(&format!(
                            "<blockquote><p>{}</p></blockquote>\n",
                            self.runs(inlines)
                        ));
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
                    match open_list {
                        Some(open_ordered) if open_ordered == *ordered => {}
                        Some(_) => {
                            close_list(&mut out, &mut open_list);
                            out.push_str(if *ordered { "<ol>\n" } else { "<ul>\n" });
                            open_list = Some(*ordered);
                        }
                        None => {
                            out.push_str(if *ordered { "<ol>\n" } else { "<ul>\n" });
                            open_list = Some(*ordered);
                        }
                    }
                    if *ordered {
                        out.push_str(&format!("<li>{}</li>\n", self.runs(inlines)));
                    } else {
                        // 无序列表的序号没有意义，直接丢弃。
                        out.push_str(&format!("<li>{}</li>\n", self.runs(inlines)));
                    }
                    let _ = index;
                }
                Block::Image { src, alt } => {
                    close_list(&mut out, &mut open_list);
                    if !self.include_images {
                        stats.skipped_images += 1;
                        continue;
                    }
                    let Some(idx) = self.register_image(src)? else {
                        // 外链图 / 坏引用 / 素材已删：跳过并计数。
                        stats.skipped_images += 1;
                        continue;
                    };
                    stats.included_images += 1;
                    // 章节 xhtml 在 `EPUB/text/` 下，图片在 `EPUB/images/` 下。
                    out.push_str(&format!(
                        "<p class=\"figure\"><img src=\"../images/{}\" alt=\"{}\"/></p>\n",
                        self.images[idx].href.rsplit('/').next().unwrap_or_default(),
                        escape_xml(alt)
                    ));
                }
                Block::Rule => {
                    close_list(&mut out, &mut open_list);
                    out.push_str("<hr/>\n");
                }
            }
        }
        close_list(&mut out, &mut open_list);

        out.push_str("</body>\n</html>\n");
        Ok(out)
    }

    /// 行内样式。
    fn runs(&self, inlines: &[Inline]) -> String {
        let mut out = String::new();
        for inline in inlines {
            let Inline::Text {
                text,
                bold,
                italic,
                underline,
            } = inline;
            let mut piece = escape_xml(text);
            if *italic {
                piece = format!("<em>{}</em>", piece);
            }
            if *bold {
                piece = format!("<strong>{}</strong>", piece);
            }
            if *underline {
                // XHTML 里 <u> 不合法，用 CSS 下划线代替。
                piece = format!("<span style=\"text-decoration:underline\">{}</span>", piece);
            }
            out.push_str(&piece);
        }
        out
    }

    /// 段落排版样式。
    ///
    /// 两种分隔方式都用 CSS 表达，而不是往正文里塞全角空格 ——
    /// 后者会让「导出字数」比编辑器里看到的多两个字，与任务书
    /// 「导出字数与应用统计一致」的要求冲突。
    fn css(&self) -> String {
        // 这里刻意用 `push_str` 拼而不是 `format!`：
        // CSS 里全是花括号，走格式串就得把它们全写成 `{{`/`}}`，
        // 可读性会掉一个档次。
        let mut out = String::with_capacity(512);
        out.push_str("body { line-height: 1.8; }\n");
        match self.separator {
            ParagraphSeparator::Indent => {
                out.push_str("p { text-indent: 2em; margin: 0 0 0.5em 0; }\n");
            }
            ParagraphSeparator::BlankLine => {
                out.push_str("p { text-indent: 0; margin: 0 0 1.4em 0; }\n");
            }
        }
        out.push_str("h1 { font-size: 1.6em; margin: 1.2em 0 0.8em 0; }\n");
        out.push_str("h2 { font-size: 1.3em; margin: 1em 0 0.6em 0; }\n");
        out.push_str("blockquote { margin: 0.8em 2em; font-style: italic; }\n");
        out.push_str("p.figure { text-indent: 0; text-align: center; }\n");
        out.push_str("img { max-width: 100%; height: auto; }\n");
        out
    }

    /// EPUB 3 目录。
    fn build_nav_xhtml(&self) -> String {
        let mut out = String::with_capacity(2048);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<!DOCTYPE html>\n");
        out.push_str(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"zh-CN\" lang=\"zh-CN\">\n",
        );
        out.push_str("<head><meta charset=\"utf-8\"/><title>目录</title></head>\n");
        out.push_str("<body>\n<nav epub:type=\"toc\" id=\"toc\">\n<h1>目录</h1>\n<ol>\n");

        // 卷 → 章两级。NCX 才是老阅读器的主路径，这里保持同样的层级。
        let mut current_volume: Option<usize> = None;
        for (index, chapter) in self.chapters.iter().enumerate() {
            let volume = self.volume_of_chapter(index);
            if volume != current_volume {
                if current_volume.is_some() {
                    out.push_str("</ol>\n</li>\n");
                }
                if let Some(vol) = volume {
                    let entry = &self.volumes[vol];
                    out.push_str(&format!(
                        "<li><span>{}</span>\n<ol>\n",
                        escape_xml(&entry.title)
                    ));
                }
                current_volume = volume;
            }
            out.push_str(&format!(
                "<li><a href=\"{}\">{}</a></li>\n",
                escape_xml(&chapter.href),
                escape_xml(&chapter.title)
            ));
        }
        if current_volume.is_some() {
            out.push_str("</ol>\n</li>\n");
        }

        out.push_str("</ol>\n</nav>\n</body>\n</html>\n");
        out
    }

    /// 某一章属于哪一卷（`volumes` 的下标）。
    fn volume_of_chapter(&self, chapter_index: usize) -> Option<usize> {
        let mut found = None;
        for (index, volume) in self.volumes.iter().enumerate() {
            if volume.first_chapter <= chapter_index {
                found = Some(index);
            } else {
                break;
            }
        }
        found
    }

    /// EPUB 2 目录（NCX）。
    fn build_ncx(&self) -> String {
        let mut out = String::with_capacity(4096);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\" xml:lang=\"zh-CN\">\n");
        out.push_str("<head>\n");
        out.push_str(&format!(
            "<meta name=\"dtb:uid\" content=\"{}\"/>\n",
            escape_xml(&self.identifier)
        ));
        out.push_str("<meta name=\"dtb:depth\" content=\"2\"/>\n");
        out.push_str("</head>\n");
        out.push_str(&format!(
            "<docTitle><text>{}</text></docTitle>\n",
            escape_xml(&self.title)
        ));
        out.push_str("<navMap>\n");

        // 卷 → 章：卷是父节点，章是它的子节点。
        // 本仓库的 EPUB 导入器靠这个层级还原「卷 → 章」结构，
        // 扁平目录会被它当成「没有分卷」。
        let mut play_order = 1usize;
        let mut current_volume: Option<usize> = None;
        for (index, chapter) in self.chapters.iter().enumerate() {
            let volume = self.volume_of_chapter(index);
            if volume != current_volume {
                if current_volume.is_some() {
                    out.push_str("</navPoint>\n");
                }
                if let Some(vol) = volume {
                    let entry = &self.volumes[vol];
                    out.push_str(&format!(
                        "<navPoint id=\"{}\" playOrder=\"{}\">\n<navLabel><text>{}</text></navLabel>\n<content src=\"{}\"/>\n",
                        escape_xml(&entry.id),
                        play_order,
                        escape_xml(&entry.title),
                        escape_xml(&entry.href)
                    ));
                    play_order += 1;
                }
                current_volume = volume;
            }
            out.push_str(&format!(
                "<navPoint id=\"{}\" playOrder=\"{}\">\n<navLabel><text>{}</text></navLabel>\n<content src=\"{}\"/>\n</navPoint>\n",
                escape_xml(&chapter.id),
                play_order,
                escape_xml(&chapter.title),
                escape_xml(&chapter.href)
            ));
            play_order += 1;
        }
        if current_volume.is_some() {
            out.push_str("</navPoint>\n");
        }

        out.push_str("</navMap>\n</ncx>\n");
        out
    }

    /// OPF 清单。
    fn build_opf(&self) -> String {
        let mut out = String::with_capacity(8192);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n");
        out.push_str("<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:opf=\"http://www.idpf.org/2007/opf\">\n");
        out.push_str(&format!(
            "<dc:identifier id=\"bookid\">{}</dc:identifier>\n",
            escape_xml(&self.identifier)
        ));
        out.push_str(&format!(
            "<dc:title>{}</dc:title>\n",
            escape_xml(&self.title)
        ));
        out.push_str("<dc:language>zh-CN</dc:language>\n");
        out.push_str(&format!(
            "<meta property=\"dcterms:modified\">{}</meta>\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
        ));
        out.push_str("</metadata>\n");

        out.push_str("<manifest>\n");
        out.push_str("<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n");
        out.push_str("<item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n");
        for volume in &self.volumes {
            out.push_str(&format!(
                "<item id=\"{}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>\n",
                escape_xml(&volume.id),
                escape_xml(&volume.href)
            ));
        }
        for chapter in &self.chapters {
            out.push_str(&format!(
                "<item id=\"{}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>\n",
                escape_xml(&chapter.id),
                escape_xml(&chapter.href)
            ));
        }
        for image in &self.images {
            out.push_str(&format!(
                "<item id=\"{}\" href=\"{}\" media-type=\"{}\"/>\n",
                escape_xml(&image.id),
                escape_xml(&image.href),
                escape_xml(&image.mime)
            ));
        }
        out.push_str("</manifest>\n");

        out.push_str("<spine toc=\"ncx\">\n");
        for chapter in &self.chapters {
            out.push_str(&format!(
                "<itemref idref=\"{}\"/>\n",
                escape_xml(&chapter.id)
            ));
        }
        out.push_str("</spine>\n</package>\n");
        out
    }
}

/// 压缩方式：deflate。
fn deflated() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// `container.xml`：指向 OPF。
const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="EPUB/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#;

/// XML 转义。
///
/// 正文里出现 `&`、`<` 是常态（书名《活着 & 兄弟》），不转义就会产出坏 XML，
/// 阅读器直接拒绝打开。
fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
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

/// 行内片段拼成纯文本。
fn inlines_text(inlines: &[Inline]) -> String {
    inlines.iter().map(|i| i.plain()).collect()
}

/// 关闭尚未闭合的列表。
fn close_list(out: &mut String, open_list: &mut Option<bool>) {
    if let Some(ordered) = open_list.take() {
        out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
    }
}

/// 扩展名清洗：只保留字母数字。
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

/// 由扩展名推断 MIME。
fn mime_of(extension: &str) -> String {
    match extension {
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "webp" => "image/webp".to_string(),
        "gif" => "image/gif".to_string(),
        _ => "image/png".to_string(),
    }
}

/// 标识符清洗：只保留字母数字与短横线。
fn sanitize_identifier(text: &str) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(48)
        .collect();
    if cleaned.is_empty() {
        "novel".to_string()
    } else {
        cleaned
    }
}
