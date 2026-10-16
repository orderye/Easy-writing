use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use crate::db::DbPool;
use shared_types::{ChatMessage as ChatMessageDto, ChatSession, ConflictInfo, ExtractionCandidate};
use std::sync::Arc;
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
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

    let system_instruction = crate::db::prompt_template::get_active_prompt_template(&conn, &module_key);
    let gateway = AiGateway::new();

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: format!("需要修饰与润色的草稿如下：\n{}", current_content),
    }];

    gateway
        .call_sync(&profile, &api_key, Some(system_instruction.to_string()), messages)
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ai_chat_stream(
    app: tauri::AppHandle,
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
    prompt: String,
    event_name: String,
) -> Result<String, String> {
    // 1. 落库用户消息，并取回「近几轮对话」作为上下文（PRD §4.1）
    let (novel, agent, profile, api_key, session_id, history) = {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let novel = crate::db::novel::get_novel(&conn, &novel_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Novel not found".to_string())?;

        let (agent, profile, api_key) =
            crate::agent::dispatcher::resolve_agent_and_profile(&conn, "writing_assistant")?;

        // 对话记录随作品保存：会话按 作品 + 章节 归集
        let session = crate::db::chat::ensure_session(&conn, &novel_id, chapter_id.as_deref())
            .map_err(|e| e.to_string())?;

        crate::db::chat::append_message(&conn, &session.id, "user", &prompt)
            .map_err(|e| e.to_string())?;

        // 取最近若干轮对话（不含本轮提问在最后位置导致的错位，交由下方拼装处理）
        let history = crate::db::chat::list_messages(&conn, &session.id, 20)
            .map_err(|e| e.to_string())?;

        (novel, agent, profile, api_key, session.id, history)
    };

    let assembled = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::ai::context::assemble_writing_context(
            &conn,
            &novel,
            chapter_id.as_deref(),
            &agent.system_prompt,
        )
        .map_err(|e| e.to_string())?
    };

    // 2. 拼装对话历史为多轮消息（历史里最后一条即本轮提问，避免重复追加）
    let mut messages: Vec<ChatMessage> = history
        .iter()
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();

    if messages
        .last()
        .map(|m| m.role != "user" || m.content != prompt)
        .unwrap_or(true)
    {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.clone(),
        });
    }

    let gateway = AiGateway::new();
    let reply = gateway
        .call_stream(
            app,
            &event_name,
            &profile,
            &api_key,
            Some(assembled.system_prompt),
            messages,
        )
        .await?;

    // 3. 落库助手回复
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let _ = crate::db::chat::append_message(&conn, &session_id, "assistant", &reply);
    }

    Ok(reply)
}

// ---------- 对话记录（PRD §4.1 P1：随作品保存，可按章节筛选） ----------

#[tauri::command(rename_all = "snake_case")]
pub fn get_or_create_chat_session(
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
) -> Result<ChatSession, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chat::ensure_session(&conn, &novel_id, chapter_id.as_deref())
        .map_err(|e| e.to_string())
}

/// 【预留】列出某作品（可按章节筛选）的全部会话。
///
/// 数据层已就绪（会话带消息条数、按最近活跃排序），但前端尚未设计
/// 「一个章节下保留多轮独立对话并切换」的交互，故当前没有调用点。
/// 接入该 UI 时请同时把它从 `tests/ipc_command_inventory.rs` 的
/// `RESERVED_COMMANDS` 中移除（有测试盯着这一点）。
#[tauri::command(rename_all = "snake_case")]
pub fn list_chat_sessions(
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
) -> Result<Vec<ChatSession>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chat::list_sessions(&conn, &novel_id, chapter_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_chat_messages(
    pool: State<'_, DbPool>,
    session_id: String,
    limit: Option<i64>,
) -> Result<Vec<ChatMessageDto>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chat::list_messages(&conn, &session_id, limit.unwrap_or(200))
        .map_err(|e| e.to_string())
}

/// 【预留】追加一条对话消息。
///
/// 注意：前端**不应该**调用它来记录本轮问答 —— `ai_chat_stream` 在返回回复前
/// 已把 user/assistant 两条消息内部落库（见本文件上方），再调一次会写重复。
/// 它服务的是将来「导入/补录历史对话」这类场景。接入时请一并更新
/// `RESERVED_COMMANDS`。
#[tauri::command(rename_all = "snake_case")]
pub fn append_chat_message(
    pool: State<'_, DbPool>,
    session_id: String,
    role: String,
    content: String,
) -> Result<ChatMessageDto, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chat::append_message(&conn, &session_id, &role, &content)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_chat_session(
    pool: State<'_, DbPool>,
    session_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chat::clear_messages(&conn, &session_id).map_err(|e| e.to_string())
}

/// 【预留】删除一个会话及其全部消息。
///
/// 与 `list_chat_sessions` 同属「多会话管理」这一尚未设计的交互。
/// 接入时请一并更新 `RESERVED_COMMANDS`。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_chat_session(
    pool: State<'_, DbPool>,
    session_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chat::delete_session(&conn, &session_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub fn list_extraction_candidates(
    pool: State<'_, DbPool>,
    novel_id: String,
) -> Result<Vec<ExtractionCandidate>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::extraction::list_pending_candidates(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
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
