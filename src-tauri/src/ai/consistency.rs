//! 设定冲突检测（T08）。
//!
//! 职责边界：
//!   * 把「喂给模型的设定条目」构造成带 `[类型#id]` 的候选清单，让模型的
//!     关联设定可以落到库内具体条目上，而不是只有一句「角色·沈砚」；
//!   * 解析模型的固定 JSON（冲突点 / 关联设定 / 证据 / 置信度 / 原因 / 建议）；
//!   * 用作者配置的开关、检查范围与置信度阈值做闸门；
//!   * 对「本章已忽略的此类提示」直接放行。
//!
//! 三种「不弹窗」的结果刻意分开（见 `ConsistencyOutcome`）：
//! 关闭检查（`Disabled≠Clean`）、低于阈值（`Suppressed`）、没测成（`NotChecked`）。
//! 把它们合并成 `Clean` 会让作者误以为「检查过且没问题」。
use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use rusqlite::Connection;
use serde::Deserialize;
use shared_types::{ConflictInfo, ConflictSettingRef, ConsistencyOutcome, LlmProfile};
use std::collections::HashSet;

/// 内容过短时连模型都不必调用。
const MIN_CHECK_LEN: usize = 20;

/// 单条设定进入冲突检测上下文时的上限，避免一条超长条目吃掉整个预算。
const MAX_SINGLE_ENTRY_CHARS: usize = 600;

/// 置信度缺省阈值（作者可在设置页覆盖）。
pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.6;

/// 一次冲突最多回填几条关联设定 —— 弹窗要能读，不是索引。
const MAX_RELATED_SETTINGS: usize = 3;

const CONTEXT_HEADER: &str = "【设定条目】（每条以 [类型#id] 开头）\n";

// ---------- 设置项 ----------

/// 检查范围：全部设定 / 仅被标记为「硬性」的设定（T06 的 pinned）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckScope {
    All,
    PinnedOnly,
}

impl CheckScope {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "pinned" | "pinned_only" => Self::PinnedOnly,
            _ => Self::All,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::PinnedOnly => "pinned",
        }
    }
}

/// 冲突检测的三项设置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConflictCheckSettings {
    /// 前置检查开关。关闭后 `ai_check_conflict` 直接返回，不碰模型与凭据。
    pub enabled: bool,
    pub scope: CheckScope,
    /// 低于此值的结果不弹窗。
    pub threshold: f32,
}

impl Default for ConflictCheckSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: CheckScope::All,
            threshold: DEFAULT_CONFIDENCE_THRESHOLD,
        }
    }
}

impl ConflictCheckSettings {
    /// 从 `app_setting` 读取；缺键或读失败一律回落到默认值。
    ///
    /// 设置读取失败不阻断写作：默认值是「开启检查 + 全部设定 + 0.6」，
    /// 与产品默认一致，而不是悄悄变成「不检查」。
    pub fn from_conn(conn: &Connection) -> Self {
        let read = |key: &str| {
            crate::db::app_setting::get_setting(conn, key)
                .ok()
                .flatten()
        };

        let enabled = read("conflict_check_enabled")
            .map(|v| matches!(v.trim(), "true" | "1" | "yes" | "on"))
            .unwrap_or(true);
        let scope = read("conflict_check_scope")
            .map(|v| CheckScope::parse(&v))
            .unwrap_or(CheckScope::All);
        let threshold = read("conflict_confidence_threshold")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .map(normalize_confidence_value)
            .unwrap_or(DEFAULT_CONFIDENCE_THRESHOLD);

        Self {
            enabled,
            scope,
            threshold,
        }
    }
}

