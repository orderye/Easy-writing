use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use crate::db::DbPool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shared_types::{Chapter, LlmProfile, Novel};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RawCandidate {
    pub kind: String,   // "worldview" | "character" | "plot" | "chapter_summary"
    pub action: String, // "create" | "update"
    pub title: String,
    pub payload: String,
    pub evidence: String,
    /// 模型声称要更新的既有条目 id（旧版没有这个字段，故 serde 默认 None）。
    #[serde(default)]
    pub target_id: Option<String>,
}

/// 宽松候选反序列化结构体：容忍各种大模型产生的别名字段和类型偏移
#[derive(Deserialize, Debug, Clone)]
pub struct TolerantRawCandidate {
    #[serde(alias = "type", alias = "category", default)]
    pub kind: Option<String>,
    #[serde(alias = "op", alias = "operation", default)]
    pub action: Option<String>,
    #[serde(alias = "name", alias = "entity", alias = "heading", alias = "term", default)]
    pub title: Option<String>,
    #[serde(alias = "content", alias = "description", alias = "desc", alias = "detail", alias = "profile", alias = "summary", default)]
    pub payload: Option<String>,
    #[serde(alias = "quote", alias = "context", alias = "proof", alias = "source", alias = "excerpt", default)]
    pub evidence: Option<String>,
    #[serde(alias = "targetId", alias = "id", alias = "ref_id", default)]
    pub target_id: Option<String>,
}

/// 自修复 JSON 解析引擎：容错处理 Markdown 代码块、前后说明闲聊、单对象、尾随逗号、注释、截断等各种模型异常。
/// 明确区分合法空数组 `[]`（返回 Ok(vec![])）与格式解析失败（返回 Err(...)）。
pub fn self_heal_and_parse_candidates(raw_text: &str, default_kind: &str) -> Result<Vec<RawCandidate>, String> {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() {
        return Err("模型输出内容为空".to_string());
    }

    // 1. 清理 Markdown 代码块包裹 ```json ... ``` 或 ``` ... ```
    let mut cleaned = trimmed.to_string();
    if let Some(start_idx) = cleaned.find("```") {
        let after_fence = &cleaned[start_idx + 3..];
        let content_start = if let Some(newline_idx) = after_fence.find('\n') {
            start_idx + 3 + newline_idx + 1
        } else {
            start_idx + 3
        };
        let end_idx = cleaned[content_start..].rfind("```").map(|i| content_start + i).unwrap_or(cleaned.len());
        cleaned = cleaned[content_start..end_idx].trim().to_string();
    }

    // 2. 提取 JSON 数组或单对象范围
    let mut json_str = cleaned.trim().to_string();
    let has_array = json_str.contains('[') && json_str.contains(']');
    if !has_array {
        // 如果没有完整数组，但有对象 { ... }，尝试将其包装为数组
        if let (Some(first_brace), Some(last_brace)) = (json_str.find('{'), json_str.rfind('}')) {
            if first_brace < last_brace {
                json_str = format!("[{}]", &json_str[first_brace..=last_brace]);
            } else {
                return Err("模型输出中未包含有效的 JSON 对象或数组".to_string());
            }
        } else {
            return Err("模型输出中未包含 JSON 数组或对象标记".to_string());
        }
    } else if let (Some(start), Some(end)) = (json_str.find('['), json_str.rfind(']')) {
        if start <= end {
            json_str = json_str[start..=end].to_string();
        }
    }

    // 3. 语法自修复：清理 JS 风格行注释 //... 与尾随逗号 ,] 或 ,}
    json_str = sanitize_json_syntax(&json_str);

    // 3.5 显式检测合法空数组：若清洗后为合法空数组 `[]`，则明确返回成功空列表
    let trimmed_json = json_str.trim();
    if trimmed_json == "[]" {
        return Ok(Vec::new());
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed_json) {
        if let Some(arr) = val.as_array() {
            if arr.is_empty() {
                return Ok(Vec::new());
            }
        }
    }

    // 4. 第一轮尝试：直接宽松反序列化
    if let Ok(tolerants) = serde_json::from_str::<Vec<TolerantRawCandidate>>(&json_str) {
        let converted = convert_tolerants(tolerants, default_kind);
        if !converted.is_empty() {
            return Ok(converted);
        }
    }

    // 5. 第二轮尝试：模型可能因 token 耗尽而发生了尾部截断（如 `[{"title": "xxx"}, {"title": "yy`）
    if let Some(repaired) = attempt_repair_truncated_json(&json_str) {
        if let Ok(tolerants) = serde_json::from_str::<Vec<TolerantRawCandidate>>(&repaired) {
            let converted = convert_tolerants(tolerants, default_kind);
            if !converted.is_empty() {
                return Ok(converted);
            }
        }
    }

    // 6. 兜底回退：正则模式提取各个完整的独立 JSON 对象
    let regex_items = fallback_regex_extract(&json_str, default_kind);
    if !regex_items.is_empty() {
        return Ok(regex_items);
    }

    // 错误消息里**只描述长度，不回显内容**。
    //
    // 旧实现拼接了模型输出的前 150 个字符，而模型输出中包含候选的
    // `evidence` 字段 —— 那是直接摘自作者正文的原句。这条错误会一路
    // 传到 eprintln!，等于把小说正文片段写进普通日志。
    // 排障需要的只是「有多长」和「开头长什么样（脱敏后）」。
    Err(format!(
        "无法从模型输出中解析出任何有效候选条目（输出长度 {} 字符，起始片段：{}）",
        raw_text.chars().count(),
        redact_prose_fragment(raw_text)
    ))
}

/// 生成一段可用于排障、但不含正文原文的片段描述。
///
/// 只保留非中文、非字母数字的结构性符号（如 `{`, `[`, `"`），
/// 并把它们替换成占位标记。这样排障时仍能看出「是不是 JSON」，
/// 却拿不到任何正文内容。
fn redact_prose_fragment(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars().take(150) {
        if ch.is_ascii() && !ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('…') {
            out.push('·');
        }
    }
    if out.is_empty() {
        "（空）".to_string()
    } else {
        out
    }
}

/// 清理语法杂质（尾随逗号、行外注释等，且智能保护字符串内部的双斜杠）
fn sanitize_json_syntax(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escape_next = false;

    for line in input.lines() {
        let trimmed_line = line.trim();
        if !in_string && trimmed_line.starts_with("//") {
            continue;
        }

        let mut line_buf = String::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if escape_next {
                escape_next = false;
                line_buf.push(c);
                continue;
            }

            if c == '\\' {
                escape_next = true;
                line_buf.push(c);
                continue;
            }

            if c == '"' {
                in_string = !in_string;
                line_buf.push(c);
                continue;
            }

            // 仅在引号外部剥离 // 注释
            if !in_string && c == '/' && chars.peek() == Some(&'/') {
                break;
            }

            line_buf.push(c);
        }
        out.push_str(&line_buf);
        out.push('\n');
    }

    // 去除所有尾随逗号（包括逗号与闭合括号之间存在换行和空白的情况）
    remove_trailing_commas(&out)
}

