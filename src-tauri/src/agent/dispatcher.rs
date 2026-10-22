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

    // 密钥缺失必须返回机器可判定的 `KEY_MISSING` 错误：
    // 「这台机器上没有那条密钥」与「网络失败」是两回事，
    // 作者查错方向会浪费掉整个下午。
    let api_key = crate::keys::require_llm_key(crate::keyring::system(), &profile.key_ref)?;

    Ok((agent, profile, api_key))
}