// ---------- 上下文构建 ----------

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// 构造冲突检测用的设定上下文。
///
/// 返回 `(context_text, candidates)`：
///   * `context_text` 只含**进入提示词**的条目，按 `pinned` 优先、随后按列表顺序，
///     受 `budget_chars` 限制；
///   * `candidates` 是**全量**（同样受检查范围约束）的条目，用于校验模型返回的 id
///     并把标题换成库内权威值 —— 即使某条因预算被裁掉，只要模型确实引用了它，
///     仍然能解析出来。
pub fn build_conflict_context(
    conn: &Connection,
    novel_id: &str,
    scope: CheckScope,
    budget_chars: usize,
) -> rusqlite::Result<(String, Vec<ConflictSettingRef>)> {
    let mut candidates: Vec<ConflictSettingRef> = Vec::new();
    let mut pinned_lines: Vec<String> = Vec::new();
    let mut normal_lines: Vec<String> = Vec::new();

    let mut push = |line: String, pinned: bool| {
        if pinned {
            pinned_lines.push(line);
        } else {
            normal_lines.push(line);
        }
    };

    for wv in crate::db::settings::list_worldview(conn, novel_id)? {
        if scope == CheckScope::PinnedOnly && !wv.pinned {
            continue;
        }
        candidates.push(ConflictSettingRef {
            entity_type: "worldview".to_string(),
            id: wv.id.clone(),
            title: wv.title.clone(),
        });
        push(
            format!(
                "- [worldview#{}] {}: {}",
                wv.id,
                wv.title,
                truncate_chars(&wv.content, MAX_SINGLE_ENTRY_CHARS)
            ),
            wv.pinned,
        );
    }

    for c in crate::db::settings::list_characters(conn, novel_id)? {
        if scope == CheckScope::PinnedOnly && !c.pinned {
            continue;
        }
        candidates.push(ConflictSettingRef {
            entity_type: "character".to_string(),
            id: c.id.clone(),
            title: c.name.clone(),
        });
        push(
            format!(
                "- [character#{}] {}(别名: {}): {}",
                c.id,
                c.name,
                c.aliases,
                truncate_chars(&c.profile, MAX_SINGLE_ENTRY_CHARS)
            ),
            c.pinned,
        );
    }

    for p in crate::db::settings::list_plots(conn, novel_id)? {
        if scope == CheckScope::PinnedOnly && !p.pinned {
            continue;
        }
        candidates.push(ConflictSettingRef {
            entity_type: "plot".to_string(),
            id: p.id.clone(),
            title: p.title.clone(),
        });
        push(
            format!(
                "- [plot#{}] {}: {}",
                p.id,
                p.title,
                truncate_chars(&p.content, MAX_SINGLE_ENTRY_CHARS)
            ),
            p.pinned,
        );
    }

    let mut text = String::from(CONTEXT_HEADER);
    let mut used = text.chars().count();
    for line in pinned_lines.iter().chain(normal_lines.iter()) {
        let cost = line.chars().count() + 1;
        if used + cost > budget_chars {
            continue;
        }
        text.push_str(line);
        text.push('\n');
        used += cost;
    }

    Ok((text, candidates))
}

// ---------- 关联设定解析 ----------

fn normalize_entity_type(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "worldview" | "世界观" | "设定" => Some("worldview"),
        "character" | "角色" | "人物" => Some("character"),
        "plot" | "剧情" | "大纲" | "伏笔" => Some("plot"),
        _ => None,
    }
}

fn entity_type_of_module(module: &str) -> Option<&'static str> {
    let m = module.trim();
    if m.contains("世界观") || m.contains("世界") {
        Some("worldview")
    } else if m.contains("角色") || m.contains("人物") {
        Some("character")
    } else if m.contains("剧情") || m.contains("大纲") || m.contains("伏笔") {
        Some("plot")
    } else {
        normalize_entity_type(m)
    }
}

fn entity_type_label(entity_type: &str) -> &'static str {
    match entity_type {
        "worldview" => "世界观",
        "character" => "角色",
        "plot" => "剧情",
        _ => "设定",
    }
}

fn find_by_id<'a>(
    candidates: &'a [ConflictSettingRef],
    entity_type: &str,
    id: &str,
) -> Option<&'a ConflictSettingRef> {
    candidates
        .iter()
        .find(|c| c.entity_type == entity_type && c.id == id)
}

fn find_by_title<'a>(
    candidates: &'a [ConflictSettingRef],
    entity_type: Option<&str>,
    title: &str,
) -> Option<&'a ConflictSettingRef> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let in_scope = |c: &&ConflictSettingRef| match entity_type {
        Some(t) => c.entity_type == t,
        None => true,
    };
    // 先精确，再包含：模型偶尔会把「沈砚（主角）」写成「沈砚」。
    candidates
        .iter()
        .filter(in_scope)
        .find(|c| c.title.trim() == title)
        .or_else(|| {
            candidates
                .iter()
                .filter(in_scope)
                .find(|c| c.title.contains(title) || title.contains(c.title.trim()))
        })
}

