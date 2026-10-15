use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{Chapter, ChapterRevision, ChapterRevisionHeader};

#[component]
pub fn RevisionModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let revisions = RwSignal::new(Vec::<ChapterRevisionHeader>::new());
    let selected_id = RwSignal::new(Option::<String>::None);
    let selected_revision = RwSignal::new(Option::<ChapterRevision>::None);
    let is_loading = RwSignal::new(false);
    let is_reverting = RwSignal::new(false);
    let snapshot_note = RwSignal::new(String::new());

    // Load revisions whenever modal opens or chapter changes
    let load_revisions = move || {
        if let Some(ch) = app_ctx.current_chapter.get() {
            let cid = ch.id.clone();
            is_loading.set(true);
            leptos::task::spawn_local(async move {
                if let Ok(list) = invoke_cmd("list_chapter_revisions")
                    .arg("chapter_id", cid)
                    .send::<Vec<ChapterRevisionHeader>>()
                    .await
                {
                    if let Some(first) = list.first() {
                        let fid = first.id.clone();
                        selected_id.set(Some(fid.clone()));
                        // Fetch full details of first revision
                        if let Ok(Some(detail)) = invoke_cmd("get_chapter_revision")
                            .arg("id", fid)
                            .send::<Option<ChapterRevision>>()
                            .await
                        {
                            selected_revision.set(Some(detail));
                        }
                    } else {
                        selected_id.set(None);
                        selected_revision.set(None);
                    }
                    revisions.set(list);
                }
                is_loading.set(false);
            });
        }
    };

    Effect::new(move |_| {
        if is_open.get() {
            load_revisions();
        }
    });

    let select_revision = move |id: String| {
        selected_id.set(Some(id.clone()));
        leptos::task::spawn_local(async move {
            if let Ok(Some(detail)) = invoke_cmd("get_chapter_revision")
                .arg("id", id)
                .send::<Option<ChapterRevision>>()
                .await
            {
                selected_revision.set(Some(detail));
            }
        });
    };

    let do_create_snapshot = move || {
        if let Some(ch) = app_ctx.current_chapter.get() {
            let cid = ch.id.clone();
            let note = snapshot_note.get().trim().to_string();
            leptos::task::spawn_local(async move {
                let note_val = if note.is_empty() {
                    "手动快照".to_string()
                } else {
                    note
                };

                if let Ok(new_rev) = invoke_cmd("create_manual_chapter_revision")
                    .arg("chapter_id", cid)
                    .arg("note", note_val)
                    .send::<ChapterRevision>()
                    .await
                {
                    snapshot_note.set(String::new());
                    selected_id.set(Some(new_rev.id.clone()));
                    selected_revision.set(Some(new_rev));
                    load_revisions();
                }
            });
        }
    };

    let handle_revert = move |rev_id: String| {
        if let Some(ch) = app_ctx.current_chapter.get() {
            let cid = ch.id.clone();
            is_reverting.set(true);
            leptos::task::spawn_local(async move {
                if let Ok(reverted_ch) = invoke_cmd("revert_chapter_to_revision")
                    .arg("chapter_id", cid)
                    .arg("revision_id", rev_id)
                    .send::<Chapter>()
                    .await
                {
                    tiptap::set_content(&reverted_ch.content);
                    app_ctx.current_chapter.set(Some(reverted_ch));
                    app_ctx.word_count.set(tiptap::get_word_count());
                    app_ctx.save_status.set("saved".to_string());
                    is_open.set(false);
                }
                is_reverting.set(false);
            });
        }
    };

    view! {
        {move || {
            if !is_open.get() {
                return view! { <div class="hidden"></div> }.into_any();
            }

            let chapter_title = app_ctx
                .current_chapter
                .get()
                .map(|c| c.title)
                .unwrap_or_else(|| "当前章节".to_string());

            let current_words = app_ctx.word_count.get();

            view! {
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-6 select-none animate-in fade-in duration-200">
                    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl shadow-2xl w-full max-w-5xl h-[85vh] flex flex-col overflow-hidden text-slate-800 dark:text-slate-100">
                        // Header
                        <div class="px-6 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50 dark:bg-slate-800/50">
                            <div class="flex items-center gap-3">
                                <span class="text-xl">"🕒"</span>
                                <div>
                                    <h2 class="text-base font-bold text-slate-800 dark:text-slate-100 flex items-center gap-2">
                                        "版本历史与回滚"
                                        <span class="text-xs px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300 font-normal">
                                            {chapter_title}
                                        </span>
                                    </h2>
                                    <p class="text-xs text-slate-500 dark:text-slate-400">
                                        "自动保存与手动打点均完整留档，回滚前会自动对当前状态进行二次备份。"
                                    </p>
                                </div>
                            </div>
                            <button
                                class="p-1 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-lg cursor-pointer"
                                title="关闭"
                                on:click=move |_| is_open.set(false)
                            >
                                "✕"
                            </button>
                        </div>

                        // Action Bar: Create Snapshot
                        <div class="px-6 py-3 bg-slate-100/70 dark:bg-slate-800/30 border-b border-slate-200 dark:border-slate-800 flex items-center gap-3">
                            <input
                                type="text"
                                class="flex-1 px-3 py-1.5 text-xs rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-200 placeholder-slate-400 focus:outline-hidden focus:ring-2 focus:ring-blue-500"
                                placeholder="输入快照备注（例如：第1稿完成、删减第3节打斗细节、大纲调整前）..."
                                prop:value=move || snapshot_note.get()
                                on:input=move |ev| snapshot_note.set(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        do_create_snapshot();
                                    }
                                }
                            />
                            <button
                                class="px-4 py-1.5 text-xs font-semibold rounded-lg bg-blue-600 hover:bg-blue-700 text-white shadow-xs flex items-center gap-1.5 transition-colors shrink-0 cursor-pointer"
                                on:click=move |_| do_create_snapshot()
                            >
                                <span>"📸"</span>
                                "记录当前快照"
                            </button>
                        </div>

                        // Main Content (Split Timeline & Preview)
                        <div class="flex-1 flex overflow-hidden">
                            // Left Column: Timeline List
                            <div class="w-80 border-r border-slate-200 dark:border-slate-800 flex flex-col bg-slate-50/50 dark:bg-slate-900/50 overflow-y-auto">
                                <div class="p-3 text-xs font-semibold text-slate-500 dark:text-slate-400 flex items-center justify-between border-b border-slate-200/60 dark:border-slate-800">
                                    <span>"历史版本记录"</span>
                                    <span>{move || format!("共 {} 个版本", revisions.get().len())}</span>
                                </div>

                                {move || {
                                    if is_loading.get() {
                                        return view! {
                                            <div class="p-8 text-center text-xs text-slate-400">
                                                <span class="inline-block animate-spin mr-1">"↻"</span>
                                                "加载历史版本..."
                                            </div>
                                        }.into_any();
                                    }

                                    let list = revisions.get();
                                    if list.is_empty() {
                                        return view! {
                                            <div class="p-8 text-center text-xs text-slate-400">
                                                <p>"暂无历史快照"</p>
                                                <p class="mt-1 text-slate-500">"正文改动保存或手动记录后将自动归档"</p>
                                            </div>
                                        }.into_any();
                                    }

                                    view! {
                                        <div class="divide-y divide-slate-200/60 dark:divide-slate-800/60">
                                            {list.into_iter().map(|item| {
                                                let id_clone = item.id.clone();
                                                let is_selected = move || selected_id.get().as_deref() == Some(&id_clone);

                                                let (type_text, type_class) = match item.revision_type.as_str() {
                                                    "manual" => ("手动快照", "bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300 border-blue-200 dark:border-blue-800"),
                                                    "rollback_before" => ("回滚备份", "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300 border-amber-200 dark:border-amber-800"),
                                                    "ai_before" => ("AI改动前", "bg-purple-100 text-purple-700 dark:bg-purple-950 dark:text-purple-300 border-purple-200 dark:border-purple-800"),
                                                    "ai_after" => ("AI改动后", "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300 border-emerald-200 dark:border-emerald-800"),
                                                    _ => ("自动快照", "bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-400 border-slate-200 dark:border-slate-700"),
                                                };

                                                let target_id = item.id.clone();
                                                let word_diff = item.word_count - (current_words as i32);
                                                let diff_str = if word_diff > 0 {
                                                    format!("+{} 字", word_diff)
                                                } else if word_diff < 0 {
                                                    format!("{} 字", word_diff)
                                                } else {
                                                    "字数持平".to_string()
                                                };

                                                view! {
                                                    <div
                                                        class=move || {
                                                            format!(
                                                                "p-3.5 cursor-pointer transition-all hover:bg-slate-100 dark:hover:bg-slate-800/80 {}",
                                                                if is_selected() {
                                                                    "bg-blue-50/80 dark:bg-blue-900/20 border-l-4 border-blue-500 pl-2.5"
                                                                } else {
                                                                    "border-l-4 border-transparent"
                                                                }
                                                            )
                                                        }
                                                        on:click=move |_| select_revision(target_id.clone())
                                                    >
                                                        <div class="flex items-center justify-between gap-1 mb-1">
                                                            <span class=format!("text-[10px] px-1.5 py-0.5 rounded border font-medium {}", type_class)>
                                                                {type_text}
                                                            </span>
                                                            <span class="text-[11px] text-slate-400 dark:text-slate-500 font-mono">
                                                                {item.created_at}
                                                            </span>
                                                        </div>

                                                        <div class="flex items-center justify-between text-xs mt-1.5">
                                                            <span class="font-medium text-slate-700 dark:text-slate-200">
                                                                {format!("{} 字", item.word_count)}
                                                            </span>
                                                            <span class=format!(
                                                                "text-[11px] {}",
                                                                if word_diff > 0 { "text-emerald-600 dark:text-emerald-400" }
                                                                else if word_diff < 0 { "text-rose-600 dark:text-rose-400" }
                                                                else { "text-slate-400 dark:text-slate-500" }
                                                            )>
                                                                {diff_str}
                                                            </span>
                                                        </div>

                                                        {if !item.note.is_empty() {
                                                            view! {
                                                                <p class="text-xs text-slate-500 dark:text-slate-400 mt-1 line-clamp-1 italic">
                                                                    "“"{item.note}"”"
                                                                </p>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span></span> }.into_any()
                                                        }}
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }}
                            </div>

                            // Right Column: Snapshot Detail & Rollback Action
                            <div class="flex-1 flex flex-col bg-white dark:bg-slate-900 overflow-hidden">
                                {move || {
                                    if let Some(detail) = selected_revision.get() {
                                        let target_rev_id = detail.id.clone();
                                        let paragraphs = detail.content_plain.split('\n')
                                            .map(|s| s.trim())
                                            .filter(|s| !s.is_empty())
                                            .map(|s| s.to_string())
                                            .collect::<Vec<_>>();

                                        view! {
                                            <div class="flex-1 flex flex-col overflow-hidden">
                                                // Preview Header
                                                <div class="px-6 py-3 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50/50 dark:bg-slate-850">
                                                    <div>
                                                        <div class="flex items-center gap-2">
                                                            <span class="text-xs font-semibold text-slate-700 dark:text-slate-200">
                                                                "快照时间: " {detail.created_at}
                                                            </span>
                                                            <span class="text-xs text-slate-400">
                                                                "| " {format!("{} 字", detail.word_count)}
                                                            </span>
                                                        </div>
                                                        {if !detail.note.is_empty() {
                                                            view! {
                                                                <p class="text-xs text-blue-600 dark:text-blue-400 font-medium mt-0.5">
                                                                    "备注: " {detail.note}
                                                                </p>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span></span> }.into_any()
                                                        }}
                                                    </div>

                                                    <button
                                                        class="px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-bold shadow-sm flex items-center gap-1.5 transition-colors cursor-pointer disabled:opacity-50"
                                                        disabled=move || is_reverting.get()
                                                        on:click=move |_| handle_revert(target_rev_id.clone())
                                                    >
                                                        <span>"⎌"</span>
                                                        {if is_reverting.get() { "正在回滚..." } else { "一键回滚至此版本" }}
                                                    </button>
                                                </div>

                                                // Preview Body
                                                <div class="flex-1 p-6 overflow-y-auto font-serif text-sm leading-relaxed text-slate-700 dark:text-slate-200 select-text">
                                                    {if paragraphs.is_empty() {
                                                        view! {
                                                            <div class="text-slate-400 text-center py-12 text-xs">
                                                                "此版本正文内容为空"
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="space-y-4 max-w-3xl mx-auto">
                                                                {paragraphs.into_iter().map(|p| {
                                                                    view! {
                                                                        <p class="indent-8 text-justify">{p}</p>
                                                                    }
                                                                }).collect::<Vec<_>>()}
                                                            </div>
                                                        }.into_any()
                                                    }}
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="flex-1 flex items-center justify-center text-slate-400 text-xs">
                                                "请在左侧选择要查看的历史版本快照"
                                            </div>
                                        }.into_any()
                                    }
                                }}
                            </div>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
