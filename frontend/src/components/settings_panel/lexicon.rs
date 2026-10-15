use crate::components::settings_panel::loaders::load_list_for_novel;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::LexiconEntry;

#[component]
pub fn LexiconView(app_ctx: AppContext) -> impl IntoView {
    let terms = RwSignal::new(Vec::<LexiconEntry>::new());
    let filter_category = RwSignal::new("all".to_string());
    let search_query = RwSignal::new(String::new());
    let is_adding = RwSignal::new(false);
    let is_batch_modal_open = RwSignal::new(false);
    let batch_input_text = RwSignal::new(String::new());
    let batch_category = RwSignal::new("custom".to_string());
    let new_term_text = RwSignal::new(String::new());
    let new_term_category = RwSignal::new("worldview".to_string());
    let is_syncing = RwSignal::new(false);

    let load_terms = move || {
        load_list_for_novel(app_ctx, "list_lexicon", terms);
    };

    // Reload when novel or lexicon_version updates
    Effect::new(move |_| {
        let _ = app_ctx.lexicon_version.get();
        load_terms();
    });

    // One-click sync from novel settings and chapters
    let on_sync = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            is_syncing.set(true);
            leptos::task::spawn_local(async move {
                let _ = invoke_cmd("sync_lexicon_from_novel")
                    .arg("novel_id", novel_id)
                    .send_unit()
                    .await;
                is_syncing.set(false);
                app_ctx.lexicon_version.update(|v| *v += 1);
            });
        }
    };

    // Add single term
    let on_add_term = move |_| {
        let t = new_term_text.get().trim().to_string();
        if t.chars().count() < 2 {
            return;
        }
        let cat = new_term_category.get();
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let _ = invoke_cmd("save_lexicon_term")
                    .arg("novel_id", novel_id)
                    .arg("term", t)
                    .arg("category", cat)
                    .opt("source_id", None::<String>)
                    .send::<LexiconEntry>()
                    .await;
                new_term_text.set(String::new());
                is_adding.set(false);
                app_ctx.lexicon_version.update(|v| *v += 1);
            });
        }
    };

    // Delete single term
    let on_delete_term = move |id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("delete_lexicon_term")
                .arg("id", id)
                .send_unit()
                .await;
            app_ctx.lexicon_version.update(|v| *v += 1);
        });
    };

    // Batch import terms
    let on_submit_batch = move |_| {
        let raw = batch_input_text.get();
        let cat = batch_category.get();
        let list: Vec<String> = raw
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.chars().count() >= 2)
            .collect();

        if list.is_empty() {
            return;
        }

        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let _ = invoke_cmd("import_lexicon_terms")
                    .arg("novel_id", novel_id)
                    .arg("terms", list)
                    .arg("category", cat)
                    .send::<usize>()
                    .await;
                batch_input_text.set(String::new());
                is_batch_modal_open.set(false);
                app_ctx.lexicon_version.update(|v| *v += 1);
            });
        }
    };

    let filtered_terms = move || {
        let list = terms.get();
        let cat = filter_category.get();
        let query = search_query.get().trim().to_lowercase();

        list.into_iter()
            .filter(|item| {
                if cat != "all" && item.category != cat {
                    return false;
                }
                if !query.is_empty() && !item.term.to_lowercase().contains(&query) {
                    return false;
                }
                true
            })
            .collect::<Vec<_>>()
    };

    let category_badge = |cat: &str| match cat {
        "worldview" => (
            "功法设定",
            "bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300",
        ),
        "character" => (
            "角色人名",
            "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300",
        ),
        "corpus" => (
            "正文高频",
            "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300",
        ),
        "plot" => (
            "剧情伏笔",
            "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300",
        ),
        _ => (
            "自定义",
            "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
        ),
    };

    view! {
        <div class="flex-1 min-h-0 flex flex-col p-3 space-y-2.5 overflow-hidden">
            // Header stats & Sync action
            <div class="flex items-center justify-between pb-2 border-b border-slate-200 dark:border-slate-800 shrink-0">
                <div>
                    <h3 class="text-xs font-bold text-slate-800 dark:text-slate-100 flex items-center gap-1">
                        <span>"⚡ 小说专属词库"</span>
                        <span class="text-[10px] font-normal px-1.5 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-500">
                            {move || format!("共 {} 词", terms.get().len())}
                        </span>
                    </h3>
                    <p class="text-[11px] text-slate-400 dark:text-slate-500 mt-0.5">
                        "输入前缀时自动出现灰字预测，按 Tab 键一键补入正文。"
                    </p>
                </div>
                <button
                    class=move || {
                        if is_syncing.get() {
                            "text-xs px-2 py-1 rounded bg-slate-200 text-slate-500 cursor-not-allowed flex items-center gap-1"
                        } else {
                            "text-xs px-2 py-1 rounded bg-blue-50 dark:bg-blue-950/50 text-blue-600 dark:text-blue-400 hover:bg-blue-100 dark:hover:bg-blue-900/50 cursor-pointer font-medium flex items-center gap-1 transition-colors"
                        }
                    }
                    disabled=move || is_syncing.get()
                    title="从当前小说的角色、世界观功法和正文中重新提取"
                    on:click=on_sync
                >
                    <span>{move || if is_syncing.get() { "↻ 提取中..." } else { "⚡ 一键提取" }}</span>
                </button>
            </div>

            // Action toolbar: Search & Add buttons
            <div class="space-y-1.5 shrink-0">
                <div class="flex items-center gap-1.5">
                    <input
                        type="text"
                        placeholder="搜索词汇..."
                        class="flex-1 text-xs px-2.5 py-1 rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-200 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                        prop:value=move || search_query.get()
                        on:input=move |ev| search_query.set(event_target_value(&ev))
                    />
                    <button
                        class="text-xs px-2 py-1 rounded bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 cursor-pointer transition-colors"
                        title="手动新增词汇"
                        on:click=move |_| is_adding.update(|v| *v = !*v)
                    >
                        {move || if is_adding.get() { "取消" } else { "+ 加词" }}
                    </button>
                    <button
                        class="text-xs px-2 py-1 rounded bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 cursor-pointer transition-colors"
                        title="从多行文本批量导入"
                        on:click=move |_| is_batch_modal_open.set(true)
                    >
                        "批量"
                    </button>
                </div>

                // Category filter chips
                <div class="flex items-center gap-1 text-[11px] overflow-x-auto pb-0.5">
                    {
                        let categories = vec![
                            ("all", "全部"),
                            ("worldview", "功法设定"),
                            ("character", "角色"),
                            ("corpus", "正文高频"),
                            ("custom", "自定义"),
                        ];
                        categories.into_iter().map(|(val, label)| {
                            let is_active = move || filter_category.get() == val;
                            view! {
                                <button
                                    class=move || {
                                        if is_active() {
                                            "px-2 py-0.5 rounded-full bg-blue-600 text-white font-medium shrink-0 cursor-pointer"
                                        } else {
                                            "px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-400 hover:bg-slate-200 dark:hover:bg-slate-700 shrink-0 cursor-pointer transition-colors"
                                        }
                                    }
                                    on:click=move |_| filter_category.set(val.to_string())
                                >
                                    {label}
                                </button>
                            }
                        }).collect::<Vec<_>>()
                    }
                </div>
            </div>

            // Add new term form (conditional)
            {move || {
                if is_adding.get() {
                    view! {
                        <div class="p-2.5 rounded-lg border border-blue-200 dark:border-blue-900/60 bg-blue-50/50 dark:bg-blue-950/30 space-y-2 shrink-0">
                            <div class="text-[11px] font-medium text-slate-700 dark:text-slate-300">
                                "手动添加词条（如功法名、兵刃名、宗派等）"
                            </div>
                            <div class="flex items-center gap-2">
                                <input
                                    type="text"
                                    placeholder="例如：降龙十八掌"
                                    class="flex-1 text-xs px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                    prop:value=move || new_term_text.get()
                                    on:input=move |ev| new_term_text.set(event_target_value(&ev))
                                />
                                <select
                                    class="text-xs px-1.5 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-700 dark:text-slate-200"
                                    prop:value=move || new_term_category.get()
                                    on:change=move |ev| new_term_category.set(event_target_value(&ev))
                                >
                                    <option value="worldview">"功法/设定"</option>
                                    <option value="character">"角色/人物"</option>
                                    <option value="custom">"自定义词"</option>
                                </select>
                            </div>
                            <div class="flex justify-end gap-1.5">
                                <button
                                    class="text-xs px-2 py-0.5 rounded text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-800 cursor-pointer"
                                    on:click=move |_| is_adding.set(false)
                                >
                                    "取消"
                                </button>
                                <button
                                    class="text-xs px-3 py-0.5 rounded bg-blue-600 hover:bg-blue-700 text-white font-medium cursor-pointer"
                                    on:click=on_add_term
                                >
                                    "确认保存"
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div class="hidden"></div> }.into_any()
                }
            }}

            // Words list
            <div class="flex-1 min-h-0 overflow-y-auto space-y-1 pr-1">
                {move || {
                    let list = filtered_terms();
                    if list.is_empty() {
                        view! {
                            <div class="py-12 text-center text-xs text-slate-400 dark:text-slate-500 space-y-1">
                                <div>"暂无匹配词汇"</div>
                                <div class="text-[11px]">"点击上方「⚡ 一键提取」从小说设定和正文生成"</div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="space-y-1">
                                {list.into_iter().map(|item| {
                                    let id = item.id.clone();
                                    let term = item.term.clone();
                                    let term_title = term.clone();
                                    let cat = item.category.clone();
                                    let freq = item.frequency;
                                    let (badge_label, badge_style) = category_badge(&cat);

                                    view! {
                                        <div class="group flex items-center justify-between px-2.5 py-1.5 rounded-md hover:bg-slate-100 dark:hover:bg-slate-800/60 border border-transparent hover:border-slate-200 dark:hover:border-slate-700/60 transition-colors">
                                            <div class="flex items-center gap-2 min-w-0">
                                                <span class="text-xs font-medium text-slate-800 dark:text-slate-200 truncate" title=term_title>
                                                    {term}
                                                </span>
                                                <span class=format!("text-[10px] px-1.5 py-0.2 rounded font-normal shrink-0 {}", badge_style)>
                                                    {badge_label}
                                                </span>
                                            </div>

                                            <div class="flex items-center gap-2 shrink-0">
                                                <span class="text-[10px] text-slate-400 dark:text-slate-500" title="在正文中被采纳或出现的频次">
                                                    {format!("{}次", freq)}
                                                </span>
                                                <button
                                                    class="text-xs text-slate-300 dark:text-slate-600 hover:text-red-500 dark:hover:text-red-400 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer p-0.5"
                                                    title="从词库中移除此词"
                                                    on:click=move |_| on_delete_term(id.clone())
                                                >
                                                    "✕"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Batch import modal
            {move || {
                if is_batch_modal_open.get() {
                    view! {
                        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-xs p-4">
                            <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl shadow-xl max-w-md w-full p-4 space-y-3">
                                <div class="flex items-center justify-between border-b border-slate-100 dark:border-slate-800 pb-2">
                                    <h4 class="text-sm font-bold text-slate-800 dark:text-slate-100">
                                        "批量导入专有词库"
                                    </h4>
                                    <button
                                        class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer text-sm"
                                        on:click=move |_| is_batch_modal_open.set(false)
                                    >
                                        "✕"
                                    </button>
                                </div>

                                <p class="text-xs text-slate-500 dark:text-slate-400">
                                    "每行输入一个词条（支持粘贴专有名词表、武功秘籍、地名等，2~40字）："
                                </p>

                                <textarea
                                    class="w-full h-40 text-xs p-2.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 text-slate-800 dark:text-slate-200 font-mono focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                    placeholder="降龙十八掌&#10;独孤九剑&#10;乾坤大挪移&#10;光明顶&#10;倚天剑"
                                    prop:value=move || batch_input_text.get()
                                    on:input=move |ev| batch_input_text.set(event_target_value(&ev))
                                ></textarea>

                                <div class="flex items-center justify-between pt-1">
                                    <div class="flex items-center gap-1.5 text-xs text-slate-600 dark:text-slate-400">
                                        <span>"导入分类:"</span>
                                        <select
                                            class="text-xs px-2 py-0.5 rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-700 dark:text-slate-200"
                                            prop:value=move || batch_category.get()
                                            on:change=move |ev| batch_category.set(event_target_value(&ev))
                                        >
                                            <option value="worldview">"功法/设定"</option>
                                            <option value="character">"角色/人物"</option>
                                            <option value="custom">"自定义词"</option>
                                        </select>
                                    </div>
                                    <div class="flex gap-2">
                                        <button
                                            class="text-xs px-3 py-1 rounded text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer"
                                            on:click=move |_| is_batch_modal_open.set(false)
                                        >
                                            "取消"
                                        </button>
                                        <button
                                            class="text-xs px-3 py-1 rounded bg-blue-600 hover:bg-blue-700 text-white font-medium cursor-pointer"
                                            on:click=on_submit_batch
                                        >
                                            "导入词条"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div class="hidden"></div> }.into_any()
                }
            }}
        </div>
    }
}