/// 校验模型给出的关联设定，并把标题换成库内权威值。
///
/// 同时计算「本章忽略此类提示」的签名：
///   * 有可定位条目 → `<entity_type>:<id>`（忽略「沈砚这个角色」这一类的提示）；
///   * 定位不到 → `module:<类型>`（按 `setting_module` 归类，最差落到 `general`）。
pub fn resolve_related_settings(info: &mut ConflictInfo, candidates: &[ConflictSettingRef]) {
    let mut resolved: Vec<ConflictSettingRef> = Vec::new();

    for raw in std::mem::take(&mut info.related_settings) {
        if resolved.len() >= MAX_RELATED_SETTINGS {
            break;
        }
        let Some(entity_type) = normalize_entity_type(&raw.entity_type) else {
            continue;
        };
        let found = if !raw.id.trim().is_empty() {
            find_by_id(candidates, entity_type, raw.id.trim())
        } else {
            find_by_title(candidates, Some(entity_type), &raw.title)
        };
        let Some(found) = found else { continue };
        if resolved
            .iter()
            .any(|r| r.entity_type == found.entity_type && r.id == found.id)
        {
            continue;
        }
        resolved.push(found.clone());
    }

    if resolved.is_empty() {
        if let Some(found) = find_by_title(
            candidates,
            entity_type_of_module(&info.setting_module),
            &info.setting_title,
        ) {
            resolved.push(found.clone());
        }
    }

    if let Some(first) = resolved.first() {
        info.ignore_signature = format!("{}:{}", first.entity_type, first.id);
        info.ignore_label = format!("{}·{}", entity_type_label(&first.entity_type), first.title);
    } else {
        let mapped = entity_type_of_module(&info.setting_module);
        info.ignore_signature = format!("module:{}", mapped.unwrap_or("general"));
        info.ignore_label = if info.setting_title.trim().is_empty() {
            format!("{}类提示", entity_type_label(mapped.unwrap_or("general")))
        } else {
            format!(
                "{}类提示（{}）",
                entity_type_label(mapped.unwrap_or("general")),
                info.setting_title.trim()
            )
        };
    }

    info.related_settings = resolved;
}

// ---------- 阈值与忽略闸门 ----------

/// 把「检测结果」过一遍作者的策略：
///   1. 本章已忽略的签名 → `Clean`（作者已经明确过不要再提示）；
///   2. 置信度低于阈值 → `Suppressed`（保留结果，但不弹窗）；
///   3. 其余原样返回。
///
/// `confidence` 为 `None`（模型没给）时不抑制 —— 阈值是过滤「模型自己都不确定」的
/// 结果，而不是把「没给数字」一律当成低置信。
pub fn apply_gate(
    outcome: ConsistencyOutcome,
    threshold: f32,
    ignored: &HashSet<String>,
) -> ConsistencyOutcome {
    match outcome {
        ConsistencyOutcome::Conflict { info } => {
            if !info.ignore_signature.is_empty() && ignored.contains(&info.ignore_signature) {
                return ConsistencyOutcome::Clean;
            }
            match info.confidence {
                Some(c) if c < threshold => ConsistencyOutcome::Suppressed { info, threshold },
                _ => ConsistencyOutcome::Conflict { info },
            }
        }
        other => other,
    }
}

// ---------- 解析 ----------

