use crate::ai::gateway::AiGateway;
use crate::db::DbPool;
use shared_types::{Agent, GenerationParams, LlmProfile, RemoteModel};
use tauri::State;

/// 归一化生成参数：仅保留有意义的取值，避免把 None 字段写进 params JSON。
fn normalize_params(
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: Option<bool>,
) -> Result<String, String> {
    if let Some(t) = temperature {
        if !(0.0..=2.0).contains(&t) {
            return Err("temperature 需在 0.0 ~ 2.0 之间".to_string());
        }
    }
    if let Some(m) = max_tokens {
        if m == 0 || m > 200_000 {
            return Err("最大输出长度需在 1 ~ 200000 之间".to_string());
        }
    }

    let params = GenerationParams {
        temperature,
        max_tokens,
        stream,
    };
    Ok(if params.is_empty() {
        "{}".to_string()
    } else {
        params.to_json_string()
    })
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub fn create_llm_profile(
    pool: State<'_, DbPool>,
    name: String,
    protocol: String,
    base_url: String,
    model: String,
    api_key: String,
    is_default: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: Option<bool>,
) -> Result<LlmProfile, String> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err("API Key 密钥不能为空，请输入以 sk- 开头的有效密钥".to_string());
    }

    let params_json = normalize_params(temperature, max_tokens, stream)?;

    let conn = pool.get().map_err(|e| e.to_string())?;
    let key_ref = format!("llm-key-{}", uuid::Uuid::new_v4());

    crate::keyring::set_secret(&key_ref, trimmed_key)?;

    let display_name = if name.trim().is_empty() {
        model.clone()
    } else {
        name
    };

    crate::db::llm_profile::create_profile(
        &conn,
        &display_name,
        &protocol,
        &base_url,
        &key_ref,
        &model,
        &params_json,
        is_default,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub fn update_llm_profile(
    pool: State<'_, DbPool>,
    id: String,
    name: String,
    protocol: String,
    base_url: String,
    model: String,
    is_default: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: Option<bool>,
) -> Result<LlmProfile, String> {
    let params_json = normalize_params(temperature, max_tokens, stream)?;
    let conn = pool.get().map_err(|e| e.to_string())?;

    let existing = crate::db::llm_profile::get_profile(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到该接口配置".to_string())?;

    let display_name = if name.trim().is_empty() {
        model.clone()
    } else {
        name
    };

    crate::db::llm_profile::update_profile(
        &conn,
        &id,
        &display_name,
        &protocol,
        &base_url,
        &model,
        &params_json,
        is_default,
    )
    .map_err(|e| e.to_string())?;

    // key_ref 保持不变，仅当传入新密钥时替换钥匙串条目
    let _ = existing;
    crate::db::llm_profile::get_profile(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "更新后读取接口配置失败".to_string())
}

/// 「拉取模型列表」：用当前表单里的协议 / Base URL / API Key 拉取远端可用模型（PRD §5.1 P0）。
/// 不落库，仅返回候选列表供前端选择；API Key 未传时回退到已保存配置的钥匙串密钥。
#[tauri::command(rename_all = "snake_case")]
pub async fn fetch_remote_models(
    pool: State<'_, DbPool>,
    protocol: String,
    base_url: String,
    api_key: Option<String>,
    profile_id: Option<String>,
) -> Result<Vec<RemoteModel>, String> {
    let key = match api_key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(k) => k,
        None => {
            // 回退：使用 profile_id（或默认 profile）在钥匙串中的已存密钥
            let conn = pool.get().map_err(|e| e.to_string())?;
            let profile = match profile_id {
                Some(pid) => crate::db::llm_profile::get_profile(&conn, &pid)
                    .map_err(|e| e.to_string())?,
                None => {
                    let list =
                        crate::db::llm_profile::list_profiles(&conn).map_err(|e| e.to_string())?;
                    list.into_iter().find(|p| p.is_default).or_else(|| {
                        crate::db::llm_profile::list_profiles(&conn)
                            .ok()
                            .and_then(|l| l.into_iter().next())
                    })
                }
            }
            .ok_or_else(|| "尚无已保存的接口配置，请先填写 API Key 后再拉取模型列表".to_string())?;

            if profile.key_ref.is_empty() {
                return Err(
                    "当前接口未检测到有效的 API Key 密钥，请先输入密钥后再拉取模型列表".to_string(),
                );
            }
            crate::keyring::get_secret(&profile.key_ref).unwrap_or_default()
        }
    };

    if key.trim().is_empty() {
        return Err("当前接口未检测到有效的 API Key 密钥，请先输入密钥后再拉取模型列表".to_string());
    }

    if base_url.trim().is_empty() {
        return Err("请先填写 Base URL 接口地址".to_string());
    }

    let gateway = AiGateway::new();
    gateway
        .fetch_remote_models(&protocol, &base_url, &key)
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_llm_profiles(pool: State<'_, DbPool>) -> Result<Vec<LlmProfile>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::llm_profile::list_profiles(&conn).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_llm_profile(pool: State<'_, DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    if let Ok(Some(prof)) = crate::db::llm_profile::get_profile(&conn, &id) {
        if !prof.key_ref.is_empty() {
            let _ = crate::keyring::delete_secret(&prof.key_ref);
        }
    }
    crate::db::llm_profile::delete_profile(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn test_llm_connection(
    pool: State<'_, DbPool>,
    profile_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let profile = crate::db::llm_profile::get_profile(&conn, &profile_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Profile not found".to_string())?;

    let api_key = if !profile.key_ref.is_empty() {
        crate::keyring::get_secret(&profile.key_ref).unwrap_or_default()
    } else {
        String::new()
    };

    if api_key.trim().is_empty() {
        return Err("当前接口未检测到有效的 API Key 密钥，已在连接前拦截。请删除此配置后重新输入以 sk- 开头的有效密钥保存。".to_string());
    }

    let gateway = AiGateway::new();
    gateway.test_connection(&profile, &api_key).await
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_agents(pool: State<'_, DbPool>) -> Result<Vec<Agent>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::agent::list_agents(&conn).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_agent(
    pool: State<'_, DbPool>,
    key: String,
    system_prompt: String,
    llm_profile_id: Option<String>,
    enabled: bool,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::agent::update_agent(&conn, &key, &system_prompt, llm_profile_id, "{}", enabled)
        .map_err(|e| e.to_string())
}
