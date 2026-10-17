use regex::Regex;
use shared_types::ImportChapterPreview;
use std::sync::LazyLock;

/// 匹配卷标题的严格正则：
/// 覆盖：第X卷、卷X、Volume X、作品相关、作品设定、正文卷、番外卷、终卷、特别卷、外传卷、前卷、后卷、上/中/下卷等。
/// 并限制附加副标题不含标点符号（防止正文普通段落误判为分卷）。
pub static VOL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(第\s*[0-9一二三四五六七八九十百千万零两]+\s*卷|卷\s*[0-9一二三四五六七八九十百千万零两]+|Volume\s+[0-9]+|作品相关|作品设定|相关资料|正文卷?|番外卷|番外篇|终卷|特别卷|外传卷|外传篇|前卷|后卷|上卷|中卷|下卷)([\s:：\-—、]*[^，。！？\n]{1,30})?\s*$",
    )
    .unwrap()
});

/// 匹配章节标题的正则：
/// 覆盖：第X章/节/部/回/集、Chapter X、引言、前言、序章、序言、楔子、契子、尾声、后记、跋、番外等。
/// 不匹配纯数字点号（如 1、 2. ），避免正文有序列表被误切为章节。
pub static CHAP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(第\s*[0-9一二三四五六七八九十百千万零两]+\s*[章节部回集].*|Chapter\s+[0-9]+.*|引言.*|前言.*|序章.*|序言.*|楔子.*|契子.*|尾声.*|后记.*|番外.*|序[:：\-—、\s].*|跋[:：\-—、\s].*|序$|跋$)",
    )
    .unwrap()
});

/// 判断单行文本是否满足分卷标题特征
pub fn is_volume_header(trimmed: &str) -> bool {
    let char_count = trimmed.chars().count();
    if !(2..=35).contains(&char_count) {
        return false;
    }
    // 严禁包含句子标点，防止引言或分析段落误判为分卷
    if trimmed
        .chars()
        .any(|c| matches!(c, '，' | '。' | '！' | '？' | '；' | '\n' | '\r' | ',' | '.' | '!' | '?' | ';'))
    {
        return false;
    }
    VOL_REGEX.is_match(trimmed)
}

/// 判断单行文本是否满足章节标题特征
pub fn is_chapter_header(trimmed: &str) -> bool {
    let char_count = trimmed.chars().count();
    if !(2..=60).contains(&char_count) {
        return false;
    }
    // 严禁包含句号、感叹号、问号等句末标点
    if trimmed
        .chars()
        .any(|c| matches!(c, '。' | '！' | '？' | '；' | '\n' | '\r' | '!' | '?' | ';'))
    {
        return false;
    }
    CHAP_REGEX.is_match(trimmed)
}

/// 归一化标题空白字符（去除首尾空白并将连续多个空格压缩为单个空格）
pub fn normalize_title(title: &str) -> String {
    let mut result = String::new();
    let mut last_was_space = false;
    for c in title.trim().chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    result
}

/// 从文件名提取作品标题默认值
pub fn clean_file_name_to_title(file_name: &str) -> String {
    let clean = if let Some(idx) = file_name.rfind('.') {
        &file_name[..idx]
    } else {
        file_name
    };
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        "未命名作品".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 统一字数统计：排除所有空白字符后的字符总数
pub fn count_words(text: &str) -> i32 {
    text.chars().filter(|c| !c.is_whitespace()).count() as i32
}

/// 将段落列表转为标准 HTML 格式（转义基本字符并包裹在 <p> 标签中）
pub fn format_paragraphs_to_html(paragraphs: &[String]) -> String {
    let mut html = String::new();
    for p in paragraphs {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            let escaped = trimmed
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            html.push_str(&format!("<p>{}</p>", escaped));
        }
    }
    html
}

/// 将章节标题和段落组装为最终的 ImportChapterPreview
pub fn finalize_chapter(title: &str, paragraphs: &[String]) -> Option<ImportChapterPreview> {
    let plain = paragraphs.join("\n\n");
    let word_count = count_words(&plain);

    // 若章节正文完全为空且标题为序章/引言，忽略无意义占位
    if plain.trim().is_empty() && (title == "序章" || title == "引言") {
        return None;
    }

    let html = format_paragraphs_to_html(paragraphs);

    Some(ImportChapterPreview {
        title: normalize_title(title),
        content: html,
        content_plain: plain,
        word_count,
    })
}

/// HTML 实体解码器，支持常用命名实体及数值实体（&#123; 或 &#x1F;）
pub fn unescape_html(s: &str) -> String {
    let s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&mdash;", "——")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&ldquo;", "“")
        .replace("&rdquo;", "”")
        .replace("&lsquo;", "‘")
        .replace("&rsquo;", "’");

    let num_re = Regex::new(r"&#(x?[0-9a-fA-F]+);").unwrap();
    num_re
        .replace_all(&s, |caps: &regex::Captures| {
            let code_str = &caps[1];
            let code = if let Some(hex) = code_str.strip_prefix('x') {
                u32::from_str_radix(hex, 16).ok()
            } else {
                code_str.parse::<u32>().ok()
            };
            if let Some(cp) = code.and_then(char::from_u32) {
                cp.to_string()
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}
