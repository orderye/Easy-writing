use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::ConflictInfo;

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct AiChatArgs {
    novel_id: String,
    chapter_id: Option<String>,
    prompt: String,
    event_name: String,
}

#[derive(Serialize)]
struct CheckConflictArgs {
    novel_id: String,
    text: String,
}

#[component]
pub fn ChatBox(app_ctx: AppContext) -> impl IntoView {
    let is_open = app_ctx.ai_chat_open;
    let input_text = RwSignal::new(String::new());
    let is_generating = RwSignal::new(false);
    let messages = RwSignal::new(vec![ChatMsg {
        role: "assistant".to_string(),
        content: "你好！我是你的专属写作助手。我的创作将严格遵循左栏的世界观、角色卡、剧情伏笔与大纲。请输入你的写作需求（如：续写本章情节、设计一段对话等）……".to_string(),
    }]);

    let do_send = move || {
        let text = input_text.get().trim().to_string();
        if text.is_empty() || is_generating.get() {
            return;
        }

        let mut list = messages.get();
        list.push(ChatMsg {
            role: "user".to_string(),
            content: text.clone(),
        });
        input_text.set(String::new());

        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            let chapter_id = app_ctx.current_chapter.get().map(|c| c.id);

            // Placeholder for stream response
            list.push(ChatMsg {
                role: "assistant".to_string(),
                content: "正在结合全书设定思考并创作...".to_string(),
            });
            messages.set(list);
            is_generating.set(true);

            leptos::task::spawn_local(async move {
                let res = invoke::<String, _>(
                    "ai_chat_stream",
                    &AiChatArgs {
                        novel_id,
                        chapter_id,
                        prompt: text,
                        event_name: "chat-reply".to_string(),
                    },
                )
                .await;

                let mut current_msgs = messages.get();
                if let Some(last) = current_msgs.last_mut() {
                    match res {
                        Ok(reply) => {
                            last.content = reply;
                        }
                        Err(err) => {
                            last.content = format!("创作生成失败: {}\n请检查系统设置中的模型接口配置与 API Key。", err);
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
                let conflict_check = invoke::<Option<ConflictInfo>, _>(
                    "ai_check_conflict",
                    &CheckConflictArgs {
                        novel_id,
                        text: text_to_insert.clone(),
                    },
                )
                .await;

                match conflict_check {
                    Ok(Some(conflict)) => {
                        // Conflict detected! Open conflict modal for resolution
                        app_ctx.pending_conflict_text.set(Some(text_to_insert));
                        app_ctx.current_conflict.set(Some(conflict));
                        app_ctx.is_conflict_modal_open.set(true);
                    }
                    _ => {
                        // Clean, insert directly
                        tiptap::insert_at_cursor(&text_to_insert);
                        app_ctx.save_status.set("dirty".to_string());
                        app_ctx.word_count.set(tiptap::get_word_count());
                    }
                }
            });
        }
    };

    view! {
        <div class="border-t border-slate-200 bg-white shadow-lg flex flex-col z-20">
            // Toggle bar
            <div
                class="h-8 bg-slate-100 hover:bg-slate-200/80 px-4 flex items-center justify-between cursor-pointer border-b border-slate-200 select-none text-xs text-slate-700"
                on:click=move |_| is_open.update(|v| *v = !*v)
            >
                <div class="flex items-center gap-2 font-semibold">
                    <span class="text-blue-600">"✨"</span>
                    <span>"AI 辅助写作助手"</span>
                    <span class="text-[10px] text-slate-400 font-normal">"（严格按设定写作 · 双重冲突检测）"</span>
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
                                    key=|m| format!("{}:{}", m.role, m.content)
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
