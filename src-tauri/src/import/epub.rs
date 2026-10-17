use crate::import::common::{
    clean_file_name_to_title, finalize_chapter, is_chapter_header, is_volume_header, unescape_html,
};
use regex::Regex;
use shared_types::{ImportChapterPreview, ImportPreview, ImportVolumePreview};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// 解码 URL 编码路径（例如 %20 还原为空格）
fn url_decode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                let hex_str = format!("{}{}", h1, h2);
                if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                    out.push(byte as char);
                    continue;
                }
                out.push('%');
                out.push(h1);
                out.push(h2);
            } else {
                out.push('%');
                if let Some(h) = h1 {
                    out.push(h);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 解析 Zip 内部相对路径
fn resolve_zip_path(base_dir: &str, href: &str) -> String {
    let clean_href = href.split(['#', '?']).next().unwrap_or(href);
    let decoded = url_decode(clean_href);

    let combined = if decoded.starts_with('/') {
        decoded.trim_start_matches('/').to_string()
    } else {
        format!("{}{}", base_dir, decoded)
    };

    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        } else if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    parts.join("/")
}

/// 从 HTML 源码中提取纯净的段落文本行与可能存在的显式主标题
fn html_to_paragraphs(html: &str) -> (Option<String>, Vec<String>) {
    // 移除 head, script, style
    let head_re = Regex::new(r"(?is)<head[^>]*>.*?</head>").unwrap();
    let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();

    let cleaned = head_re.replace_all(html, "");
    let cleaned = script_re.replace_all(&cleaned, "");
    let cleaned = style_re.replace_all(&cleaned, "");

    // 尝试提取 h1 或 h2 作为主标题
    let heading_re = Regex::new(r"(?is)<h[1-2][^>]*>(.*?)</h[1-2]>").unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let ws_re = Regex::new(r"\s+").unwrap();
    let mut detected_heading = None;
    if let Some(cap) = heading_re.captures(&cleaned) {
        let raw_h = &cap[1];
        let plain_h = tag_re.replace_all(raw_h, " ").to_string();
        let unescaped = unescape_html(plain_h.trim());
        let normalized = ws_re.replace_all(&unescaped, " ").trim().to_string();
        if !normalized.is_empty() && normalized.chars().count() <= 60 {
            detected_heading = Some(normalized);
        }
    }

    // 将显式标题从正文清理中移除，避免标题重复出现在正文第一行
    let cleaned_without_heading = if detected_heading.is_some() {
        heading_re.replace(&cleaned, "").to_string()
    } else {
        cleaned.to_string()
    };

    // 块级标签替换为换行符
    let br_re = Regex::new(r"(?i)<br\s*/?>").unwrap();
    let block_re = Regex::new(r"(?i)</?(p|div|h[1-6]|li|blockquote|section|article)\b[^>]*>").unwrap();

    let with_breaks = br_re.replace_all(&cleaned_without_heading, "\n");
    let with_blocks = block_re.replace_all(&with_breaks, "\n\n");
    let strip_all = tag_re.replace_all(&with_blocks, "");

    let mut paragraphs = Vec::new();
    for raw_line in strip_all.lines() {
        let unescaped = unescape_html(raw_line).trim().to_string();
        if !unescaped.is_empty() {
            paragraphs.push(unescaped);
        }
    }

    (detected_heading, paragraphs)
}

/// 树形导航节点（支持递归包含子章节）
#[derive(Debug, Clone)]
struct NavNode {
    title: String,
    src: String,
    children: Vec<NavNode>,
}

/// 基于栈的 EPUB 2 NCX 递归导航解析器
fn parse_ncx_nav(ncx_xml: &str, ncx_dir: &str) -> Vec<NavNode> {
    let tag_re = Regex::new(
        r#"(?is)(<navPoint\b[^>]*>|</navPoint>|<text[^>]*>(.*?)</text>|<content\b[^>]*src\s*=\s*["']([^"']+)["'][^>]*>)"#,
    )
    .unwrap();

    let mut stack: Vec<NavNode> = Vec::new();
    let mut roots: Vec<NavNode> = Vec::new();

    for cap in tag_re.captures_iter(ncx_xml) {
        let full = &cap[0];
        let lower = full.to_ascii_lowercase();
        if lower.starts_with("<navpoint") {
            let node = NavNode {
                title: String::new(),
                src: String::new(),
                children: Vec::new(),
            };
            stack.push(node);
        } else if lower.starts_with("</navpoint") {
            if let Some(finished_node) = stack.pop() {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(finished_node);
                } else {
                    roots.push(finished_node);
                }
            }
        } else if lower.starts_with("<text") {
            if let Some(node) = stack.last_mut() {
                if let Some(c) = cap.get(2) {
                    node.title = unescape_html(c.as_str().trim());
                }
            }
        } else if lower.starts_with("<content") {
            if let Some(node) = stack.last_mut() {
                if let Some(c) = cap.get(3) {
                    let clean_src = c.as_str().split(['#', '?']).next().unwrap_or(c.as_str()).trim();
                    node.src = resolve_zip_path(ncx_dir, clean_src);
                }
            }
        }
    }

    roots
}