/// 智能去除 JSON 中闭合括号前多余的尾随逗号，且保护引号字符串内部的逗号
fn remove_trailing_commas(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut in_str = false;
    let mut escape = false;
    let len = chars.len();

    let mut i = 0;
    while i < len {
        let c = chars[i];
        if escape {
            escape = false;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '\\' {
            escape = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            out.push(c);
            i += 1;
            continue;
        }

        if !in_str && c == ',' {
            // 查看后续第一个非空白字符
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (chars[j] == '}' || chars[j] == ']') {
                // 跳过此逗号（即删除尾随逗号）
                i += 1;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }
    out
}

/// 尝试自修复截断的 JSON 数组
fn attempt_repair_truncated_json(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    // 找到最后一个合法的对象闭合大括号 '}'
    if let Some(last_brace) = trimmed.rfind('}') {
        let candidate = format!("{}]", &trimmed[..=last_brace]);
        // 验证自修复后的字符串是否可以解析
        if serde_json::from_str::<serde_json::Value>(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

/// 将 TolerantRawCandidate 转换为标准 RawCandidate 并自修复缺失字段
fn convert_tolerants(items: Vec<TolerantRawCandidate>, default_kind: &str) -> Vec<RawCandidate> {
    let mut res = Vec::new();
    for item in items {
        let mut kind = item.kind.unwrap_or_else(|| default_kind.to_string()).to_lowercase();
        if !["worldview", "character", "plot", "chapter_summary"].contains(&kind.as_str()) {
            kind = default_kind.to_string();
        }

        let action = item.action.unwrap_or_else(|| "create".to_string()).to_lowercase();
        let valid_action = if action.contains("update") || action.contains("更新") {
            "update".to_string()
        } else {
            "create".to_string()
        };

        let mut title = item.title.unwrap_or_default().trim().to_string();
        let payload = item.payload.unwrap_or_default().trim().to_string();
        let evidence = item.evidence.unwrap_or_default().trim().to_string();

        // 自修复：若标题为空，但 payload 存在，尝试从 payload 第一行提取
        if title.is_empty() && !payload.is_empty() {
            title = payload
                .lines()
                .next()
                .unwrap_or("未命名条目")
                .chars()
                .take(30)
                .collect();
        }

        if !title.is_empty() || !payload.is_empty() {
            res.push(RawCandidate {
                kind,
                action: valid_action,
                title,
                payload,
                evidence,
                target_id: item.target_id.filter(|s| !s.trim().is_empty() && s != "null"),
            });
        }
    }
    res
}

/// 正则容灾回退提取器：即使 JSON 严重残损，依然能从文本中捕获提取条目
fn fallback_regex_extract(input: &str, default_kind: &str) -> Vec<RawCandidate> {
    let mut list = Vec::new();
    // 匹配每个花括号包含的片段
    let mut chars = input.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch == '{' {
            let mut depth = 1;
            let mut end = start;
            for (idx, c) in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end = idx;
                        break;
                    }
                }
            }
            if depth == 0 {
                let obj_str = &input[start..=end];
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(obj_str) {
                    if let Some(obj) = v.as_object() {
                        let title = obj
                            .get("title")
                            .or_else(|| obj.get("name"))
                            .or_else(|| obj.get("heading"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();

                        let payload = obj
                            .get("payload")
                            .or_else(|| obj.get("content"))
                            .or_else(|| obj.get("description"))
                            .or_else(|| obj.get("desc"))
                            .or_else(|| obj.get("summary"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();

                        let evidence = obj
                            .get("evidence")
                            .or_else(|| obj.get("quote"))
                            .or_else(|| obj.get("context"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();

                        let target_id = obj
                            .get("target_id")
                            .or_else(|| obj.get("targetId"))
                            .or_else(|| obj.get("id"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty() && s != "null");

                        if !title.is_empty() || !payload.is_empty() {
                            list.push(RawCandidate {
                                kind: default_kind.to_string(),
                                action: if target_id.is_some() { "update".to_string() } else { "create".to_string() },
                                title,
                                payload,
                                evidence,
                                target_id,
                            });
                        }
                    }
                }
            }
        }
    }
    list
}

/// 既有条目索引：kind -> [(id, title, aliases_json)]。
pub type ExistingIndex = Vec<(String, Vec<(String, String, String)>)>;

/// 归一化标题，用于「同一条目的不同写法」匹配。
///
/// 注意这**不是**去重哈希：哈希只能判等，无法把「李四（师兄）」这种
/// 带后缀的写法归到「李四」上。这里至少处理空白与大小写差异。
pub fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase().replace(' ', "")
}

/// 一个候选最终应当执行的动作。
#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedAction {
    pub action: String,
    pub target_id: Option<String>,
}

/// 把模型的 create/update 主张落到具体的既有条目上。
///
/// 规则优先级：
///   1. 模型给的 target_id 真的存在 → 更新它（模型最了解上下文）；
///   2. 标题（或既有别名）归一化后与某条目相等 → 更新它；
///   3. 否则 → 新建。
///
/// 这样做的意义：旧实现直接采信模型的 action，而模型**看不到**已有设定库，
/// 于是「李四」和「李四（师兄）」会各自建一条独立角色，长期积累成脏数据。
pub fn resolve_candidate_action(
    claimed_action: &str,
    claimed_target_id: Option<&str>,
    title: &str,
    kind: &str,
    existing: &ExistingIndex,
) -> ResolvedAction {
    let entries = existing
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v.as_slice())
        .unwrap_or(&[]);

    // 1) 模型点了名，且该 id 确实存在于这个 kind 下
    if let Some(tid) = claimed_target_id {
        if entries.iter().any(|(id, _, _)| id == tid) {
            return ResolvedAction {
                action: "update".to_string(),
                target_id: Some(tid.to_string()),
            };
        }
    }

    // 2) 标题 / 别名规则匹配
    let key = normalize_title(title);
    if !key.is_empty() {
        for (id, existing_title, aliases_json) in entries {
            if normalize_title(existing_title) == key {
                return ResolvedAction {
                    action: "update".to_string(),
                    target_id: Some(id.clone()),
                };
            }
            let aliases: Vec<String> =
                serde_json::from_str(aliases_json).unwrap_or_default();
            if aliases.iter().any(|a| normalize_title(a) == key) {
                return ResolvedAction {
                    action: "update".to_string(),
                    target_id: Some(id.clone()),
                };
            }
        }
    }

    // 3) 兜底：新建，且不带上任何 target_id，避免悬挂引用
    let _ = claimed_action;
    ResolvedAction {
        action: "create".to_string(),
        target_id: None,
    }
}

/// 把既有设定库读成索引，供 prompt 注入与动作判定使用。
pub fn build_existing_index(
    conn: &rusqlite::Connection,
    novel_id: &str,
) -> Result<ExistingIndex, String> {
    let mut index: ExistingIndex = Vec::new();

    let chars = crate::db::settings::list_characters(conn, novel_id).map_err(|e| e.to_string())?;
    index.push((
        "character".to_string(),
        chars
            .into_iter()
            .map(|c| (c.id, c.name, c.aliases))
            .collect(),
    ));

    let wv = crate::db::settings::list_worldview(conn, novel_id).map_err(|e| e.to_string())?;
    index.push((
        "worldview".to_string(),
        wv.into_iter()
            .map(|w| (w.id, w.title, "[]".to_string()))
            .collect(),
    ));

    let plots = crate::db::settings::list_plots(conn, novel_id).map_err(|e| e.to_string())?;
    index.push((
        "plot".to_string(),
        plots
            .into_iter()
            .map(|p| (p.id, p.title, "[]".to_string()))
            .collect(),
    ));

    Ok(index)
}

/// 把某个 kind 的既有条目渲染成 prompt 片段。
fn render_existing_for_prompt(existing: &ExistingIndex, kind: &str) -> String {
    let entries = existing
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v.as_slice())
        .unwrap_or(&[]);

    if entries.is_empty() {
        return "（当前设定库中还没有这一类的条目）".to_string();
    }

    let mut s = String::new();
    for (id, title, aliases) in entries.iter().take(200) {
        s.push_str(&format!("- id={id} 标题={title} 别名={aliases}\n"));
    }
    s
}

pub fn compute_dedup_key(kind: &str, title: &str) -> String {
    let normalized = normalize_title(title);
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn compute_update_dedup_key(kind: &str, target_id: &str, payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(b":update:");
    hasher.update(target_id.as_bytes());
    hasher.update(b":");
    let norm_payload: String = payload.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    hasher.update(norm_payload.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 为章节梗概计算专用的去重键。
///
/// 章节梗概的核心实体为章节自身（`chapter_id`）。去重键绑定 `chapter_id` 与正规化后的 `payload`：
/// 1. 相同章节若产生完全相同的梗概文本，能被有效去重；
/// 2. 相同章节若在正文修改后产生新梗概文本，将产生不同 dedup_key，不会被历史已采纳的旧梗概永久拦截。
pub fn compute_chapter_summary_dedup_key(chapter_id: &str, payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"chapter_summary:");
    hasher.update(chapter_id.as_bytes());
    hasher.update(b":");
    let norm_payload: String = payload.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    hasher.update(norm_payload.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn compute_chapter_fingerprint(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// T07：提炼成本控制（single_call）、超长章节分块、疑似同人标记
// ---------------------------------------------------------------------------

/// 提炼调用模式（设置键 `extraction_mode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionMode {
    /// 四个 Agent 受限并行（默认，现状）：四类候选各一次调用，互不干扰。
    Parallel4,
    /// 一次调用返回四类候选：省 3 次请求，但单次输出更长、更容易被截断。
    SingleCall,
}

impl ExtractionMode {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("single_call") => Self::SingleCall,
            _ => Self::Parallel4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parallel4 => "parallel_4",
            Self::SingleCall => "single_call",
        }
    }
}

pub const SETTING_EXTRACTION_MODE: &str = "extraction_mode";
pub const SETTING_EXTRACTION_CHUNK_THRESHOLD: &str = "extraction_chunk_threshold";
/// 默认分块阈值（字符）。约 1.2 万汉字，给 prompt 与输出留出上下文余量。
pub const DEFAULT_CHUNK_THRESHOLD_CHARS: usize = 12_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionSettings {
    pub mode: ExtractionMode,
    pub chunk_threshold_chars: usize,
}

/// 读取提炼相关设置。缺省即 `parallel_4` + 1.2 万字符，不要求作者先配置。
pub fn read_extraction_settings(conn: &rusqlite::Connection) -> ExtractionSettings {
    let mode_raw =
        crate::db::app_setting::get_setting(conn, SETTING_EXTRACTION_MODE)
            .ok()
            .flatten();
    let threshold =
        crate::db::app_setting::get_setting(conn, SETTING_EXTRACTION_CHUNK_THRESHOLD)
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<usize>().ok())
            // 下限 500：再小只会把句子切碎、成倍增加调用。
            .filter(|v| *v >= 500)
            .unwrap_or(DEFAULT_CHUNK_THRESHOLD_CHARS);

    ExtractionSettings {
        mode: ExtractionMode::parse(mode_raw.as_deref()),
        chunk_threshold_chars: threshold,
    }
}

/// 把超长正文按行（段落）切成不超过 `threshold_chars` 的块。
///
/// 只在行边界切；单行本身超阈值时按字符硬切 —— 否则一段超长正文会让
/// 整章退回「一次超长调用」，分块就失去意义。
pub fn split_into_chunks(text: &str, threshold_chars: usize) -> Vec<String> {
    if threshold_chars == 0 || text.chars().count() <= threshold_chars {
        return vec![text.to_string()];
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for line in text.split('\n') {
        let line_len = line.chars().count();

        if line_len > threshold_chars {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
                current_len = 0;
            }
            let chars: Vec<char> = line.chars().collect();
            for slice in chars.chunks(threshold_chars) {
                chunks.push(slice.iter().collect());
            }
            continue;
        }

        let needed = line_len + if current.is_empty() { 0 } else { 1 };
        if current_len + needed > threshold_chars && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if !current.is_empty() {
            current.push('\n');
            current_len += 1;
        }
        current.push_str(line);
        current_len += line_len;
    }

    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

pub const CANDIDATE_KINDS: [&str; 4] = ["worldview", "character", "plot", "chapter_summary"];

/// 解析 `single_call` 的输出。
///
/// 同时接受两种形态：
///   1. 分组对象 `{"worldview": [...], "character": [...]}` —— 任务书要求的形态；
///   2. 自带 `kind` 字段的数组 —— 模型经常「回退」到并行模式的输出结构。
/// 两种形态最终都归一成同一种 `Vec<RawCandidate>`，与 parallel_4 完全同构。
pub fn parse_single_call_candidates(raw: &str) -> Result<Vec<RawCandidate>, String> {
    if let Some(result) = parse_grouped_candidates(raw) {
        return result;
    }
    let items = self_heal_and_parse_candidates(raw, "")?;
    let valid: Vec<RawCandidate> = items
        .into_iter()
        .filter(|i| CANDIDATE_KINDS.contains(&i.kind.as_str()))
        .collect();
    if valid.is_empty() {
        return Err("single_call 输出中没有可识别的四类候选".to_string());
    }
    Ok(valid)
}

/// 尝试把分组对象形态摊平成候选数组；不是该形态时返回 `None`。
fn parse_grouped_candidates(raw: &str) -> Option<Result<Vec<RawCandidate>, String>> {
    let cleaned = raw.trim();
    let start = cleaned.find('{')?;
    let end = cleaned.rfind('}')?;
    if end <= start {
        return None;
    }
    let candidate = sanitize_json_syntax(&cleaned[start..=end]);
    let value: serde_json::Value = serde_json::from_str(&candidate).ok()?;
    let obj = value.as_object()?;
    if !CANDIDATE_KINDS.iter().any(|k| obj.contains_key(*k)) {
        return None;
    }

    let mut merged: Vec<serde_json::Value> = Vec::new();
    for kind in CANDIDATE_KINDS {
        let Some(items) = obj.get(kind).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in items {
            let mut item = item.clone();
            if let Some(map) = item.as_object_mut() {
                map.insert(
                    "kind".to_string(),
                    serde_json::Value::String(kind.to_string()),
                );
                merged.push(item);
            }
        }
    }

    let array_json = serde_json::Value::Array(merged).to_string();
    Some(self_heal_and_parse_candidates(&array_json, ""))
}

/// 判断两个归一化标题是否「看起来是同一人」。
///
/// 规则刻意宽松：**只用于提示，不用于自动合并**。命中后由作者决定是否合并，
/// 所以宁可多提示，也不能悄悄把两个角色合成一个。
///
///   1. 互相包含（「李四」/「李四（师兄）」）—— 标点与后缀是最常见的写法漂移；
///   2. 短名（2–4 个汉字）共享至少一半字符（「林舟」/「小舟」）——
///      中文名字常见「姓 + 小名」共享一个字的情形。
pub fn titles_look_same(a: &str, b: &str) -> bool {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() || a == b {
        return false;
    }
    if a.contains(b) || b.contains(a) {
        return true;
    }

    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let short = ac.len().min(bc.len());
    let long = ac.len().max(bc.len());
    if short < 2 || long > 4 {
        return false;
    }
    let all_cjk = ac.iter().all(|c| is_cjk(*c)) && bc.iter().all(|c| is_cjk(*c));
    if !all_cjk {
        return false;
    }
    let shared = ac.iter().filter(|c| bc.contains(c)).count();
    shared * 2 >= short
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF)
}

/// 找出候选的「疑似同人」锚点。
///
/// 只对角色生效：世界观/伏笔的同名条目本来就该是 update，不存在「疑似」的歧义。
/// 锚点优先取既有角色（可直接合并），其次取候选池中的另一条待审候选。
pub fn detect_possible_duplicate(
    title: &str,
    kind: &str,
    existing: &ExistingIndex,
    pool: &[(String, String)],
) -> Option<String> {
    if kind != "character" {
        return None;
    }
    let norm = normalize_title(title);
    if norm.chars().count() < 2 {
        return None;
    }

    if let Some((_, entries)) = existing.iter().find(|(k, _)| k == "character") {
        for (id, name, aliases_json) in entries {
            // 与主名完全相等：resolve_candidate_action 会直接判成 update，
            // 这是「确认同一条」，不是「疑似」，不需要额外提示。
            if norm == normalize_title(name) {
                continue;
            }
            let aliases: Vec<String> = serde_json::from_str(aliases_json).unwrap_or_default();
            let alias_hit = aliases.iter().any(|a| {
                let an = normalize_title(a);
                an == norm || titles_look_same(&norm, &an)
            });
            if alias_hit || titles_look_same(&norm, &normalize_title(name)) {
                return Some(id.clone());
            }
        }
    }

    for (candidate_id, candidate_title) in pool {
        let other = normalize_title(candidate_title);
        if other == norm || titles_look_same(&norm, &other) {
            return Some(candidate_id.clone());
        }
    }
    None
}

/// 分块提炼的章纲合并：每段各给一句，直接入库会变成多条互不相同的「本章梗概」。
fn merge_chunk_summaries(items: &mut Vec<RawCandidate>) {
    let summaries: Vec<RawCandidate> = items
        .iter()
        .filter(|i| i.kind == "chapter_summary")
        .cloned()
        .collect();
    if summaries.len() <= 1 {
        return;
    }

    let mut joined = String::new();
    let mut seen: Vec<String> = Vec::new();
    let mut evidence = String::new();
    for s in &summaries {
        let p = s.payload.trim().to_string();
        if p.is_empty() || seen.contains(&p) {
            continue;
        }
        seen.push(p.clone());
        if !joined.is_empty() {
            joined.push('；');
        }
        joined.push_str(&p);
        if evidence.is_empty() {
            evidence = s.evidence.clone();
        }
    }

    items.retain(|i| i.kind != "chapter_summary");
    if !joined.is_empty() {
        items.push(RawCandidate {
            kind: "chapter_summary".to_string(),
            action: summaries[0].action.clone(),
            title: summaries[0].title.clone(),
            payload: joined,
            evidence,
            target_id: None,
        });
    }
}

fn agent_kinds() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("worldview", "世界观 Agent", "提炼正文中出现的全新时代背景、地理地点、世界运行规则或独特专有名词。"),
        ("character", "角色 Agent", "提炼正文中新登场的人物角色、称号外号、外貌体态、性格行为以及与其他角色的关系演变。"),
        ("plot", "剧情 Agent", "提炼正文中发生的核心戏剧冲突事件、新埋设的伏笔暗示，或某已知伏笔是否在本章获得回收。"),
        ("chapter_summary", "大纲 Agent", "为本章节生成一至两句高度精炼的单章核心梗概摘要（用作章纲）。"),
    ]
}

fn build_agent_prompt(
    kind: &str,
    name: &str,
    instructions: &str,
    chapter_title: &str,
    text: &str,
    existing_block: &str,
) -> String {
    format!(
        "你是一个小说【{}】。\n\
         任务指令：{}\n\
         当前章节标题：《{}》\n\
         当前正文内容如下：\n{}\n\n\
         【设定库中已有的同类条目】（更新已有条目时，必须在 target_id 里填它的 id；\n\
         若正文讲的是全新的人/事/规则，则 action 填 create 且 target_id 留空）：\n\
         {}\n\
         请仔细阅读正文，提炼出结构化的候选条目。\n\
         请务必输出严格的 JSON 数组格式，不得输出其他字符：\n\
         [\n\
           {{\n\
             \"kind\": \"{}\",\n\
             \"action\": \"create 或 update\",\n\
             \"target_id\": \"要更新的既有条目 id；新建则填 null\",\n\
             \"title\": \"条目标题（如角色名、规则名或伏笔名）\",\n\
             \"payload\": \"条目的完整具体描述文本\",\n\
             \"evidence\": \"来自正文的原句证据片段\"\n\
           }}\n\
         ]\n\
         （若本章节中未出现值得提炼的内容，返回空数组 [] 即可）",
        name, instructions, chapter_title, text, existing_block, kind
    )
}

fn build_single_call_prompt(
    chapter_title: &str,
    text: &str,
    existing: &ExistingIndex,
) -> String {
    let mut existing_block = String::new();
    for (kind, label) in [
        ("worldview", "世界观"),
        ("character", "角色"),
        ("plot", "剧情/伏笔"),
    ] {
        existing_block.push_str(&format!("[{}]\n", label));
        existing_block.push_str(&render_existing_for_prompt(existing, kind));
        existing_block.push('\n');
    }

    format!(
        "你是一个小说设定提炼总控 Agent。请一次读完整章，分别提炼四类候选。\n\
         当前章节标题：《{}》\n\
         当前正文内容如下：\n{}\n\n\
         【设定库中已有的同类条目】（更新已有条目时，必须在 target_id 里填它的 id；\n\
         若正文讲的是全新的人/事/规则，则 action 填 create 且 target_id 留空）：\n\
         {}\n\
         请务必输出严格 JSON 对象，不得输出其他字符：\n\
         {{\n\
           \"worldview\": [ {{\"action\": \"create 或 update\", \"target_id\": null, \"title\": \"…\", \"payload\": \"…\", \"evidence\": \"…\"}} ],\n\
           \"character\": [ … ],\n\
           \"plot\": [ … ],\n\
           \"chapter_summary\": [ {{\"action\": \"create 或 update\", \"title\": \"本章梗概\", \"payload\": \"一至两句单章梗概\", \"evidence\": \"…\"}} ]\n\
         }}\n\
         四类都要给（没有内容的给空数组 []）。不要输出 markdown 代码块标记。",
        chapter_title, text, existing_block
    )
}

pub async fn run_parallel_extraction(
    gateway: Arc<AiGateway>,
    pool: DbPool,
    novel: Novel,
    chapter: Chapter,
    profile: LlmProfile,
    api_key: String,
) -> Result<crate::agent::outcome::ExtractionOutcome, String> {
    // 1. Check if chapter text has changed
    let current_hash = compute_chapter_fingerprint(&chapter.content_plain);
    if let Some(ref last_hash) = chapter.extracted_hash {
        if last_hash == &current_hash && !chapter.content_plain.trim().is_empty() {
            // Already extracted with same text, return pending.
            //
            // 注意：这里返回的是**整个作品**的待审候选（list_pending_candidates
            // 只按 novel_id 过滤），不是本章的。旧实现把它当作「本次提炼结果」
            // 直接展示，作者会误以为这些是刚刚提炼出来的。
            // 现在显式标记 skipped_fingerprint_match，由上层如实告知。
            let conn = pool.get().map_err(|e| e.to_string())?;
            let candidates =
                crate::db::extraction::list_pending_candidates(&conn, &novel.id)
                    .map_err(|e| e.to_string())?;
            return Ok(crate::agent::outcome::ExtractionOutcome {
                candidates,
                succeeded_agents: 0,
                failures: Vec::new(),
                skipped_fingerprint_match: true,
                all_succeeded: true,
            });
        }
    }

    // 1.5 读既有设定库与提炼设置：既用于 prompt 注入，也用于动作判定与分块
    let (existing, settings) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let existing = build_existing_index(&conn, &novel.id)?;
        let settings = read_extraction_settings(&conn);
        (existing, settings)
    };

    let chunks = split_into_chunks(&chapter.content_plain, settings.chunk_threshold_chars);

    let mut raw_candidates: Vec<RawCandidate> = Vec::new();
    let mut failures: Vec<crate::agent::outcome::AgentFailure> = Vec::new();
    let mut total_calls = 0usize;

    for (idx, chunk) in chunks.iter().enumerate() {
        let chunk_label = if chunks.len() > 1 {
            format!("第{}段", idx + 1)
        } else {
            String::new()
        };

        match settings.mode {
            ExtractionMode::Parallel4 => {
                // 并发上限 2：四类 Agent 同时打满容易触发服务端限流。
                let semaphore = Arc::new(Semaphore::new(2));
                let mut tasks = Vec::new();

                for (kind, name, instructions) in agent_kinds() {
                    total_calls += 1;
                    let sem = semaphore.clone();
                    let gw = gateway.clone();
                    let prof = profile.clone();
                    let key = api_key.clone();
                    let text = chunk.clone();
                    let ch_title = chapter.title.clone();
                    let existing_block = render_existing_for_prompt(&existing, kind);
                    let prompt =
                        build_agent_prompt(kind, name, instructions, &ch_title, &text, &existing_block);

                    let task = tokio::spawn(async move {
                        let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
                        let messages = vec![ChatMessage {
                            role: "user".to_string(),
                            content: prompt,
                        }];
                        gw.call_sync(&prof, &key, None, messages).await
                    });

                    tasks.push((kind, name, chunk_label.clone(), task));
                }

                for (kind, name, label, task) in tasks {
                    let agent_label = if label.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name}·{label}")
                    };
                    match task.await {
                        Ok(Ok(raw_json)) => match self_heal_and_parse_candidates(&raw_json, kind) {
                            Ok(items) => raw_candidates.extend(items),
                            Err(e) => {
                                let reason = crate::agent::outcome::sanitize_reason(&e);
                                eprintln!("[Extraction] Agent {} 输出解析失败: {}", agent_label, reason);
                                failures.push(crate::agent::outcome::AgentFailure {
                                    agent: agent_label,
                                    kind: crate::agent::outcome::ExtractionFailureKind::MalformedOutput,
                                    reason,
                                });
                            }
                        },
                        Ok(Err(e)) => {
                            let kind_class = crate::agent::outcome::classify_error(&e);
                            let reason = crate::agent::outcome::sanitize_reason(&e);
                            eprintln!(
                                "[Extraction] Agent {} 执行失败（{:?}）: {}",
                                agent_label, kind_class, reason
                            );
                            failures.push(crate::agent::outcome::AgentFailure {
                                agent: agent_label,
                                kind: kind_class,
                                reason,
                            });
                        }
                        Err(e) => {
                            eprintln!("[Extraction] Agent {} JoinError: {}", agent_label, e);
                            failures.push(crate::agent::outcome::AgentFailure {
                                agent: agent_label,
                                kind: crate::agent::outcome::ExtractionFailureKind::Internal,
                                reason: "Agent 任务异常终止".to_string(),
                            });
                        }
                    }
                }
            }
            ExtractionMode::SingleCall => {
                total_calls += 1;
                let agent_label = if chunk_label.is_empty() {
                    "单次提炼".to_string()
                } else {
                    format!("单次提炼·{chunk_label}")
                };
                let prompt = build_single_call_prompt(&chapter.title, chunk, &existing);
                let messages = vec![ChatMessage {
                    role: "user".to_string(),
                    content: prompt,
                }];
                match gateway.call_sync(&profile, &api_key, None, messages).await {
                    Ok(raw_json) => match parse_single_call_candidates(&raw_json) {
                        Ok(items) => raw_candidates.extend(items),
                        Err(e) => {
                            let reason = crate::agent::outcome::sanitize_reason(&e);
                            eprintln!("[Extraction] {} 输出解析失败: {}", agent_label, reason);
                            failures.push(crate::agent::outcome::AgentFailure {
                                agent: agent_label,
                                kind: crate::agent::outcome::ExtractionFailureKind::MalformedOutput,
                                reason,
                            });
                        }
                    },
                    Err(e) => {
                        let kind_class = crate::agent::outcome::classify_error(&e);
                        let reason = crate::agent::outcome::sanitize_reason(&e);
                        eprintln!(
                            "[Extraction] {} 执行失败（{:?}）: {}",
                            agent_label, kind_class, reason
                        );
                        failures.push(crate::agent::outcome::AgentFailure {
                            agent: agent_label,
                            kind: kind_class,
                            reason,
                        });
                    }
                }
            }
        }
    }

    // 分块提炼的章纲需要合并成一条，否则一次提炼会产出多条互不相同的「本章梗概」。
    if chunks.len() > 1 {
        merge_chunk_summaries(&mut raw_candidates);
    }

    // 2. 入库：解析动作、去重、疑似同人标记
    let mut extracted_candidates = Vec::new();
    let mut all_succeeded = failures.is_empty();
    let mut candidate_pool: Vec<(String, String)> = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::db::extraction::list_pending_candidates(&conn, &novel.id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|c| c.kind == "character")
            .map(|c| (c.id, c.title))
            .collect()
    };

    for item in raw_candidates {
        let (resolved, dedup_key) = if item.kind == "chapter_summary" {
            let action = if chapter.summary.trim().is_empty() {
                "create".to_string()
            } else {
                "update".to_string()
            };
            let key = compute_chapter_summary_dedup_key(&chapter.id, &item.payload);
            (
                ResolvedAction {
                    action,
                    target_id: Some(chapter.id.clone()),
                },
                key,
            )
        } else {
            let res = resolve_candidate_action(
                &item.action,
                item.target_id.as_deref(),
                &item.title,
                &item.kind,
                &existing,
            );
            let key = if res.action == "update" {
                compute_update_dedup_key(
                    &item.kind,
                    res.target_id.as_deref().unwrap_or(&item.title),
                    &item.payload,
                )
            } else {
                compute_dedup_key(&item.kind, &item.title)
            };
            (res, key)
        };

        let possible_duplicate_of =
            detect_possible_duplicate(&item.title, &item.kind, &existing, &candidate_pool);

        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                all_succeeded = false;
                eprintln!("[Extraction] 获取数据库连接失败: {}", e);
                failures.push(crate::agent::outcome::AgentFailure {
                    agent: item.kind.clone(),
                    kind: crate::agent::outcome::ExtractionFailureKind::Database,
                    reason: "获取数据库连接失败".to_string(),
                });
                continue;
            }
        };

        match crate::db::extraction::is_candidate_ignored_or_accepted(
            &conn,
            &novel.id,
            &dedup_key,
        ) {
            Ok(false) => {
                let agent_id = match item.kind.as_str() {
                    "worldview" => "agent-worldview",
                    "character" => "agent-character",
                    "plot" => "agent-plot",
                    _ => "agent-outline",
                };

                match crate::db::extraction::insert_candidate(
                    &conn,
                    &novel.id,
                    &chapter.id,
                    agent_id,
                    &item.kind,
                    &resolved.action,
                    resolved.target_id.as_deref(),
                    &item.title,
                    &item.payload,
                    &item.evidence,
                    &dedup_key,
                    possible_duplicate_of.as_deref(),
                ) {
                    Ok(cand) => {
                        if cand.kind == "character" {
                            candidate_pool.push((cand.id.clone(), cand.title.clone()));
                        }
                        extracted_candidates.push(cand);
                    }
                    Err(e) => {
                        all_succeeded = false;
                        eprintln!(
                            "[Extraction] 写入候选失败 (kind: {}): {}",
                            item.kind, e
                        );
                        failures.push(crate::agent::outcome::AgentFailure {
                            agent: item.kind.clone(),
                            kind: crate::agent::outcome::ExtractionFailureKind::Database,
                            reason: "写入候选记录失败".to_string(),
                        });
                    }
                }
            }
            Ok(true) => {
                // 既有重复条目，正常略过
            }
            Err(e) => {
                all_succeeded = false;
                eprintln!(
                    "[Extraction] 查询候选去重状态失败 (kind: {}): {}",
                    item.kind, e
                );
                failures.push(crate::agent::outcome::AgentFailure {
                    agent: item.kind.clone(),
                    kind: crate::agent::outcome::ExtractionFailureKind::Database,
                    reason: "查询候选去重状态失败".to_string(),
                });
            }
        }
    }

    // 关键防御：仅当所有调用均成功执行返回、解析成功且入库无任何错误时，才更新 extracted_hash！
    // 一旦有任何环节失败（如 429 限流、输出格式畸形、SQL 写失败），绝不打 hash 戳，
    // 使得用户再次触发提炼时，能够重新跑并有机会补齐缺失分类的候选。
    if all_succeeded {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE chapter SET extracted_hash = ?1 WHERE id = ?2",
            rusqlite::params![current_hash, chapter.id],
        )
        .map_err(|e| format!("更新章节 extracted_hash 失败: {}", e))?;
    } else {
        eprintln!(
            "[Extraction] 部分调用、解析或入库未完全成功，跳过写入 extracted_hash 以支持后续重试补齐 (chapter_id: {})",
            chapter.id
        );
    }

    Ok(crate::agent::outcome::ExtractionOutcome {
        succeeded_agents: total_calls.saturating_sub(failures.len()),
        candidates: extracted_candidates,
        failures,
        skipped_fingerprint_match: false,
        all_succeeded,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    fn index_with_character() -> ExistingIndex {
        vec![(
            "character".to_string(),
            vec![(
                "char-1".to_string(),
                "李四".to_string(),
                r#"["四哥","李四公子"]"#.to_string(),
            )],
        )]
    }

    #[test]
    fn valid_target_id_wins() {
        let r = resolve_candidate_action(
            "update",
            Some("char-1"),
            "随便什么标题",
            "character",
            &index_with_character(),
        );
        assert_eq!(r.action, "update");
        assert_eq!(r.target_id.as_deref(), Some("char-1"));
    }

    #[test]
    fn dangling_target_id_falls_back_to_title_match() {
        // 模型给了个不存在的 id，但标题能对上既有条目 —— 仍应判为 update
        let r = resolve_candidate_action(
            "update",
            Some("does-not-exist"),
            "李四",
            "character",
            &index_with_character(),
        );
        assert_eq!(r.action, "update");
        assert_eq!(r.target_id.as_deref(), Some("char-1"));
    }

    #[test]
    fn title_and_alias_matching_is_whitespace_and_case_insensitive() {
        for title in ["李四", " 李四 ", "四哥", "李四公子"] {
            let r = resolve_candidate_action("create", None, title, "character", &index_with_character());
            assert_eq!(r.action, "update", "标题 {title} 应命中既有角色");
            assert_eq!(r.target_id.as_deref(), Some("char-1"));
        }
    }

    #[test]
    fn genuinely_new_entry_stays_create_without_dangling_reference() {
        let r = resolve_candidate_action(
            "update",
            Some("nope"),
            "王五",
            "character",
            &index_with_character(),
        );
        assert_eq!(r.action, "create");
        assert_eq!(r.target_id, None, "新建候选绝不能带上不存在的 target_id");
    }

    #[test]
    fn target_id_is_scoped_to_its_own_kind() {
        // 拿 role 的 id 去更新 worldview，必须无效
        let r = resolve_candidate_action(
            "update",
            Some("char-1"),
            "某条世界观",
            "worldview",
            &index_with_character(),
        );
        assert_eq!(r.action, "create");
        assert_eq!(r.target_id, None);
    }

    #[test]
    fn dedup_key_ignores_whitespace_and_case() {
        assert_eq!(
            compute_dedup_key("character", "李 四"),
            compute_dedup_key("character", "李四")
        );
    }

    #[test]
    fn self_heal_markdown_and_chatter() {
        let raw = r#"
好的，这是我为您从第3章提炼出的角色设定：
```json
[
  {
    "kind": "character",
    "action": "create",
    "title": "韩世忠",
    "payload": "西军名将，骁勇善战，号称韩五",
    "evidence": "韩世忠提刀立马"
  }
]
```
请您审阅！如有变动随时告知。
"#;
        let cands = self_heal_and_parse_candidates(raw, "character").expect("解析成功");
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].title, "韩世忠");
        assert_eq!(cands[0].kind, "character");
        assert_eq!(cands[0].action, "create");
        assert_eq!(cands[0].evidence, "韩世忠提刀立马");
    }

    #[test]
    fn self_heal_single_object_drift() {
        let raw = r#"
{
  "kind": "worldview",
  "action": "create",
  "title": "御营司",
  "payload": "建炎南渡初期掌控禁军的大营机枢",
  "evidence": "御营司调兵符"
}
"#;
        let cands = self_heal_and_parse_candidates(raw, "worldview").expect("解析成功");
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].title, "御营司");
        assert_eq!(cands[0].kind, "worldview");
    }

    #[test]
    fn self_heal_trailing_commas_and_comments() {
        let raw = r#"
[
  {
    "kind": "plot",
    "action": "create",
    "title": "明道宫兵变伏笔",
    "payload": "苗傅、刘正彦部下心怀不满，暗中密谋 // 伏笔埋设",
    "evidence": "苗刘二人对坐饮酒不乐",
  },
]
"#;
        let cands = self_heal_and_parse_candidates(raw, "plot").expect("解析成功");
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].title, "明道宫兵变伏笔");
        assert!(cands[0].payload.contains("暗中密谋"));
    }

    #[test]
    fn self_heal_truncated_json() {
        // 模拟大模型输出到达 max_tokens 产生尾部截断
        let raw = r#"
[
  {
    "kind": "character",
    "action": "create",
    "title": "赵玖",
    "payload": "大宋官家，穿越者",
    "evidence": "朕要亲自看一看"
  },
  {
    "kind": "character",
    "action": "create",
    "title": "汪伯彦",
    "payload": "主和派宰执，力主南逃
"#;
        let cands = self_heal_and_parse_candidates(raw, "character").expect("解析成功");
        // 截断自修复应当抢救下已完整闭合的第一个对象“赵玖”
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].title, "赵玖");
    }

    #[test]
    fn self_heal_field_aliases_drift() {
        let raw = r#"
[
  {
    "type": "character",
    "name": "梁红玉",
    "content": "韩世忠之妻，击鼓退金兵",
    "quote": "夫人亲擂战鼓",
    "targetId": "char-999"
  }
]
"#;
        let cands = self_heal_and_parse_candidates(raw, "character").expect("解析成功");
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].title, "梁红玉");
        assert_eq!(cands[0].payload, "韩世忠之妻，击鼓退金兵");
        assert_eq!(cands[0].evidence, "夫人亲擂战鼓");
        assert_eq!(cands[0].target_id.as_deref(), Some("char-999"));
    }

    #[test]
    fn self_heal_empty_title_fallback_from_payload() {
        let raw = r#"
[
  {
    "kind": "chapter_summary",
    "title": "",
    "payload": "官家行在移驻下邳，军心初定，韩世忠引军来会。",
    "evidence": ""
  }
]
"#;
        let cands = self_heal_and_parse_candidates(raw, "chapter_summary").expect("解析成功");
        assert_eq!(cands.len(), 1);
        assert!(!cands[0].title.is_empty(), "当title为空时应自修复从payload提取");
        assert_eq!(cands[0].payload, "官家行在移驻下邳，军心初定，韩世忠引军来会。");
    }

    #[test]
    fn self_heal_legitimate_empty_array() {
        // 合法空数组：模型明确告知无提炼项，必须成功返回空列表而不是报错
        let empty1 = "[]";
        let empty2 = "```json\n[]\n```";
        let empty3 = "  [ \n // 本章无提炼项 \n ] ";
        let empty4 = "本章节没有需要提炼的设定：\n```json\n[]\n```";

        assert_eq!(self_heal_and_parse_candidates(empty1, "worldview").unwrap().len(), 0);
        assert_eq!(self_heal_and_parse_candidates(empty2, "character").unwrap().len(), 0);
        assert_eq!(self_heal_and_parse_candidates(empty3, "plot").unwrap().len(), 0);
        assert_eq!(self_heal_and_parse_candidates(empty4, "chapter_summary").unwrap().len(), 0);
    }

    #[test]
    fn self_heal_unparseable_gibberish_returns_err() {
        // 非法内容、纯报错文本或严重破损无候选的内容，必须返回 Err 以触发失败保护
        let err1 = "";
        let err2 = "抱歉，由于网络限制，我无法回答该问题。";
        let err3 = "这是一个没有返回任何 JSON 的模型纯文本回答。";
        let err4 = "[ { \"invalid_field\": 123 } ]";

        assert!(self_heal_and_parse_candidates(err1, "worldview").is_err());
        assert!(self_heal_and_parse_candidates(err2, "character").is_err());
        assert!(self_heal_and_parse_candidates(err3, "plot").is_err());
        assert!(self_heal_and_parse_candidates(err4, "chapter_summary").is_err());
    }

    #[test]
    fn test_chapter_summary_dedup_key_behavior() {
        let ch_id = "chap-001";
        let payload1 = "赵玖下邳驻跸，召见韩世忠商议抗金对策。";
        let payload2 = "赵玖下邳遇刺，韩世忠引军平乱护驾。";

        let key1 = compute_chapter_summary_dedup_key(ch_id, payload1);
        let key1_same = compute_chapter_summary_dedup_key(ch_id, "  赵玖下邳驻跸，召见韩世忠商议抗金对策。 \n");
        let key2 = compute_chapter_summary_dedup_key(ch_id, payload2);
        let key_other_chap = compute_chapter_summary_dedup_key("chap-002", payload1);

        // 相同章节且正规化后相同文本：生成相同 dedup_key，能够去重
        assert_eq!(key1, key1_same);

        // 相同章节但内容发生修改：生成不同 dedup_key，不再被历史已采纳记录死锁拦截
        assert_ne!(key1, key2);

        // 不同章节即便摘要文本凑巧相同：也生成不同 key
        assert_ne!(key1, key_other_chap);
    }

    // ---- T07：分块 / single_call / 疑似同人 ----

    #[test]
    fn chunking_splits_on_line_boundaries_and_keeps_order() {
        let text = "第一段。\n第二段。\n第三段。\n第四段。";
        // 阈值 8 字符：一行 4 字，两行 + 换行正好 9 > 8，因此按两行一组切
        let chunks = split_into_chunks(text, 8);
        assert!(chunks.len() >= 2, "应切成多块：{chunks:?}");
        for c in &chunks {
            assert!(c.chars().count() <= 8, "每块不得超过阈值：{:?}", c);
        }
        let rejoined = chunks.join("\n");
        assert_eq!(
            rejoined.replace("\n\n", "\n"),
            text,
            "切分不得丢字或改字"
        );
    }

    #[test]
    fn short_text_is_a_single_chunk() {
        let chunks = split_into_chunks("只有一行。", 12_000);
        assert_eq!(chunks, vec!["只有一行。".to_string()]);
    }

    #[test]
    fn oversized_single_line_is_hard_split() {
        let text: String = "字".repeat(25);
        let chunks = split_into_chunks(&text, 10);
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.chars().count() <= 10));
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn single_call_parses_grouped_object_into_four_kinds() {
        let raw = r#"好的，结果如下：
{"worldview":[{"action":"create","title":"东京","payload":"都城","evidence":"东京"}],
 "character":[{"action":"create","title":"林舟","payload":"主角","evidence":"林舟"}],
 "plot":[{"action":"create","title":"断水","payload":"伏笔","evidence":"断水"}],
 "chapter_summary":[{"action":"create","title":"梗概","payload":"沈砚入京。","evidence":"沈砚"}]}"#;
        let items = parse_single_call_candidates(raw).expect("分组对象应可解析");
        assert_eq!(items.len(), 4);
        let kinds: Vec<&str> = items.iter().map(|i| i.kind.as_str()).collect();
        for expected in CANDIDATE_KINDS {
            assert!(kinds.contains(&expected), "缺少 {expected}: {kinds:?}");
        }
    }

    #[test]
    fn single_call_also_accepts_flat_array_with_kind() {
        // 模型经常「回退」到并行模式的输出结构；两种形态必须同构。
        let raw = r#"[
          {"kind":"worldview","action":"create","title":"东京","payload":"都城","evidence":"东京"},
          {"kind":"character","action":"create","title":"林舟","payload":"主角","evidence":"林舟"}
        ]"#;
        let items = parse_single_call_candidates(raw).expect("数组形态应可解析");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "worldview");
        assert_eq!(items[1].kind, "character");
    }

    #[test]
    fn single_call_rejects_output_without_known_kinds() {
        let raw = r#"[{"kind":"unknown","action":"create","title":"x","payload":"y","evidence":"z"}]"#;
        assert!(parse_single_call_candidates(raw).is_err());
    }

    #[test]
    fn similar_name_rules_are_advisory_and_conservative() {
        // 共享一个字的两字名（林舟 / 小舟）→ 疑似
        assert!(titles_look_same("林舟", "小舟"));
        // 后缀漂移（李四 / 李四（师兄））→ 疑似
        assert!(titles_look_same("李四", "李四（师兄）"));
        // 完全不同的名字 → 不提示
        assert!(!titles_look_same("林舟", "沈砚"));
        // 拉丁名不参与「共享字符」判定（避免 John / Joan 这类误报）
        assert!(!titles_look_same("John", "Joan"));
    }

    #[test]
    fn duplicate_detection_prefers_existing_character_then_pool() {
        let existing: ExistingIndex = vec![(
            "character".to_string(),
            vec![("c-lin".to_string(), "林舟".to_string(), r#"["小舟"]"#.to_string())],
        )];

        // 与别名完全一致
        assert_eq!(
            detect_possible_duplicate("小舟", "character", &existing, &[]),
            Some("c-lin".to_string())
        );
        // 共享一个字（fuzzy）
        assert_eq!(
            detect_possible_duplicate("小舟", "character", &existing, &[]),
            Some("c-lin".to_string())
        );
        // 主名完全一致不算「疑似」（resolve 会判 update）
        assert_eq!(detect_possible_duplicate("林舟", "character", &existing, &[]), None);
        // 非角色类别不参与
        assert_eq!(detect_possible_duplicate("小舟", "worldview", &existing, &[]), None);

        // 候选池命中：池内「沈砚」与候选「沈砚之」是包含关系
        let pool = vec![("cand-1".to_string(), "沈砚".to_string())];
        assert_eq!(
            detect_possible_duplicate("沈砚之", "character", &ExistingIndex::new(), &pool),
            Some("cand-1".to_string())
        );
        // 池内完全不同的名字不提示
        let unrelated = vec![("cand-x".to_string(), "赵玖".to_string())];
        assert_eq!(
            detect_possible_duplicate("沈砚", "character", &ExistingIndex::new(), &unrelated),
            None
        );
        let pool2 = vec![("cand-2".to_string(), "林舟".to_string())];
        assert_eq!(
            detect_possible_duplicate("小舟", "character", &ExistingIndex::new(), &pool2),
            Some("cand-2".to_string())
        );
    }

    #[test]
    fn chunked_summaries_are_merged_into_one_candidate() {
        let mut items = vec![
            RawCandidate {
                kind: "chapter_summary".to_string(),
                action: "create".to_string(),
                title: "梗概".to_string(),
                payload: "前半章。".to_string(),
                evidence: "e1".to_string(),
                target_id: None,
            },
            RawCandidate {
                kind: "chapter_summary".to_string(),
                action: "create".to_string(),
                title: "梗概".to_string(),
                payload: "后半章。".to_string(),
                evidence: "e2".to_string(),
                target_id: None,
            },
        ];
        merge_chunk_summaries(&mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].payload, "前半章。；后半章。");
    }
}

