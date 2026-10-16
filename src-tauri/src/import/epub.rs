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

/// 简单 HTML 实体反转义
fn unescape_html(s: &str) -> String {
    let mut res = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&mdash;", "——")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&ldquo;", "“")
        .replace("&rdquo;", "”")
        .replace("&lsquo;", "‘")
        .replace("&rsquo;", "’");

    let num_entity_re = Regex::new(r"&#(x[0-9a-fA-F]+|[0-9]+);").unwrap();
    res = num_entity_re
        .replace_all(&res, |caps: &regex::Captures| {
            let num_str = &caps[1];
            let ch_opt = if let Some(hex) = num_str.strip_prefix('x') {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else {
                num_str.parse::<u32>().ok().and_then(char::from_u32)
            };
            match ch_opt {
                Some(c) => c.to_string(),
                None => caps[0].to_string(),
            }
        })
        .to_string();

    res
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
    let mut detected_heading = None;
    if let Some(cap) = heading_re.captures(&cleaned) {
        let raw_h = &cap[1];
        let plain_h = tag_re.replace_all(raw_h, " ").to_string();
        let unescaped = unescape_html(plain_h.trim());
        if !unescaped.is_empty() && unescaped.chars().count() <= 60 {
            detected_heading = Some(unescaped);
        }
    }

    // 块级标签替换为换行符
    let br_re = Regex::new(r"(?i)<br\s*/?>").unwrap();
    let block_re = Regex::new(r"(?i)</?(p|div|h[1-6]|li|blockquote|section|article)\b[^>]*>").unwrap();

    let with_breaks = br_re.replace_all(&cleaned, "\n");
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
        let clean_fname = if let Some(idx) = file_name.rfind('.') {
            &file_name[..idx]
        } else {
            file_name
        };
        novel_title = if clean_fname.trim().is_empty() {
            "未命名作品".to_string()
        } else {
            clean_fname.trim().to_string()
        };
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

    // 5. 解析目录映射 (zip_path -> 章节标题)
    let mut toc_titles: HashMap<String, String> = HashMap::new();

    // 5.1 从 EPUB 2 NCX 提取
    if let Some(ncx_p) = &toc_ncx_path {
        if let Ok(mut file) = archive.by_name(ncx_p) {
            let mut ncx_xml = String::new();
            if file.read_to_string(&mut ncx_xml).is_ok() {
                let ncx_dir = if let Some(idx) = ncx_p.rfind('/') {
                    &ncx_p[..idx + 1]
                } else {
                    ""
                };

                let navpoint_re = Regex::new(r#"(?is)<navPoint\b[^>]*>(.*?)</navPoint>"#).unwrap();
                let text_re = Regex::new(r#"(?is)<text[^>]*>(.*?)</text>"#).unwrap();
                let src_re = Regex::new(r#"src\s*=\s*["']([^"']+)["']"#).unwrap();

                for np in navpoint_re.captures_iter(&ncx_xml) {
                    let content = &np[1];
                    let text = text_re
                        .captures(content)
                        .map(|c| unescape_html(c[1].trim()));
                    let src = src_re.captures(content).map(|c| c[1].to_string());

                    if let (Some(t), Some(s)) = (text, src) {
                        let full_p = resolve_zip_path(ncx_dir, &s);
                        if !t.is_empty() && !toc_titles.contains_key(&full_p) {
                            toc_titles.insert(full_p, t);
                        }
                    }
                }
            }
        }
    }

    // 5.2 从 EPUB 3 Nav XHTML 补充
    if let Some(nav_p) = &nav_xhtml_path {
        if let Ok(mut file) = archive.by_name(nav_p) {
            let mut nav_xml = String::new();
            if file.read_to_string(&mut nav_xml).is_ok() {
                let nav_dir = if let Some(idx) = nav_p.rfind('/') {
                    &nav_p[..idx + 1]
                } else {
                    ""
                };

                let a_re = Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#).unwrap();
                let tag_re = Regex::new(r"<[^>]+>").unwrap();

                for a_cap in a_re.captures_iter(&nav_xml) {
                    let href = &a_cap[1];
                    let raw_text = &a_cap[2];
                    let plain = tag_re.replace_all(raw_text, "").to_string();
                    let title = unescape_html(plain.trim());
                    let full_p = resolve_zip_path(nav_dir, href);

                    if !title.is_empty() && !toc_titles.contains_key(&full_p) {
                        toc_titles.insert(full_p, title);
                    }
                }
            }
        }
    }

    // 6. 解析 Spine 阅读顺序
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

    // 7. 正则匹配定义（卷、章）
    let vol_regex = Regex::new(
        r"(?i)^\s*(第\s*[0-9一二三四五六七八九十百千万零两]+\s*卷.*|卷\s*[0-9一二三四五六七八九十百千万零两]+.*|Volume\s+[0-9]+.*)$",
    )
    .unwrap();

    let chap_regex = Regex::new(
        r"(?i)^\s*(第\s*[0-9一二三四五六七八九十百千万零两]+\s*[章节部回节].*|Chapter\s+[0-9]+.*|引言|序章|尾声|后记|番外.*)$",
    )
    .unwrap();

    let numbered_chap_regex = Regex::new(r"^\s*([0-9]{1,4})\s*[、.．\s]+(.*)$").unwrap();

    // 8. 依次遍历 Spine 内容文档
    let mut volumes: Vec<ImportVolumePreview> = Vec::new();
    let mut current_vol_title = "正文卷".to_string();
    let mut current_vol_chapters: Vec<ImportChapterPreview> = Vec::new();
    let mut chapter_seq = 1;

    let finalize_chapter = |title: &str, paragraphs: &[String]| -> Option<ImportChapterPreview> {
        let plain = paragraphs.join("\n\n");
        let word_count = plain.chars().filter(|c| !c.is_whitespace()).count() as i32;
        if plain.trim().is_empty() {
            return None;
        }

        let mut html = String::new();
        for p in paragraphs {
            let escaped = p
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            html.push_str(&format!("<p>{}</p>", escaped));
        }

        Some(ImportChapterPreview {
            title: title.to_string(),
            content: html,
            content_plain: plain,
            word_count,
        })
    };

    for idref in spine_items {
        let Some(item) = manifest.get(&idref) else {
            continue;
        };

        // 仅处理 HTML/XHTML 文件
        let is_html = item.media_type.contains("html")
            || item.full_path.ends_with(".xhtml")
            || item.full_path.ends_with(".html")
            || item.full_path.ends_with(".htm");

        if !is_html {
            continue;
        }

        // 读取文档内容
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

        let (heading_opt, paragraphs) = html_to_paragraphs(&html_content);
        if paragraphs.is_empty() {
            continue;
        }

        // 优先使用 TOC 目录中的标题，其次使用文档 h1/h2 标题，最后从段落开头提取
        let file_title_opt = toc_titles
            .get(&item.full_path)
            .cloned()
            .or(heading_opt);

        // 检查文档是否为纯分卷说明文档（如只有一行 "第一卷 龙兴微末"）
        let first_line = paragraphs[0].trim();
        if paragraphs.len() <= 2
            && first_line.chars().count() <= 50
            && vol_regex.is_match(first_line)
        {
            // 归档上一个卷
            if !current_vol_chapters.is_empty() {
                volumes.push(ImportVolumePreview {
                    title: current_vol_title.clone(),
                    chapters: std::mem::take(&mut current_vol_chapters),
                });
            }
            current_vol_title = first_line.to_string();
            continue;
        }

        // 检查该文档内是否包含多个章节划分（部分 EPUB 把多章写在一个 XHTML 内）
        let mut inner_chap_title = file_title_opt.clone().unwrap_or_else(|| {
            format!("第{}章", chapter_seq)
        });
        let mut inner_paragraphs: Vec<String> = Vec::new();
        let mut found_inner_splits = false;

        for (idx, p) in paragraphs.iter().enumerate() {
            let trimmed = p.trim();
            // 检查分卷
            if trimmed.chars().count() <= 50 && vol_regex.is_match(trimmed) {
                if let Some(ch) = finalize_chapter(&inner_chap_title, &inner_paragraphs) {
                    current_vol_chapters.push(ch);
                    chapter_seq += 1;
                    inner_paragraphs.clear();
                }
                if !current_vol_chapters.is_empty() {
                    volumes.push(ImportVolumePreview {
                        title: current_vol_title.clone(),
                        chapters: std::mem::take(&mut current_vol_chapters),
                    });
                }
                current_vol_title = trimmed.to_string();
                inner_chap_title = format!("第{}章", chapter_seq);
                found_inner_splits = true;
                continue;
            }

            // 检查分章（非首段出现章节标题，或者首段与文件外部标题明显不同）
            let is_chap_header = trimmed.chars().count() <= 60
                && (chap_regex.is_match(trimmed) || numbered_chap_regex.is_match(trimmed));

            if is_chap_header && (idx > 0 || file_title_opt.is_none()) {
                if let Some(ch) = finalize_chapter(&inner_chap_title, &inner_paragraphs) {
                    current_vol_chapters.push(ch);
                    chapter_seq += 1;
                    inner_paragraphs.clear();
                }
                inner_chap_title = trimmed.to_string();
                found_inner_splits = true;
                continue;
            }

            // 若首行恰好与标题完全相同，则不重复记录进正文第一行
            if idx == 0 && Some(trimmed) == file_title_opt.as_deref() {
                continue;
            }

            inner_paragraphs.push(trimmed.to_string());
        }

        // 收集该文档剩余段落
        if let Some(ch) = finalize_chapter(&inner_chap_title, &inner_paragraphs) {
            current_vol_chapters.push(ch);
            chapter_seq += 1;
        } else if !found_inner_splits && !inner_paragraphs.is_empty() {
            // 回退归档
            if let Some(ch) = finalize_chapter(
                &file_title_opt.unwrap_or_else(|| format!("第{}章", chapter_seq)),
                &inner_paragraphs,
            ) {
                current_vol_chapters.push(ch);
                chapter_seq += 1;
            }
        }
    }

    // 归档最后一卷
    if !current_vol_chapters.is_empty() {
        volumes.push(ImportVolumePreview {
            title: current_vol_title,
            chapters: current_vol_chapters,
        });
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