/// 解析 EPUB 3 Nav XHTML 中的目录链接
fn parse_nav_xhtml(nav_xml: &str, nav_dir: &str) -> Vec<NavNode> {
    let a_re = Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#).unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let mut nodes = Vec::new();

    for a_cap in a_re.captures_iter(nav_xml) {
        let href = &a_cap[1];
        let raw_text = &a_cap[2];
        let plain = tag_re.replace_all(raw_text, "").to_string();
        let title = unescape_html(plain.trim());
        let clean_href = href.split(['#', '?']).next().unwrap_or(href).trim();
        let full_p = resolve_zip_path(nav_dir, clean_href);

        if !title.is_empty() {
            nodes.push(NavNode {
                title,
                src: full_p,
                children: Vec::new(),
            });
        }
    }

    nodes
}

/// 递归将目录树拍平为路径到标题的映射字典
fn collect_toc_titles(nodes: &[NavNode], map: &mut HashMap<String, String>) {
    for node in nodes {
        if !node.src.is_empty() && !node.title.is_empty() {
            let clean_src = node.src.split(['#', '?']).next().unwrap_or(&node.src);
            map.entry(clean_src.to_string()).or_insert_with(|| node.title.clone());
        }
        collect_toc_titles(&node.children, map);
    }
}



/// 解析 EPUB 二进制数据并生成分卷分章预览结构
pub fn parse_import_epub(file_name: &str, data: &[u8]) -> Result<ImportPreview, String> {
    let mut archive = ZipArchive::new(Cursor::new(data))
        .map_err(|e| format!("无法解压 EPUB 文件（非有效 ZIP 归档）：{}", e))?;

    // 1. 读取 META-INF/container.xml 寻找 OPF 路径
    let opf_path = {
        let container_str = if let Ok(mut container_file) = archive.by_name("META-INF/container.xml") {
            let mut s = String::new();
            if container_file.read_to_string(&mut s).is_ok() {
                Some(s)
            } else {
                None
            }
        } else {
            None
        };

        let mut path = None;
        if let Some(s) = container_str {
            let rootfile_re = Regex::new(r#"full-path\s*=\s*["']([^"']+)["']"#).unwrap();
            if let Some(cap) = rootfile_re.captures(&s) {
                path = Some(cap[1].to_string());
            }
        }

        if let Some(p) = path {
            p
        } else {
            // 回退：在压缩包中查找任意 .opf 文件
            let mut found = None;
            for i in 0..archive.len() {
                if let Ok(f) = archive.by_index(i) {
                    if let Ok(name) = f.name() {
                        if name.ends_with(".opf") {
                            found = Some(name.to_string());
                            break;
                        }
                    }
                }
            }
            found.ok_or_else(|| "未找到 EPUB 核心 OPF 包文件".to_string())?
        }
    };

    let opf_dir = if let Some(idx) = opf_path.rfind('/') {
        opf_path[..idx + 1].to_string()
    } else {
        String::new()
    };

    // 2. 读取 OPF 文件内容
    let opf_content = {
        let mut opf_file = archive
            .by_name(&opf_path)
            .map_err(|_| format!("找不到指定的 OPF 文件: {}", opf_path))?;
        let mut s = String::new();
        opf_file
            .read_to_string(&mut s)
            .map_err(|e| format!("读取 OPF 文件失败: {}", e))?;
        s
    };

    // 3. 提取作品标题
    let title_re = Regex::new(r"(?is)<dc:title[^>]*>(.*?)</dc:title>").unwrap();
    let mut novel_title = if let Some(cap) = title_re.captures(&opf_content) {
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let plain = tag_re.replace_all(&cap[1], "").to_string();
        unescape_html(plain.trim())
    } else {
        String::new()
    };

    if novel_title.is_empty() {
        novel_title = clean_file_name_to_title(file_name);
    }

    // 4. 解析 Manifest (id -> (full_path, media_type))
    struct ManifestItem {
        full_path: String,
        media_type: String,
    }

    let item_re = Regex::new(r#"(?is)<item\b([^>]*)/?>"#).unwrap();
    let id_attr_re = Regex::new(r#"id\s*=\s*["']([^"']+)["']"#).unwrap();
    let href_attr_re = Regex::new(r#"href\s*=\s*["']([^"']+)["']"#).unwrap();
    let media_attr_re = Regex::new(r#"media-type\s*=\s*["']([^"']+)["']"#).unwrap();
    let props_attr_re = Regex::new(r#"properties\s*=\s*["']([^"']+)["']"#).unwrap();

    let mut manifest: HashMap<String, ManifestItem> = HashMap::new();
    let mut toc_ncx_path: Option<String> = None;
    let mut nav_xhtml_path: Option<String> = None;

    for cap in item_re.captures_iter(&opf_content) {
        let attrs = &cap[1];
        let id = id_attr_re.captures(attrs).map(|c| c[1].to_string());
        let href = href_attr_re.captures(attrs).map(|c| c[1].to_string());
        let media_type = media_attr_re
            .captures(attrs)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let properties = props_attr_re
            .captures(attrs)
            .map(|c| c[1].to_string())
            .unwrap_or_default();

        if let (Some(id), Some(href)) = (id, href) {
            let full_path = resolve_zip_path(&opf_dir, &href);
            if media_type == "application/x-dtbncx+xml" || id == "ncx" || full_path.ends_with(".ncx") {
                toc_ncx_path = Some(full_path.clone());
            }
            if properties.contains("nav") || full_path.ends_with("nav.xhtml") {
                nav_xhtml_path = Some(full_path.clone());
            }
            manifest.insert(
                id,
                ManifestItem {
                    full_path,
                    media_type,
                },
            );
        }
    }

    // 5. 解析目录树（优先 NCX，次选 Nav XHTML）
    let mut toc_roots: Vec<NavNode> = Vec::new();

    if let Some(ncx_p) = &toc_ncx_path {
        if let Ok(mut file) = archive.by_name(ncx_p) {
            let mut ncx_xml = String::new();
            if file.read_to_string(&mut ncx_xml).is_ok() {
                let ncx_dir = if let Some(idx) = ncx_p.rfind('/') {
                    &ncx_p[..idx + 1]
                } else {
                    ""
                };
                toc_roots = parse_ncx_nav(&ncx_xml, ncx_dir);
            }
        }
    }

    if toc_roots.is_empty() {
        if let Some(nav_p) = &nav_xhtml_path {
            if let Ok(mut file) = archive.by_name(nav_p) {
                let mut nav_xml = String::new();
                if file.read_to_string(&mut nav_xml).is_ok() {
                    let nav_dir = if let Some(idx) = nav_p.rfind('/') {
                        &nav_p[..idx + 1]
                    } else {
                        ""
                    };
                    toc_roots = parse_nav_xhtml(&nav_xml, nav_dir);
                }
            }
        }
    }

    let mut toc_titles: HashMap<String, String> = HashMap::new();
    collect_toc_titles(&toc_roots, &mut toc_titles);

    let has_hierarchical_toc = toc_roots.iter().any(|r| !r.children.is_empty());

    let mut volumes: Vec<ImportVolumePreview> = Vec::new();

    if has_hierarchical_toc {
        // ========== 模式 A：基于严谨树形目录结构提取 ==========
        let mut front_chapters: Vec<ImportChapterPreview> = Vec::new();

        for root in toc_roots {
            if !root.children.is_empty() {
                // 若之前暂存了独立前置章（如版本制作、内容简介），则将其归档为前言/作品相关卷
                if !front_chapters.is_empty() {
                    volumes.push(ImportVolumePreview {
                        title: "作品相关".to_string(),
                        chapters: std::mem::take(&mut front_chapters),
                    });
                }

                let mut vol_chapters = Vec::new();

                // 检查分卷封面/过渡文档本身是否包含独立有效说明文字（非仅单一卷名标题）
                if !root.src.is_empty() {
                    if let Ok(mut file) = archive.by_name(&root.src) {
                        let mut html = String::new();
                        if file.read_to_string(&mut html).is_ok() {
                            let (_, paras) = html_to_paragraphs(&html);
                            if !paras.is_empty() {
                                let is_just_vol_title = paras.len() == 1
                                    && (paras[0].trim() == root.title.trim()
                                        || is_volume_header(&paras[0]));
                                if !is_just_vol_title {
                                    if let Some(ch) = finalize_chapter("卷首语", &paras) {
                                        vol_chapters.push(ch);
                                    }
                                }
                            }
                        }
                    }
                }

                // 提取属于该卷的所有子章节
                for child in root.children {
                    if child.src.is_empty() {
                        continue;
                    }
                    if let Ok(mut file) = archive.by_name(&child.src) {
                        let mut html = String::new();
                        if file.read_to_string(&mut html).is_ok() {
                            let (detected_h, mut paras) = html_to_paragraphs(&html);
                            let chap_title = if !child.title.is_empty() {
                                child.title.clone()
                            } else if let Some(h) = detected_h {
                                h
                            } else {
                                format!("第{}章", vol_chapters.len() + 1)
                            };

                            // 去除首段与章节标题的冗余重复
                            if let Some(first_p) = paras.first() {
                                let clean_first = first_p.replace(char::is_whitespace, "");
                                let clean_title = chap_title.replace(char::is_whitespace, "");
                                if clean_first == clean_title {
                                    paras.remove(0);
                                }
                            }

                            if let Some(ch) = finalize_chapter(&chap_title, &paras) {
                                vol_chapters.push(ch);
                            }
                        }
                    }
                }

                if !vol_chapters.is_empty() {
                    volumes.push(ImportVolumePreview {
                        title: root.title,
                        chapters: vol_chapters,
                    });
                }
            } else {
                // 顶层单叶子节点（如 "制作设计", "内容简介", "后记"）
                if !root.src.is_empty() {
                    if let Ok(mut file) = archive.by_name(&root.src) {
                        let mut html = String::new();
                        if file.read_to_string(&mut html).is_ok() {
                            let (detected_h, mut paras) = html_to_paragraphs(&html);
                            let chap_title = if !root.title.is_empty() {
                                root.title.clone()
                            } else if let Some(h) = detected_h {
                                h
                            } else {
                                "章节".to_string()
                            };

                            if let Some(first_p) = paras.first() {
                                let clean_first = first_p.replace(char::is_whitespace, "");
                                let clean_title = chap_title.replace(char::is_whitespace, "");
                                if clean_first == clean_title {
                                    paras.remove(0);
                                }
                            }

                            if let Some(ch) = finalize_chapter(&chap_title, &paras) {
                                if let Some(last_vol) = volumes.last_mut() {
                                    last_vol.chapters.push(ch);
                                } else {
                                    front_chapters.push(ch);
                                }
                            }
                        }
                    }
                }
            }
        }

        if !front_chapters.is_empty() {
            volumes.push(ImportVolumePreview {
                title: "作品相关".to_string(),
                chapters: front_chapters,
            });
        }
    } else {
        // ========== 模式 B：回退扁平 Spine 顺序扫描 ==========
        let spine_re = Regex::new(r#"(?is)<spine\b[^>]*>(.*?)</spine>"#).unwrap();
        let itemref_re = Regex::new(r#"idref\s*=\s*["']([^"']+)["']"#).unwrap();

        let spine_items: Vec<String> = if let Some(cap) = spine_re.captures(&opf_content) {
            itemref_re
                .captures_iter(&cap[1])
                .map(|c| c[1].to_string())
                .collect()
        } else {
            Vec::new()
        };

        let mut current_vol_title = "正文卷".to_string();
        let mut current_vol_chapters: Vec<ImportChapterPreview> = Vec::new();
        let mut chapter_seq = 1;

        for idref in spine_items {
            let Some(item) = manifest.get(&idref) else {
                continue;
            };

            let is_html = item.media_type.contains("html")
                || item.full_path.ends_with(".xhtml")
                || item.full_path.ends_with(".html")
                || item.full_path.ends_with(".htm");

            if !is_html {
                continue;
            }

            let html_content = {
                let Ok(mut file) = archive.by_name(&item.full_path) else {
                    continue;
                };
                let mut s = String::new();
                if file.read_to_string(&mut s).is_err() {
                    continue;
                }
                s
            };

            let (heading_opt, mut paragraphs) = html_to_paragraphs(&html_content);
            let file_title_opt = toc_titles
                .get(&item.full_path)
                .cloned()
                .or(heading_opt.clone());

            // 检查文档是否为纯分卷说明文档（如只有一行 "第一卷 龙兴微末" 或仅含卷标题 h1）
            let is_vol_header_doc = if paragraphs.is_empty() {
                if let Some(h) = &heading_opt {
                    is_volume_header(h)
                } else {
                    false
                }
            } else if paragraphs.len() <= 2 {
                let first_line = paragraphs[0].trim();
                is_volume_header(first_line)
                    || (file_title_opt.as_deref().map(|t| is_volume_header(t)).unwrap_or(false))
            } else {
                false
            };

            if is_vol_header_doc {
                if !current_vol_chapters.is_empty() {
                    volumes.push(ImportVolumePreview {
                        title: current_vol_title.clone(),
                        chapters: std::mem::take(&mut current_vol_chapters),
                    });
                }
                let detected_vol_title = heading_opt
                    .or(file_title_opt)
                    .unwrap_or_else(|| paragraphs.first().cloned().unwrap_or_else(|| "正文卷".to_string()));
                current_vol_title = detected_vol_title;
                continue;
            }

            if paragraphs.is_empty() {
                continue;
            }

            // 检查正文首行是否与外部或内部标题重复
            let doc_title = file_title_opt.clone().unwrap_or_else(|| format!("第{}章", chapter_seq));
            if let Some(first_p) = paragraphs.first() {
                let clean_p = first_p.replace(char::is_whitespace, "");
                let clean_t = doc_title.replace(char::is_whitespace, "");
                if clean_p == clean_t {
                    paragraphs.remove(0);
                }
            }

            if paragraphs.is_empty() {
                continue;
            }

            // 若单一文档内包含多个明确章节标题（无独立外部 TOC 映射时），进行内部切分
            let has_external_title = file_title_opt.is_some();
            let mut inner_chap_title = doc_title;
            let mut inner_paragraphs: Vec<String> = Vec::new();

            for (idx, p) in paragraphs.iter().enumerate() {
                let trimmed = p.trim();

                // 仅在无独立文件标题或非首段出现明确章节标记时进行切分
                let is_chap_split = !has_external_title && idx > 0 && is_chapter_header(trimmed);

                if is_chap_split {
                    if let Some(ch) = finalize_chapter(&inner_chap_title, &inner_paragraphs) {
                        current_vol_chapters.push(ch);
                        chapter_seq += 1;
                        inner_paragraphs.clear();
                    }
                    inner_chap_title = trimmed.to_string();
                    continue;
                }

                inner_paragraphs.push(trimmed.to_string());
            }

            if let Some(ch) = finalize_chapter(&inner_chap_title, &inner_paragraphs) {
                current_vol_chapters.push(ch);
                chapter_seq += 1;
            }
        }

        if !current_vol_chapters.is_empty() {
            volumes.push(ImportVolumePreview {
                title: current_vol_title,
                chapters: current_vol_chapters,
            });
        }
    }

    if volumes.is_empty() {
        return Err("EPUB 文件未能解析出有效正文段落，请检查文件是否损坏或受 DRM 保护".to_string());
    }

    let total_word_count = volumes
        .iter()
        .flat_map(|v| &v.chapters)
        .map(|c| c.word_count)
        .sum();

    Ok(ImportPreview {
        novel_title,
        total_word_count,
        volumes,
    })
}
