use crate::ai::context::{
    assemble_writing_context_with_options, AssembleOptions, CharCoefficientEstimator,
    DEFAULT_CONTEXT_BUDGET_TOKENS,
};
use crate::ai::gateway::{generation_params_of, AiGateway};
use crate::ai::openai::ChatMessage;
use crate::db::DbPool;
use shared_types::{
    ChatMessage as ChatMessageDto, ChatSession, ContextManifest, ExtractionCandidate, LlmProfile,
};
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
    // 用户在「本次上下文」面板上取消勾选的条目 id（T06）：不进入本次装配。
    excluded_entry_ids: Option<Vec<String>>,
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
        // 预算（T06）：接口配置的 context_window（缺省 8k token）按 1.5 字符/token
        // 换算为字符预算；面板与真实发送共用同一解析链，保证清单与实际一致。
        let (budget_chars, budget_tokens) = context_budget_tokens_of(&profile);
        let estimator = CharCoefficientEstimator::default();
        let mut opts = AssembleOptions::new(budget_chars, budget_tokens, &estimator);
        opts.excluded_entry_ids = excluded_entry_ids.unwrap_or_default();
        assemble_writing_context_with_options(
            &conn,
            &novel,
            chapter_id.as_deref(),
            &agent.system_prompt,
            &prompt,
            &opts,
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

// ---------- 上下文装配透明度（T06） ----------

/// 从 LLM 接口配置解析上下文预算：`params.context_window`（token）
/// 缺省 8k；字符预算按 1.5 字符/token 换算。
/// `ai_chat_stream` 与 `get_context_manifest` 共用，保证面板与实际发送一致。
fn context_budget_tokens_of(profile: &LlmProfile) -> (usize, usize) {
    let params = generation_params_of(profile);
    let budget_tokens = params
        .context_window
        .map(|w| w as usize)
        .unwrap_or(DEFAULT_CONTEXT_BUDGET_TOKENS);
    let budget_chars = budget_tokens * 3 / 2;
    (budget_chars, budget_tokens)
}

/// 【T06】读取某作品（可按章节）本次会话的上下文装配清单。
///
/// 纯读库：不依赖 keyring / 网络，面板预览不因「未配置密钥」而失败；
/// 预算解析链与 `ai_chat_stream` 完全一致。
#[tauri::command(rename_all = "snake_case")]
pub fn get_context_manifest(
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
    prompt: Option<String>,
) -> Result<ContextManifest, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let novel = crate::db::novel::get_novel(&conn, &novel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Novel not found".to_string())?;

    // 写作助手 Agent 与其接口配置（不读密钥）
    let agent = crate::db::agent::get_agent_by_key(&conn, "writing_assistant")
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Agent 'writing_assistant' not found in database".to_string())?;
    let mut profile = match agent.llm_profile_id.as_ref() {
        Some(prof_id) => crate::db::llm_profile::get_profile(&conn, prof_id)
            .map_err(|e| e.to_string())?,
        None => None,
    };
    if profile.is_none() {
        // 与 dispatcher 同款回退：默认配置 → 首个可用配置
        let list =
            crate::db::llm_profile::list_profiles(&conn).map_err(|e| e.to_string())?;
        profile = list.into_iter().find(|p| p.is_default).or_else(|| {
            crate::db::llm_profile::list_profiles(&conn)
                .ok()
                .and_then(|l| l.into_iter().next())
        });
    }
    let profile = profile
        .ok_or_else(|| "No LLM profile configured. Please add an API profile in Settings.".to_string())?;

    let (budget_chars, budget_tokens) = context_budget_tokens_of(&profile);
    let estimator = CharCoefficientEstimator::default();
    let opts = AssembleOptions::new(budget_chars, budget_tokens, &estimator);
    let prompt = prompt.unwrap_or_default();
    let assembled = assemble_writing_context_with_options(
        &conn,
        &novel,
        chapter_id.as_deref(),
        &agent.system_prompt,
        &prompt,
        &opts,
    )
    .map_err(|e| e.to_string())?;

    Ok(assembled.manifest)
}

/// 【T06】切换条目「恒入上下文」标记。
///
/// pinned 条目在装配时始终带入且排在所属类别最前；
/// `entity_type` 取值与上下文清单一致：worldview / character / plot。
#[tauri::command(rename_all = "snake_case")]
pub fn set_entry_pinned(
    pool: State<'_, DbPool>,
    entity_type: String,
    entry_id: String,
    pinned: bool,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let table = match entity_type.as_str() {
        "worldview" => "worldview_entry",
        "character" => "character",
        "plot" => "plot_item",
        other => return Err(format!("不支持的实体类型：{}", other)),
    };
    let flag = if pinned { 1 } else { 0 };
    conn.execute(
        &format!(
            "UPDATE {} SET pinned = ?1, updated_at = datetime('now') WHERE id = ?2",
            table
        ),
        rusqlite::params![flag, entry_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
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
) -> Result<shared_types::ConsistencyOutcome, String> {
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
    Ok(crate::ai::consistency::check_consistency(
        &gateway,
        &context_text,
        &profile,
        &api_key,
        &text,
    )
    .await)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_chapter_extract(
    pool: State<'_, DbPool>,
    novel_id: String,
    chapter_id: String,
) -> Result<crate::agent::outcome::ExtractionOutcome, String> {
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
        .prepare("SELECT novel_id, chapter_id, kind, action, target_id, COALESCE(title, ''), payload, evidence FROM extraction_candidate WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let candidate_row: Result<(String, String, String, String, Option<String>, String, String, String), rusqlite::Error> =
        stmt.query_row(rusqlite::params![candidate_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        });

    if let Ok((novel_id, chapter_id, kind, _cand_action, target_id, cand_title, default_payload, evidence)) = candidate_row {
        let final_text = custom_payload.unwrap_or(default_payload);
        let mut title = cand_title.trim().to_string();
        if title.is_empty() {
            title = final_text
                .lines()
                .next()
                .unwrap_or("未命名条目")
                .chars()
                .take(30)
                .collect();
        }

        match kind.as_str() {
            "worldview" => {
                // 1. 设定：保存到世界观设定库（自修复 target_id 存在性）
                let real_target_id = target_id.filter(|tid| {
                    conn.query_row(
                        "SELECT 1 FROM worldview_entry WHERE id = ?1 AND deleted_at IS NULL",
                        rusqlite::params![tid],
                        |_| Ok(()),
                    )
                    .is_ok()
                });

                let _ = crate::db::settings::save_worldview(
                    &conn,
                    real_target_id,
                    &novel_id,
                    "世界观设定",
                    &title,
                    &final_text,
                );

                // 2. 词库自联动：将该专有名词自动注册到小说词库中，供编辑器自动补全
                let _ = crate::db::lexicon::upsert_lexicon_term(
                    &conn,
                    &novel_id,
                    &title,
                    "worldview",
                    None,
                );
            }
            "character" => {
                // 1. 角色与角色卡：智能合并 6 维全息解构
                let real_target_id = target_id.filter(|tid| {
                    conn.query_row(
                        "SELECT 1 FROM character WHERE id = ?1 AND deleted_at IS NULL",
                        rusqlite::params![tid],
                        |_| Ok(()),
                    )
                    .is_ok()
                });

                let char_id = if let Some(ref tid) = real_target_id {
                    // 更新既有角色：无损合并新提炼信息至 ExplodedProfile
                    let existing_char = crate::db::settings::list_characters(&conn, &novel_id)
                        .unwrap_or_default()
                        .into_iter()
                        .find(|c| &c.id == tid);

                    let mut ep = if let Some(ref c) = existing_char {
                        shared_types::ExplodedProfile::parse_from_str(&c.profile)
                    } else {
                        shared_types::ExplodedProfile::default()
                    };

                    if ep.raw_notes.is_empty() {
                        ep.raw_notes = final_text.clone();
                    } else if !ep.raw_notes.contains(&final_text) {
                        ep.raw_notes.push_str("\n\n---\n");
                        ep.raw_notes.push_str(&final_text);
                    }

                    let profile_json = ep.to_json_string();
                    let ch_name = existing_char.as_ref().map(|c| c.name.as_str()).unwrap_or(&title);
                    let ch_aliases = existing_char.as_ref().map(|c| c.aliases.as_str()).unwrap_or("[]");
                    let _ = crate::db::settings::save_character(
                        &conn,
                        Some(tid.clone()),
                        &novel_id,
                        ch_name,
                        ch_aliases,
                        &profile_json,
                    );
                    tid.clone()
                } else {
                    // 新建角色：初始化 6 维角色卡
                    let mut ep = shared_types::ExplodedProfile::default();
                    ep.raw_notes = final_text.clone();
                    ep.faction = "默认阵营".to_string();
                    ep.importance = 2;
                    let profile_json = ep.to_json_string();

                    if let Ok(new_ch) = crate::db::settings::save_character(
                        &conn,
                        None,
                        &novel_id,
                        &title,
                        "[]",
                        &profile_json,
                    ) {
                        new_ch.id
                    } else {
                        uuid::Uuid::new_v4().to_string()
                    }
                };

                // 2. 词库自联动：角色名加入词库
                let _ = crate::db::lexicon::upsert_lexicon_term(
                    &conn,
                    &novel_id,
                    &title,
                    "character",
                    Some(&char_id),
                );

                // 3. 关系管理与星云爆炸图自联动：检测与小说中其他已有角色的交互关系并建立连线
                if let Ok(all_chars) = crate::db::settings::list_characters(&conn, &novel_id) {
                    for other in all_chars {
                        if other.id != char_id && (final_text.contains(&other.name) || evidence.contains(&other.name)) {
                            // 自修复：推断关系标签并入库 character_relation
                            let rel_name = if final_text.contains("君臣") || final_text.contains("官家") || final_text.contains("陛下") {
                                "君臣"
                            } else if final_text.contains("同袍") || final_text.contains("同门") || final_text.contains("部将") {
                                "同袍/盟友"
                            } else if final_text.contains("敌") || final_text.contains("战") || final_text.contains("诛") {
                                "对立/宿敌"
                            } else if final_text.contains("妻") || final_text.contains("夫") || final_text.contains("眷") {
                                "眷属/羁绊"
                            } else {
                                "相识交互"
                            };

                            let desc = format!("本章（第 {} 章证据）出现交互：{}", chapter_id, evidence.chars().take(60).collect::<String>());
                            let _ = crate::db::character_relation::save_character_relation(
                                &conn,
                                None,
                                &novel_id,
                                &char_id,
                                &other.id,
                                rel_name,
                                &desc,
                            );
                        }
                    }
                }
            }
            "plot" => {
                // 剧情与伏笔：判定 mainline / conflict / foreshadow 以及 planted / resolved
                let plot_kind = if final_text.contains("主线") {
                    "mainline"
                } else if final_text.contains("冲突") || final_text.contains("危机") {
                    "conflict"
                } else {
                    "foreshadow"
                };

                let plot_status = if final_text.contains("回收") || final_text.contains("已解决") || final_text.contains("resolved") {
                    "resolved"
                } else {
                    "planted"
                };

                let real_target_id = target_id.filter(|tid| {
                    conn.query_row(
                        "SELECT 1 FROM plot_item WHERE id = ?1 AND deleted_at IS NULL",
                        rusqlite::params![tid],
                        |_| Ok(()),
                    )
                    .is_ok()
                });

                let _ = crate::db::settings::save_plot(
                    &conn,
                    real_target_id,
                    &novel_id,
                    &title,
                    &final_text,
                    plot_kind,
                    plot_status,
                    Some(chapter_id.clone()),
                );
            }
            "chapter_summary" => {
                // 大纲：更新章节梗概 summary 字段，直接呈现在大纲视图
                let _ = conn.execute(
                    "UPDATE chapter SET summary = ?1, updated_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![final_text, chapter_id],
                );
                let _ = crate::db::settings::record_setting_change(
                    &conn,
                    &novel_id,
                    Some(chapter_id.as_str()),
                    "chapter",
                    &chapter_id,
                    &title,
                    "",
                    &final_text,
                    "AI提炼章纲梗概",
                );
            }
            _ => {}
        }
        let _ = crate::db::extraction::update_candidate_status(&conn, &candidate_id, "accepted");
    }

    Ok(())
}