#[derive(Debug, Deserialize)]
struct RawSettingRef {
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConflictCheckResult {
    has_conflict: bool,
    #[serde(default)]
    conflict_point: Option<String>,
    #[serde(default)]
    setting_module: Option<String>,
    #[serde(default)]
    setting_title: Option<String>,
    #[serde(default)]
    related_settings: Vec<RawSettingRef>,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    proposed_change: Option<String>,
}

/// 置信度的容错：`85` 这类百分数按 0.85 处理，其余夹到 `[0,1]`。
fn normalize_confidence_value(v: f32) -> f32 {
    if !v.is_finite() {
        return DEFAULT_CONFIDENCE_THRESHOLD;
    }
    let v = if v > 1.0 && v <= 100.0 { v / 100.0 } else { v };
    v.clamp(0.0, 1.0)
}

fn normalize_confidence(v: Option<f32>) -> Option<f32> {
    v.filter(|c| c.is_finite()).map(normalize_confidence_value)
}

/// 把模型的原始返回解释成结构化结果。
///
/// 抽成纯函数的两个理由：
///   1. 「模型返回了非 JSON」是必须被单独断言的分支，纯函数让它可单测
///      （不需要真的调一次模型来制造这个输入）；
///   2. 旧实现在解析失败时用 if let Ok(..) 静默跳过、返回 None，
///      上层看到的是「无冲突」—— 那是静默放行。这里把「未检测」变成一等公民。
pub fn interpret_response(raw: &str) -> ConsistencyOutcome {
    let json_str = extract_json_block(raw);
    match serde_json::from_str::<ConflictCheckResult>(&json_str) {
        Ok(res) => {
            if res.has_conflict {
                let related_settings = res
                    .related_settings
                    .into_iter()
                    .map(|r| ConflictSettingRef {
                        entity_type: r.entity_type.unwrap_or_default(),
                        id: r.id.unwrap_or_default(),
                        title: r.title.unwrap_or_default(),
                    })
                    .filter(|r| !r.id.trim().is_empty() || !r.title.trim().is_empty())
                    .collect();

                ConsistencyOutcome::Conflict {
                    info: ConflictInfo {
                        conflict_point: res
                            .conflict_point
                            .unwrap_or_else(|| "正文与既有设定存在出入".to_string()),
                        setting_module: res
                            .setting_module
                            .unwrap_or_else(|| "设定库".to_string()),
                        setting_title: res
                            .setting_title
                            .unwrap_or_else(|| "未命名条目".to_string()),
                        reason: res
                            .reason
                            .unwrap_or_else(|| "检测到正文逻辑与已知设定矛盾".to_string()),
                        proposed_change: res.proposed_change,
                        related_settings,
                        evidence: res
                            .evidence
                            .map(|e| e.trim().to_string())
                            .unwrap_or_default(),
                        confidence: normalize_confidence(res.confidence),
                        // 签名需要与库内条目对照，由 `resolve_related_settings` 填。
                        ignore_signature: String::new(),
                        ignore_label: String::new(),
                    },
                }
            } else {
                ConsistencyOutcome::Clean
            }
        }
        Err(e) => ConsistencyOutcome::NotChecked {
            reason: format!("模型返回的内容不是可解析的判定 JSON：{e}"),
        },
    }
}

/// 检查待写入正文是否与既有设定冲突。
///
/// 返回结构化结果：调用方必须能区分「干净」「冲突」「低于阈值」「未检测」「已关闭」。
pub struct ConflictCheckInput<'a, 'b> {
    pub context_text: &'a str,
    pub generated_text: &'a str,
    pub candidates: &'a [ConflictSettingRef],
    pub threshold: f32,
    pub ignored: &'b HashSet<String>,
}

pub async fn check_consistency(
    gateway: &AiGateway,
    input: ConflictCheckInput<'_, '_>,
    profile: &LlmProfile,
    api_key: &str,
) -> ConsistencyOutcome {
    let trimmed = input.generated_text.trim();
    if trimmed.chars().count() < MIN_CHECK_LEN {
        return ConsistencyOutcome::NotChecked {
            reason: format!("内容不足 {MIN_CHECK_LEN} 字，未做冲突检测"),
        };
    }

    let prompt = format!(
        "你是一个极其严格的小说逻辑与设定审核员。\n\
         请对照下述设定条目，检查即将写入正文的内容是否存在前后矛盾、人设冲突、\n\
         世界法则违背或伏笔崩坏。\n\n\
         {context}\n\n\
         【待检查的正文内容】：\n{text}\n\n\
         请务必以严格的 JSON 格式输出判定结果，不得附加任何多余说明：\n\
         {{\n\
           \"has_conflict\": true 或 false,\n\
           \"conflict_point\": \"矛盾核心点概述（若无冲突为 null）\",\n\
           \"related_settings\": [{{\"entity_type\": \"worldview|character|plot\", \"id\": \"设定条目的 id（只填设定条目里出现过的 id）\", \"title\": \"条目名称\"}}],\n\
           \"evidence\": \"判定依据：从上述设定或待检查正文中摘录的原句（若无冲突为空字符串）\",\n\
           \"confidence\": 0.0 到 1.0 之间的数字,\n\
           \"setting_module\": \"涉及模块（世界观 / 角色 / 剧情 / 大纲）\",\n\
           \"setting_title\": \"涉及的具体条目名称\",\n\
           \"reason\": \"详细说明为什么存在矛盾，指出冲突的具体设定原句\",\n\
           \"proposed_change\": \"若作者执意保留当前正文，建议如何修改原设定以消除矛盾\"\n\
         }}\n\
         约束：related_settings 只能引用设定条目里出现过的 id；不确定就留空数组，\n\
         不要编造 id。confidence 表示你自己对这一判定的确信程度。",
        context = input.context_text,
        text = trimmed
    );

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt,
    }];

    match gateway.call_sync(profile, api_key, None, messages).await {
        Ok(raw) => {
            let outcome = interpret_response(&raw);
            let outcome = match outcome {
                ConsistencyOutcome::Conflict { mut info } => {
                    resolve_related_settings(&mut info, input.candidates);
                    ConsistencyOutcome::Conflict { info }
                }
                other => other,
            };
            apply_gate(outcome, input.threshold, input.ignored)
        }
        Err(e) => ConsistencyOutcome::NotChecked {
            reason: format!("调用模型失败，未完成冲突检测：{e}"),
        },
    }
}

