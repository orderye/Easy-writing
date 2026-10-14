use crate::ai::gateway::AiGateway;
use crate::db::DbPool;
use shared_types::{Agent, LlmProfile};
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub fn create_llm_profile(
    pool: State<'_, DbPool>,
    name: String,
    protocol: String,
    base_url: String,
    model: String,
    api_key: String,
    is_default: bool,
) -> Result<LlmProfile, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let key_ref = format!("llm-key-{}", uuid::Uuid::new_v4());

    if !api_key.is_empty() {
        crate::keyring::set_secret(&key_ref, &api_key)?;
    }

    crate::db::llm_profile::create_profile(
        &conn,
        &name,
        &protocol,
        &base_url,
        &key_ref,
        &model,
        "{}",
        is_default,
    )
    .map_err(|e| e.to_string())
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
