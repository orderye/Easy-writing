pub mod epub;

use regex::Regex;
use rusqlite::{params, Connection};
use shared_types::{ImportChapterPreview, ImportPreview, ImportVolumePreview, Novel};
use uuid::Uuid;

pub fn parse_import_document(file_name: &str, raw_text: &str) -> ImportPreview {
    // 1. Determine novel title from file name
    let clean_title = if let Some(idx) = file_name.rfind('.') {
        &file_name[..idx]
    } else {
        file_name
    };
    let novel_title = if clean_title.trim().is_empty() {
        "未命名作品".to_string()
    } else {
        clean_title.trim().to_string()
    };

    // 2. Compile header regexes
    let vol_regex = Regex::new(
        r"(?i)^\s*(第\s*[0-9一二三四五六七八九十百千万零两]+\s*卷.*|卷\s*[0-9一二三四五六七八九十百千万零两]+.*|Volume\s+[0-9]+.*)$",
    )
    .unwrap();

    let chap_regex = Regex::new(
        r"(?i)^\s*(第\s*[0-9一二三四五六七八九十百千万零两]+\s*[章节部回节].*|Chapter\s+[0-9]+.*|引言|序章|尾声|后记|番外.*)$",
    )
    .unwrap();

    let numbered_chap_regex = Regex::new(r"^\s*([0-9]{1,4})\s*[、.．\s]+(.*)$").unwrap();

    // 3. Parse line by line
    let mut volumes: Vec<ImportVolumePreview> = Vec::new();
    let mut current_vol_title = "默认卷".to_string();
    let mut current_chap_title = "序章".to_string();
    let mut current_chap_paragraphs: Vec<String> = Vec::new();
    let mut current_vol_chapters: Vec<ImportChapterPreview> = Vec::new();

    let finalize_chapter = |title: &str, paragraphs: &[String]| -> Option<ImportChapterPreview> {
        let plain = paragraphs.join("\n\n");
        let word_count = plain.chars().filter(|c| !c.is_whitespace()).count() as i32;
        if plain.trim().is_empty() && title == "序章" {
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

    for line in raw_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if line is a Volume header (must be relatively short, <= 50 chars)
        if trimmed.chars().count() <= 50 && vol_regex.is_match(trimmed) {
            // Finalize previous chapter
            if let Some(ch) = finalize_chapter(&current_chap_title, &current_chap_paragraphs) {
                current_vol_chapters.push(ch);
                current_chap_paragraphs.clear();
            }

            // Finalize previous volume if it has chapters
            if !current_vol_chapters.is_empty() {
                volumes.push(ImportVolumePreview {
                    title: current_vol_title.clone(),
                    chapters: std::mem::take(&mut current_vol_chapters),
                });
            }

            current_vol_title = trimmed.to_string();
            current_chap_title = "引言".to_string();
            continue;
        }

        // Check if line is a Chapter header (must be <= 60 chars)
        let is_chap = trimmed.chars().count() <= 60
            && (chap_regex.is_match(trimmed) || numbered_chap_regex.is_match(trimmed));

        if is_chap {
            if let Some(ch) = finalize_chapter(&current_chap_title, &current_chap_paragraphs) {
                current_vol_chapters.push(ch);
                current_chap_paragraphs.clear();
            }
            current_chap_title = trimmed.to_string();
            continue;
        }

        // Normal text paragraph
        current_chap_paragraphs.push(trimmed.to_string());
    }

    // Finalize last chapter
    if let Some(ch) = finalize_chapter(&current_chap_title, &current_chap_paragraphs) {
        current_vol_chapters.push(ch);
    }

    // Finalize last volume
    if !current_vol_chapters.is_empty() {
        volumes.push(ImportVolumePreview {
            title: current_vol_title,
            chapters: current_vol_chapters,
        });
    }

    // Fallback: If no chapters were recognized, create a single chapter with all text
    if volumes.is_empty() {
        let plain = raw_text.trim().to_string();
        let word_count = plain.chars().filter(|c| !c.is_whitespace()).count() as i32;
        let mut html = String::new();
        for p in raw_text.lines() {
            let t = p.trim();
            if !t.is_empty() {
                let escaped = t
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                html.push_str(&format!("<p>{}</p>", escaped));
            }
        }

        volumes.push(ImportVolumePreview {
            title: "正文".to_string(),
            chapters: vec![ImportChapterPreview {
                title: "第一章".to_string(),
                content: html,
                content_plain: plain,
                word_count,
            }],
        });
    }

    let total_word_count = volumes
        .iter()
        .flat_map(|v| &v.chapters)
        .map(|c| c.word_count)
        .sum();

    ImportPreview {
        novel_title,
        total_word_count,
        volumes,
    }
}

pub fn commit_import_novel(
    conn: &mut Connection,
    target_novel_id: Option<String>,
    preview: ImportPreview,
) -> Result<Novel, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (novel_id, novel) = match target_novel_id {
        Some(nid) => {
            let novel = tx
                .query_row(
                    "SELECT id, title, synopsis, created_at, updated_at, deleted_at FROM novel WHERE id = ?1 AND deleted_at IS NULL",
                    params![nid],
                    |r| {
                        Ok(Novel {
                            id: r.get(0)?,
                            title: r.get(1)?,
                            synopsis: r.get(2)?,
                            created_at: r.get(3)?,
                            updated_at: r.get(4)?,
                            deleted_at: r.get(5)?,
                        })
                    },
                )
                .map_err(|e| format!("目标作品未找到: {}", e))?;
            (nid, novel)
        }
        None => {
            let new_id = Uuid::new_v4().to_string();
            let synopsis = format!(
                "批量导入长篇文稿，共包含 {} 卷，{} 字。",
                preview.volumes.len(),
                preview.total_word_count
            );
            tx.execute(
                "INSERT INTO novel (id, title, synopsis) VALUES (?1, ?2, ?3)",
                params![new_id, preview.novel_title, synopsis],
            )
            .map_err(|e| format!("创建作品失败: {}", e))?;

            let novel = tx
                .query_row(
                    "SELECT id, title, synopsis, created_at, updated_at, deleted_at FROM novel WHERE id = ?1",
                    params![new_id],
                    |r| {
                        Ok(Novel {
                            id: r.get(0)?,
                            title: r.get(1)?,
                            synopsis: r.get(2)?,
                            created_at: r.get(3)?,
                            updated_at: r.get(4)?,
                            deleted_at: r.get(5)?,
                        })
                    },
                )
                .map_err(|e| format!("读取新建作品失败: {}", e))?;
            (new_id, novel)
        }
    };

    // Find starting sort orders
    let mut vol_order: i32 = tx
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM volume WHERE novel_id = ?1 AND deleted_at IS NULL",
            params![novel_id],
            |r| r.get(0),
        )
        .unwrap_or(-1)
        + 1;

    for vol_preview in preview.volumes {
        let vol_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO volume (id, novel_id, title, summary, sort_order) VALUES (?1, ?2, ?3, '', ?4)",
            params![vol_id, novel_id, vol_preview.title, vol_order],
        )
        .map_err(|e| format!("创建分卷失败: {}", e))?;
        vol_order += 1;

        let mut chap_order = 0;
        for chap_preview in vol_preview.chapters {
            let chap_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO chapter (id, novel_id, volume_id, title, summary, content, content_plain, word_count, status, sort_order)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, 'draft', ?8)",
                params![
                    chap_id,
                    novel_id,
                    vol_id,
                    chap_preview.title,
                    chap_preview.content,
                    chap_preview.content_plain,
                    chap_preview.word_count,
                    chap_order,
                ],
            )
            .map_err(|e| format!("创建章节失败: {}", e))?;
            chap_order += 1;
        }
    }

    tx.commit().map_err(|e| format!("提交导入事务失败: {}", e))?;
    Ok(novel)
}
