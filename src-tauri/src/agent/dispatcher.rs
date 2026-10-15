use rusqlite::Connection;
use shared_types::{Agent, LlmProfile};

pub fn resolve_agent_and_profile(
    conn: &Connection,
    agent_key: &str,
) -> Result<(Agent, LlmProfile, String), String> {
    let agent = crate::db::agent::get_agent_by_key(conn, agent_key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent '{}' not found in database", agent_key))?;

    let profile = if let Some(ref prof_id) = agent.llm_profile_id {
        crate::db::llm_profile::get_profile(conn, prof_id)
            .map_err(|e| e.to_string())?
    } else {
        None
    };

    let profile = match profile {
        Some(p) => p,
        None => {
            // Find default profile or first available
            let list = crate::db::llm_profile::list_profiles(conn).map_err(|e| e.to_string())?;
            list.into_iter()
                .find(|p| p.is_default)
                .or_else(|| {
                    crate::db::llm_profile::list_profiles(conn)
                        .ok()
                        .and_then(|l| l.into_iter().next())
                })
                .ok_or_else(|| "No LLM profile configured. Please add an API profile in Settings.".to_string())?
        }
    };

    let api_key = if !profile.key_ref.is_empty() {
        crate::keyring::get_secret(&profile.key_ref).unwrap_or_default()
    } else {
        String::new()
    };

    if api_key.trim().is_empty() {
        return Err("当前绑定的模型接口未检测到有效的 API Key 密钥，已在调用前拦截。请在系统设置中重新配置。".to_string());
    }

    Ok((agent, profile, api_key))
}
