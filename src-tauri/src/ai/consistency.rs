use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use serde::Deserialize;
use shared_types::{ConflictInfo, LlmProfile};

#[derive(Deserialize)]
struct ConflictCheckResult {
    has_conflict: bool,
    conflict_point: Option<String>,
    setting_module: Option<String>,
    setting_title: Option<String>,
    reason: Option<String>,
    proposed_change: Option<String>,
}

pub async fn check_consistency(
    gateway: &AiGateway,
    context_text: &str,
    profile: &LlmProfile,
    api_key: &str,
    generated_text: &str,
) -> Result<Option<ConflictInfo>, String> {
    // Quick length check - trivial responses don't need expensive conflict checking
    if generated_text.trim().len() < 20 {
        return Ok(None);
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
        context_text, generated_text
    );

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt,
    }];

    let response = gateway
        .call_sync(profile, api_key, None, messages)
        .await?;

    // Parse JSON
    let json_str = extract_json_block(&response);
    if let Ok(res) = serde_json::from_str::<ConflictCheckResult>(&json_str) {
        if res.has_conflict {
            return Ok(Some(ConflictInfo {
                conflict_point: res.conflict_point.unwrap_or_else(|| "正文与既有设定存在出入".to_string()),
                setting_module: res.setting_module.unwrap_or_else(|| "设定库".to_string()),
                setting_title: res.setting_title.unwrap_or_else(|| "未命名条目".to_string()),
                reason: res.reason.unwrap_or_else(|| "检测到正文逻辑与已知设定矛盾".to_string()),
                proposed_change: res.proposed_change,
            }));
        }
    }

    Ok(None)
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
