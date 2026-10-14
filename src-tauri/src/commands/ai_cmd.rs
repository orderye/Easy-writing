use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use crate::db::DbPool;
use shared_types::{ConflictInfo, ExtractionCandidate};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ai_polish(
    pool: State<'_, DbPool>,
    module_key: String,
    current_content: String,
) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    // Map module_key to corresponding agent
    let agent_key = match module_key.as_str() {
        "worldview" => "worldview",
        "character" => "character",
        "plot" => "plot",
        "outline" => "outline",
        "volume_summary" | "chapter_summary" => "volume_chapter",
        _ => "writing_assistant",
    };

    let (_agent, profile, api_key) =
        crate::agent::dispatcher::resolve_agent_and_profile(&conn, agent_key)?;

    let system_instruction = crate::agent::polish::get_polish_prompt(&module_key);
    let gateway = AiGateway::new();

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: format!("需要修饰与润色的草稿如下：\n{}", current_content),
    }];

    gateway
        .call_sync(&profile, &api_key, Some(system_instruction.to_string()), messages)
        .await
}

#[tauri::command]
pub async fn ai_chat_stream(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
    prompt: String,
    event_name: String,
) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let novel = crate::db::novel::get_novel(&conn, &novel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Novel not found".to_string())?;

    let (agent, profile, api_key) =
        crate::agent::dispatcher::resolve_agent_and_profile(&conn, "writing_assistant")?;

    let assembled = crate::ai::context::assemble_writing_context(
        &conn,
        &novel,
        chapter_id.as_deref(),
        &agent.system_prompt,
    )
    .map_err(|e| e.to_string())?;

    let gateway = AiGateway::new();
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt,
    }];

    gateway
        .call_stream(
            app,
            &event_name,
            &profile,
            &api_key,
            Some(assembled.system_prompt),
            messages,
        )
        .await
}

#[tauri::command]
pub async fn ai_check_conflict(
    pool: State<'_, DbPool>,
    novel_id: String,
    text: String,
) -> Result<Option<ConflictInfo>, String> {
    let (profile, api_key, context_text) = {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let novel = crate::db::novel::get_novel(&conn, &novel_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Novel not found".to_string())?;

        let (_agent, profile, api_key) =
            crate::agent::dispatcher::resolve_agent_and_profile(&conn, "writing_assistant")?;

        let assembled = crate::ai::context::assemble_writing_context(&conn, &novel, None, "")
            .map_err(|e| e.to_string())?;

        (profile, api_key, assembled.context_text)
    };

    let gateway = AiGateway::new();
    crate::ai::consistency::check_consistency(&gateway, &context_text, &profile, &api_key, &text).await
}

#[tauri::command]
pub async fn trigger_chapter_extract(
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: String,
) -> Result<Vec<ExtractionCandidate>, String> {
    let (novel, chapter, profile, api_key) = {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let novel = crate::db::novel::get_novel(&conn, &novel_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Novel not found".to_string())?;

        let chapter = crate::db::chapter::get_chapter(&conn, &chapter_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Chapter not found".to_string())?;

        let (_agent, profile, api_key) =
            crate::agent::dispatcher::resolve_agent_and_profile(&conn, "writing_assistant")?;

        (novel, chapter, profile, api_key)
    };

    let gateway_arc = Arc::new(AiGateway::new());
    let pool_clone = pool.inner().clone();

    crate::agent::extraction::run_parallel_extraction(
        gateway_arc,
        pool_clone,
        novel,
        chapter,
        profile,
        api_key,
    )
    .await
}

#[tauri::command]
pub fn list_extraction_candidates(
    pool: State<'_, DbPool>,
    novel_id: String,
) -> Result<Vec<ExtractionCandidate>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::extraction::list_pending_candidates(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolve_candidate(
    pool: State<'_, DbPool>,
    candidate_id: String,
    action: String, // "accept" | "ignore"
    custom_payload: Option<String>,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    if action == "ignore" {
        crate::db::extraction::update_candidate_status(&conn, &candidate_id, "ignored")
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Accept: fetch candidate details and insert into formal table
    let mut stmt = conn
        .prepare("SELECT novel_id, kind, payload FROM extraction_candidate WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let candidate_row: Result<(String, String, String), rusqlite::Error> =
        stmt.query_row(rusqlite::params![candidate_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)));

    if let Ok((novel_id, kind, default_payload)) = candidate_row {
        let final_text = custom_payload.unwrap_or(default_payload);
        match kind.as_str() {
            "worldview" => {
                let _ = crate::db::settings::save_worldview(
                    &conn,
                    None,
                    &novel_id,
                    "AI提炼",
                    "提炼条目",
                    &final_text,
                );
            }
            "character" => {
                let _ = crate::db::settings::save_character(
                    &conn,
                    None,
                    &novel_id,
                    "提炼新角色",
                    "[]",
                    &final_text,
                );
            }
            "plot" => {
                let _ = crate::db::settings::save_plot(
                    &conn,
                    None,
                    &novel_id,
                    "提炼事件",
                    &final_text,
                    "foreshadow",
                    "planted",
                    None,
                );
            }
            _ => {}
        }
        let _ = crate::db::extraction::update_candidate_status(&conn, &candidate_id, "accepted");
    }

    Ok(())
}
