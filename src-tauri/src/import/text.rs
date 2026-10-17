use crate::import::common::{
    clean_file_name_to_title, count_words, finalize_chapter, format_paragraphs_to_html,
    is_chapter_header, is_volume_header, normalize_title,
};
use shared_types::{ImportChapterPreview, ImportPreview, ImportVolumePreview};

/// 解析纯文本或 Markdown 格式的长篇文稿，支持自动识别多卷多章层级与智能拆解
pub fn parse_import_document(file_name: &str, raw_text: &str) -> ImportPreview {
    let novel_title = clean_file_name_to_title(file_name);

    let mut volumes: Vec<ImportVolumePreview> = Vec::new();
    let mut current_vol_title = "默认卷".to_string();
    let mut current_chap_title = "序章".to_string();
    let mut current_chap_paragraphs: Vec<String> = Vec::new();
    let mut current_vol_chapters: Vec<ImportChapterPreview> = Vec::new();

    for line in raw_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 1. 判断是否是分卷标题
        if is_volume_header(trimmed) {
            // 归档上一章节
            if let Some(ch) = finalize_chapter(&current_chap_title, &current_chap_paragraphs) {
                current_vol_chapters.push(ch);
                current_chap_paragraphs.clear();
            }

            // 归档上一卷（若有章节）
            if !current_vol_chapters.is_empty() {
                volumes.push(ImportVolumePreview {
                    title: normalize_title(&current_vol_title),
                    chapters: std::mem::take(&mut current_vol_chapters),
                });
            }

            current_vol_title = trimmed.to_string();
            current_chap_title = "引言".to_string();
            continue;
        }

        // 2. 判断是否是章节标题
        if is_chapter_header(trimmed) {
            if let Some(ch) = finalize_chapter(&current_chap_title, &current_chap_paragraphs) {
                current_vol_chapters.push(ch);
                current_chap_paragraphs.clear();
            }
            current_chap_title = trimmed.to_string();
            continue;
        }

        // 3. 普通正文段落
        current_chap_paragraphs.push(trimmed.to_string());
    }

    // 归档最后一章
    if let Some(ch) = finalize_chapter(&current_chap_title, &current_chap_paragraphs) {
        current_vol_chapters.push(ch);
    }

    // 归档最后一卷
    if !current_vol_chapters.is_empty() {
        volumes.push(ImportVolumePreview {
            title: normalize_title(&current_vol_title),
            chapters: current_vol_chapters,
        });
    }

    // 回退兜底：如果文稿中没有识别出任何分卷分章标记，则作为单章处理
    if volumes.is_empty() {
        let plain = raw_text.trim().to_string();
        let word_count = count_words(&plain);
        let paragraphs: Vec<String> = raw_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let html = format_paragraphs_to_html(&paragraphs);

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
