use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{ChatMessage as ChatMessageDto, ChatSession, ConsistencyOutcome};

/// 本地消息视图。seq 为客户端自增序号，用于 For key——同一角色重复内容也不会冲突。
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMsg {
    pub seq: u64,
    pub role: String,
    pub content: String,
}

const WELCOME: &str = "你好！我是你的专属写作助手。我的创作将严格遵循左栏的世界观、角色卡、剧情伏笔与大纲。请输入你的写作需求（如：续写本章情节、设计一段对话等）……";

#[component]
pub fn ChatBox(app_ctx: AppContext) -> impl IntoView {
    let is_open = app_ctx.ai_chat_open;
    let input_text = RwSignal::new(String::new());
    let is_generating = RwSignal::new(false);
    // 单调递增序号，保证 For key 全局唯一
    let next_seq = StoredValue::new(1u64);
    let messages = RwSignal::new(vec![ChatMsg {
        seq: 0,
        role: "assistant".to_string(),
        content: WELCOME.to_string(),
    }]);
    // 当前对话会话（随作品 + 章节保存，PRD §4.1 P1）
    let session = RwSignal::new(Option::<ChatSession>::None);
    let history_loaded = RwSignal::new(false);

    // 章节切换 / 作品切换时，重新载入该章节的对话记录
    Effect::new(move |_| {
        let novel = app_ctx.current_novel.get();
        let chapter_id = app_ctx.current_chapter.get().map(|c| c.id);
        let novel_id = match novel {
            Some(n) => n.id,
            None => {
                session.set(None);
                history_loaded.set(false);
                return;
            }
        };

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("get_or_create_chat_session")
                .arg("novel_id", novel_id.clone())
                .opt("chapter_id", chapter_id.clone())
                .send::<ChatSession>()
                .await;

            match res {
                Ok(sess) => {
                    let s_id = sess.id.clone();
                    session.set(Some(sess));

                    if let Ok(history) = invoke_cmd("list_chat_messages")
                        .arg("session_id", s_id)
                        .opt("limit", Some(200i64))
                        .send::<Vec<ChatMessageDto>>()
                        .await
                    {
                        let mut loaded: Vec<ChatMsg> = Vec::new();
                        for m in history {
                            let seq = next_seq.get_value();
                            next_seq.set_value(seq + 1);
                            loaded.push(ChatMsg {
                                seq,
                                role: m.role,
                                content: m.content,
                            });
                        }

                        if loaded.is_empty() {
                            messages.set(vec![ChatMsg {
                                seq: 0,
                                role: "assistant".to_string(),
                                content: WELCOME.to_string(),
                            }]);
                        } else {
                            messages.set(loaded);
                        }
                        history_loaded.set(true);
                    }
                }
                Err(_) => {
                    // 落库失败不阻塞对话：退化为内存会话
                    session.set(None);
                    history_loaded.set(true);
                }
            }
        });
    });

    let do_clear_history = move |_| {
        if let Some(sess) = session.get() {
            leptos::task::spawn_local(async move {
                let _ = invoke_cmd("clear_chat_session")
                    .arg("session_id", sess.id)
                    .send_unit()
                    .await;
            });
        }
        next_seq.set_value(1);
        messages.set(vec![ChatMsg {
            seq: 0,
            role: "assistant".to_string(),
            content: WELCOME.to_string(),
        }]);
    };

    let do_send = move || {
        let text = input_text.get().trim().to_string();
        if text.is_empty() || is_generating.get() {
            return;
        }

        let mut list = messages.get();
        let seq_user = next_seq.get_value();
        next_seq.set_value(seq_user + 1);
        list.push(ChatMsg {
            seq: seq_user,
            role: "user".to_string(),
            content: text.clone(),
        });
        input_text.set(String::new());

        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            let chapter_id = app_ctx.current_chapter.get().map(|c| c.id);

            // Placeholder for stream response
            let seq_reply = next_seq.get_value();
            next_seq.set_value(seq_reply + 1);
            list.push(ChatMsg {
                seq: seq_reply,
                role: "assistant".to_string(),
                content: "正在结合全书设定思考并创作...".to_string(),
            });
            messages.set(list);
            is_generating.set(true);

            leptos::task::spawn_local(async move {
                // ai_chat_stream 会在后端把用户消息与助手回复一并落库（PRD §4.1 P1）
                let res = invoke_cmd("ai_chat_stream")
                    .arg("novel_id", novel_id)
                    .opt("chapter_id", chapter_id)
                    .arg("prompt", text)
                    .arg("event_name", "chat-reply".to_string())
                    .send::<String>()
                    .await;

                let mut current_msgs = messages.get();
                if let Some(last) = current_msgs.last_mut() {
                    match res {
                        Ok(reply) => {
                            last.content = reply;
                        }
                        Err(err) => {
                            // 「缺少密钥」要引导去补密钥，而不是让作者去查网络。
                            last.content = match crate::keymsg::explain_key_missing(&err) {
                                Some(msg) => msg,
                                None => format!(
                                    "创作生成失败: {}\n请检查系统设置中的模型接口配置与 API Key。",
                                    err
                                ),
                            };
                        }
                    }
                }
                messages.set(current_msgs);
                is_generating.set(false);
            });
        }
    };

    let on_insert_to_editor = move |content: String| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            let text_to_insert = content.clone();
            leptos::task::spawn_local(async move {
                let conflict_check = invoke_cmd("ai_check_conflict")
                    .arg("novel_id", novel_id)
                    .arg("text", text_to_insert.clone())
                    .send::<ConsistencyOutcome>()
                    .await;

                let insert_into_editor = || {
                    tiptap::insert_at_cursor(&text_to_insert);
                    app_ctx.save_status.set("dirty".to_string());
                    app_ctx.word_count.set(tiptap::get_word_count());
                };

                match conflict_check {
                    Ok(ConsistencyOutcome::Conflict { info }) => {
                        // 检测到冲突：开冲突弹窗让用户决定改正文还是改设定
                        app_ctx.pending_conflict_text.set(Some(text_to_insert.clone()));
                        app_ctx.current_conflict.set(Some(info));
                        app_ctx.is_conflict_modal_open.set(true);
                    }
                    Ok(ConsistencyOutcome::Clean) => {
                        insert_into_editor();
                    }
                    Ok(ConsistencyOutcome::NotChecked { reason }) => {
                        // 「没测成」≠「没冲突」。正文照插（不阻塞写作），
                        // 但必须让用户看见这一次是没检查过，而不是检查通过。
                        // 旧实现把这种情况当成 None 静默插入，属于静默放行。
                        web_sys::console::warn_1(
                            &format!("[冲突检测未完成] {reason}").into(),
                        );
                        app_ctx
                            .save_alert
                            .set(Some(format!("本次未完成设定冲突检测：{reason}")));
                        insert_into_editor();
                    }
                    Err(e) => {
                        // 调用层失败同样属于「没测成」，不能当成干净
                        web_sys::console::warn_1(
                            &format!("[冲突检测调用失败] {e}").into(),
                        );
                        app_ctx
                            .save_alert
                            .set(Some("本次未完成设定冲突检测（调用失败）".to_string()));
                        insert_into_editor();
                    }
                }
            });
        }
    };

    view! {
        <div class="border-t border-slate-200 bg-white shadow-lg flex flex-col z-20 shrink-0">
            // Toggle bar
            <div
                class="h-8 bg-slate-100 hover:bg-slate-200/80 px-4 flex items-center justify-between cursor-pointer border-b border-slate-200 select-none text-xs text-slate-700"
                on:click=move |_| is_open.update(|v| *v = !*v)
            >
                <div class="flex items-center gap-2 font-semibold">
                    <span class="text-blue-600">"✨"</span>
                    <span>"AI 辅助写作助手"</span>
                    <span class="text-[10px] text-slate-400 font-normal">"（严格按设定写作 · 双重冲突检测）"</span>
                    {move || match session.get() {
                        Some(s) => {
                            let label = if s.chapter_id.is_some() { "本章对话已存档" } else { "全书对话已存档" };
                            view! {
                                <span class="text-[10px] text-emerald-600 font-normal">"· " {label}</span>
                            }.into_any()
                        }
                        None => view! { <span></span> }.into_any(),
                    }}
                </div>
                <span class="text-slate-500 font-mono text-xs">
                    {move || if is_open.get() { "▼ 收起" } else { "▲ 展开对话" }}
                </span>
            </div>

            // Expanded conversation area
            {move || {
                if is_open.get() {
                    view! {
                        <div class="h-64 flex flex-col bg-slate-50/50">
                            <div class="flex-1 overflow-y-auto p-4 space-y-3">
                                <For
                                    each=move || messages.get()
                                    key=|m| m.seq
                                    children=move |msg| {
                                        let is_user = msg.role == "user";
                                        let content_for_insert = msg.content.clone();
                                        view! {
                                            <div class=format!("flex flex-col {}", if is_user { "items-end" } else { "items-start" })>
                                                <div class=format!(
                                                    "max-w-xl text-xs rounded-xl p-3 shadow-xs {}",
                                                    if is_user {
                                                        "bg-blue-600 text-white rounded-br-none"
                                                    } else {
                                                        "bg-white border border-slate-200 text-slate-800 rounded-bl-none"
                                                    }
                                                )>
                                                    <div class="whitespace-pre-wrap">{msg.content.clone()}</div>
                                                    {if !is_user {
                                                        let c_ins = content_for_insert.clone();
                                                        view! {
                                                            <div class="mt-2 pt-2 border-t border-slate-100 flex items-center gap-2">
                                                                <button
                                                                    class="text-[10px] bg-blue-50 text-blue-600 hover:bg-blue-100 px-2 py-0.5 rounded font-medium transition-colors cursor-pointer"
                                                                    on:click=move |_| on_insert_to_editor(c_ins.clone())
                                                                >
                                                                    "↳ 校验冲突并插入正文"
                                                                </button>
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                </div>
                                            </div>
                                        }
                                    }
                                />
                            </div>

                            // Chat input box
                            <div class="p-3 bg-white border-t border-slate-200 flex items-center gap-2">
                                <input
                                    type="text"
                                    class="flex-1 text-xs border border-slate-300 rounded-md px-3 py-2 focus:outline-blue-500 bg-slate-50 focus:bg-white"
                                    placeholder="输入向写作助手的写作指令或前文续写要求……"
                                    prop:value=move || input_text.get()
                                    on:input=move |ev| input_text.set(event_target_value(&ev))
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            do_send();
                                        }
                                    }
                                />
                                <button
                                    class="px-2.5 py-2 bg-white border border-slate-300 hover:bg-slate-100 text-slate-600 text-xs font-medium rounded-md transition-colors shrink-0 cursor-pointer"
                                    title="清空当前章节的对话记录（已存档内容一并删除）"
                                    disabled=move || !history_loaded.get()
                                    on:click=do_clear_history
                                >
                                    "清空记录"
                                </button>
                                <button
                                    class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-xs font-semibold rounded-md shadow-xs transition-colors shrink-0 cursor-pointer disabled:opacity-50"
                                    disabled=move || is_generating.get()
                                    on:click=move |_| do_send()
                                >
                                    {move || if is_generating.get() { "生成中..." } else { "发送" }}
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </div>
    }
}