fn extract_json_block(text: &str) -> String {
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if start <= end {
                return text[start..=end].to_string();
            }
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_conflict() -> ConflictInfo {
        ConflictInfo {
            conflict_point: "沈砚已死".to_string(),
            setting_module: "角色".to_string(),
            setting_title: "沈砚".to_string(),
            reason: "第 3 章明确写其战死".to_string(),
            proposed_change: None,
            related_settings: Vec::new(),
            evidence: "第 3 章：沈砚战死".to_string(),
            confidence: Some(0.9),
            ignore_signature: String::new(),
            ignore_label: String::new(),
        }
    }

    fn candidates() -> Vec<ConflictSettingRef> {
        vec![
            ConflictSettingRef {
                entity_type: "character".to_string(),
                id: "ch_shen".to_string(),
                title: "沈砚".to_string(),
            },
            ConflictSettingRef {
                entity_type: "worldview".to_string(),
                id: "wv_lamp".to_string(),
                title: "灯塔法则".to_string(),
            },
        ]
    }

    #[test]
    fn clean_response_is_clean() {
        let raw = r#"{"has_conflict": false, "conflict_point": null}"#;
        assert!(matches!(interpret_response(raw), ConsistencyOutcome::Clean));
    }

    #[test]
    fn conflicting_response_carries_info() {
        let raw = r#"{"has_conflict": true, "conflict_point": "沈砚已死", "setting_module": "角色", "setting_title": "沈砚", "reason": "第 3 章明确写其战死", "proposed_change": null}"#;
        let outcome = interpret_response(raw);
        let info = outcome.as_conflict().expect("应判定为冲突");
        assert_eq!(info.conflict_point, "沈砚已死");
        assert_eq!(info.setting_title, "沈砚");
        assert_eq!(info.confidence, None, "模型没给置信度就不能编一个");
        assert!(info.evidence.is_empty());
    }

    #[test]
    fn non_json_response_is_not_checked_not_clean() {
        // 这正是旧实现会静默放行的输入：解析失败 → 以前返回 None → 被当成无冲突
        let outcome = interpret_response("我觉得应该没什么问题吧，直接写就行。");
        assert!(
            outcome.is_not_checked(),
            "非 JSON 返回必须判为未检测，绝不能当成无冲突"
        );
    }

    #[test]
    fn json_wrapped_in_prose_is_still_parsed() {
        let raw = "好的，判定结果如下：\n{\"has_conflict\": false}\n以上。";
        assert!(matches!(interpret_response(raw), ConsistencyOutcome::Clean));
    }

    #[test]
    fn missing_has_conflict_field_is_not_checked() {
        // 结构不对（缺字段）同样属于「没测成」，而不是「干净」
        assert!(interpret_response(r#"{"foo": 1}"#).is_not_checked());
    }

    #[test]
    fn structured_response_keeps_evidence_and_confidence() {
        let raw = r#"{
            "has_conflict": true,
            "conflict_point": "沈砚复活",
            "related_settings": [{"entity_type": "character", "id": "ch_shen", "title": "沈砚"}],
            "evidence": "第 3 章：沈砚战死",
            "confidence": 0.82,
            "setting_module": "角色",
            "setting_title": "沈砚",
            "reason": "设定写明已死",
            "proposed_change": "改为诈死"
        }"#;
        let outcome = interpret_response(raw);
        let info = outcome.as_conflict().expect("应判定为冲突");
        assert_eq!(info.confidence, Some(0.82));
        assert_eq!(info.evidence, "第 3 章：沈砚战死");
        assert_eq!(info.related_settings.len(), 1);
        assert_eq!(info.related_settings[0].id, "ch_shen");
        assert_eq!(info.proposed_change.as_deref(), Some("改为诈死"));
    }

    #[test]
    fn percentage_confidence_is_normalized() {
        let raw = r#"{"has_conflict": true, "confidence": 85}"#;
        assert_eq!(interpret_response(raw).as_conflict().unwrap().confidence, Some(0.85));
        let raw = r#"{"has_conflict": true, "confidence": 3}"#;
        assert_eq!(interpret_response(raw).as_conflict().unwrap().confidence, Some(0.03));
    }

    #[test]
    fn gate_suppresses_below_threshold_and_keeps_equal() {
        let outcome = apply_gate(
            ConsistencyOutcome::Conflict {
                info: sample_conflict(),
            },
            0.95,
            &HashSet::new(),
        );
        assert!(outcome.is_suppressed());

        let mut info = sample_conflict();
        info.confidence = Some(0.9);
        let outcome = apply_gate(ConsistencyOutcome::Conflict { info }, 0.9, &HashSet::new());
        assert!(matches!(outcome, ConsistencyOutcome::Conflict { .. }), "等于阈值不应被抑制");
    }

    #[test]
    fn missing_confidence_is_never_suppressed() {
        let mut info = sample_conflict();
        info.confidence = None;
        let outcome = apply_gate(
            ConsistencyOutcome::Conflict { info },
            0.99,
            &HashSet::new(),
        );
        assert!(matches!(outcome, ConsistencyOutcome::Conflict { .. }));
    }

    #[test]
    fn ignored_signature_returns_clean() {
        let mut info = sample_conflict();
        resolve_related_settings(&mut info, &candidates());
        assert_eq!(info.ignore_signature, "character:ch_shen");

        let ignored = HashSet::from([info.ignore_signature.clone()]);
        let outcome = apply_gate(ConsistencyOutcome::Conflict { info }, 0.0, &ignored);
        assert!(matches!(outcome, ConsistencyOutcome::Clean));
    }

    #[test]
    fn resolve_drops_bogus_ids_and_fills_title() {
        let mut info = sample_conflict();
        info.related_settings = vec![
            ConflictSettingRef {
                entity_type: "character".to_string(),
                id: "ghost".to_string(),
                title: "沈砚".to_string(),
            },
            ConflictSettingRef {
                entity_type: "character".to_string(),
                id: "ch_shen".to_string(),
                title: "旧标题".to_string(),
            },
        ];
        resolve_related_settings(&mut info, &candidates());
        assert_eq!(info.related_settings.len(), 1, "伪造 id 必须被丢弃");
        assert_eq!(info.related_settings[0].title, "沈砚", "标题以库内为准");
    }

    #[test]
    fn resolve_falls_back_to_title_when_model_omits_id() {
        let mut info = sample_conflict();
        info.related_settings = vec![ConflictSettingRef {
            entity_type: "角色".to_string(),
            id: String::new(),
            title: "沈砚（主角）".to_string(),
        }];
        resolve_related_settings(&mut info, &candidates());
        assert_eq!(info.related_settings.len(), 1);
        assert_eq!(info.related_settings[0].id, "ch_shen");
    }

    #[test]
    fn resolve_without_any_match_uses_module_signature() {
        let mut info = sample_conflict();
        info.setting_title = "不存在的条目".to_string();
        resolve_related_settings(&mut info, &candidates());
        assert!(info.related_settings.is_empty());
        assert_eq!(info.ignore_signature, "module:character");
        assert!(info.ignore_label.contains("角色"));
    }

    #[test]
    fn settings_defaults_and_parsing() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE app_setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        assert_eq!(
            ConflictCheckSettings::from_conn(&conn),
            ConflictCheckSettings::default()
        );

        crate::db::app_setting::set_setting(&conn, "conflict_check_enabled", "false").unwrap();
        crate::db::app_setting::set_setting(&conn, "conflict_check_scope", "pinned").unwrap();
        crate::db::app_setting::set_setting(&conn, "conflict_confidence_threshold", "0.75")
            .unwrap();
        let s = ConflictCheckSettings::from_conn(&conn);
        assert!(!s.enabled);
        assert_eq!(s.scope, CheckScope::PinnedOnly);
        assert!((s.threshold - 0.75).abs() < f32::EPSILON);

        // 非法值回落缺省，而不是把阈值弄成 0（那会关闭所有提示）
        crate::db::app_setting::set_setting(&conn, "conflict_confidence_threshold", "abc")
            .unwrap();
        assert_eq!(
            ConflictCheckSettings::from_conn(&conn).threshold,
            DEFAULT_CONFIDENCE_THRESHOLD
        );
    }
}