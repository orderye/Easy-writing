use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use serde::Deserialize;
use shared_types::{ConflictInfo, ConsistencyOutcome, LlmProfile};

#[derive(Deserialize)]
struct ConflictCheckResult {
    has_conflict: bool,
    conflict_point: Option<String>,
    setting_module: Option<String>,
    setting_title: Option<String>,
    reason: Option<String>,
    proposed_change: Option<String>,
}

/// 内容过短时连模型都不必调用。
const MIN_CHECK_LEN: usize = 20;

/// 把模型的原始返回解释成三态结果。
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
/// 返回三态而非 Option：调用方必须能区分「干净」与「没测成」。
pub async fn check_consistency(
    gateway: &AiGateway,
    context_text: &str,
    profile: &LlmProfile,
    api_key: &str,
    generated_text: &str,
) -> ConsistencyOutcome {
    let trimmed = generated_text.trim();
    if trimmed.chars().count() < MIN_CHECK_LEN {
        return ConsistencyOutcome::NotChecked {
            reason: format!("内容不足 {MIN_CHECK_LEN} 字，未做冲突检测"),
        };
    }

    let prompt = format!(
        "你是一个极其严格的小说逻辑与设定审核员。\n\
         请仔细对照下述小说设定，检查即将写入正文的内容是否存在任何前后矛盾、人设冲突、世界法则违背或伏笔崩坏。\n\n\
         【小说设定库】：\n{}\n\n\
         【待检查的正文内容】：\n{}\n\n\
         请务必以严格的 JSON 格式输出判定结果，不得附加任何多余说明：\n\
         {{\n\
           \"has_conflict\": true 或 false,\n\
           \"conflict_point\": \"矛盾核心点概述（若无冲突为 null）\",\n\
           \"setting_module\": \"涉及模块（世界观 / 角色 / 剧情 / 大纲）\",\n\
           \"setting_title\": \"涉及的具体条目名称\",\n\
           \"reason\": \"详细说明为什么存在矛盾，指出冲突的具体设定原句\",\n\
           \"proposed_change\": \"若用户执意要保留当前正文，建议如何修改原设定以消除矛盾（AI 拟好的设定修改建议）\"\n\
         }}",
        context_text, trimmed
    );

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt,
    }];

    match gateway.call_sync(profile, api_key, None, messages).await {
        Ok(raw) => interpret_response(&raw),
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
}
