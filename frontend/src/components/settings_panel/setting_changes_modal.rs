use crate::components::settings_panel::loaders::load_for_novel;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{AffectedChapter, SettingChange};

#[component]
pub fn SettingChangesModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let changes = RwSignal::new(Vec::<SettingChange>::new());
    let is_loading = RwSignal::new(false);
    let reverting_id = RwSignal::new(Option::<String>::None);
    let filter_module = RwSignal::new("all".to_string());

    // 影响面扫描（P2-4）：改了一条设定之后，正文里哪些地方提到过它？
    // 这一步刻意做成确定性字面匹配（不调模型），结果可复现；
    // 真正的"要不要同步修改"交给作者判断。
    let impact_term = RwSignal::new(String::new());
    let impact_results = RwSignal::new(Vec::<AffectedChapter>::new());
    let is_scanning = RwSignal::new(false);

    let run_impact_scan = move || {
        let term = impact_term.get_untracked().trim().to_string();
        if term.is_empty() {
            impact_results.set(Vec::new());
            return;
        }
        let Some(novel) = app_ctx.current_novel.get_untracked() else {
            return;
        };
        is_scanning.set(true);
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_cmd("find_affected_chapters")
                .arg("novel_id", novel.id)
                .arg("term", term)
                .send::<Vec<AffectedChapter>>()
                .await
            {
                impact_results.set(list);
            }
            is_scanning.set(false);
        });
    };

    let load_changes = move || {
        is_loading.set(true);
        load_for_novel(app_ctx, "list_setting_changes", changes, move || {
            is_loading.set(false);
        });
    };

    Effect::new(move |_| {
        if is_open.get() {
            load_changes();
        }
    });

    let handle_revert = move |change_id: String| {
        reverting_id.set(Some(change_id.clone()));
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("revert_setting_change")
                .arg("change_id", change_id)
                .send_unit()
                .await;
            reverting_id.set(None);
            load_changes();
        });
    };

    let get_filtered_list = move || {
        let list = changes.get();
        let f = filter_module.get();
        if f == "all" {
            list
        } else {
            list.into_iter().filter(|c| c.target_type == f).collect()
        }
    };

    view! {
        {move || {
            if !is_open.get() {
                return view! { <div class="hidden"></div> }.into_any();
            }

            view! {
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-6 select-none animate-in fade-in duration-200">
                    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl shadow-2xl w-full max-w-4xl h-[85vh] flex flex-col overflow-hidden text-slate-800 dark:text-slate-100">
                        // Header
                        <div class="px-6 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50 dark:bg-slate-800/50">
                            <div class="flex items-center gap-3">
                                <span class="text-xl">"📜"</span>
                                <div>
                                    <h2 class="text-base font-bold text-slate-800 dark:text-slate-100">
                                        "设定改动留档与撤销"
                                    </h2>
                                    <p class="text-xs text-slate-500 dark:text-slate-400">
                                        "完整追踪世界观、角色与剧情设定的手动编辑与AI冲突调整，支持一键撤销回退。"
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

                        // Filter Toolbar
                        <div class="px-6 py-2.5 bg-slate-100/70 dark:bg-slate-800/30 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
                            <div class="flex items-center gap-2">
                                <span class="text-xs font-semibold text-slate-500 dark:text-slate-400 mr-1">"模块筛选:"</span>
                                {["all", "worldview", "character", "plot"].into_iter().map(|mod_key| {
                                    let is_active = move || filter_module.get() == mod_key;
                                    let label = match mod_key {
                                        "worldview" => "世界观",
                                        "character" => "角色",
                                        "plot" => "剧情",
                                        _ => "全部",
                                    };
                                    view! {
                                        <button
                                            class=move || {
                                                format!(
                                                    "px-2.5 py-1 text-xs rounded-md transition-colors cursor-pointer {}",
                                                    if is_active() {
                                                        "bg-white dark:bg-slate-800 font-bold text-blue-600 dark:text-blue-400 shadow-xs border border-slate-300 dark:border-slate-700"
                                                    } else {
                                                        "text-slate-600 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-slate-800/60"
                                                    }
                                                )
                                            }
                                            on:click=move |_| filter_module.set(mod_key.to_string())
                                        >
                                            {label}
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>

                            <span class="text-xs text-slate-400">
                                {move || format!("共 {} 条改动记录", get_filtered_list().len())}
                            </span>
                        </div>

                        // 影响面扫描条：设定改了，正文里还有哪些地方在用它？
                        <div class="px-6 py-2.5 bg-amber-50/60 dark:bg-amber-950/20 border-b border-amber-200/70 dark:border-amber-900/40">
                            <div class="flex items-center gap-2">
                                <span class="text-xs font-semibold text-amber-700 dark:text-amber-300 shrink-0">
                                    "🔍 影响面扫描"
                                </span>
                                <input
                                    class="flex-1 px-2.5 py-1 text-xs rounded-md border border-amber-300 dark:border-amber-800 bg-white dark:bg-slate-900 text-slate-700 dark:text-slate-200 focus:outline-none focus:ring-1 focus:ring-amber-400"
                                    placeholder="输入设定名 / 角色名 / 专有名词，扫描哪些章节正文提到过它…"
                                    prop:value=move || impact_term.get()
                                    on:input=move |ev| impact_term.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" {
                                            run_impact_scan();
                                        }
                                    }
                                />
                                <button
                                    class="px-3 py-1 text-xs rounded-md bg-amber-500 hover:bg-amber-600 text-white font-medium transition-colors cursor-pointer shrink-0"
                                    on:click=move |_| run_impact_scan()
                                >
                                    {move || if is_scanning.get() { "扫描中…" } else { "扫描" }}
                                </button>
                            </div>
                            {move || {
                                let list = impact_results.get();
                                if list.is_empty() {
                                    return view! { <span></span> }.into_any();
                                }
                                view! {
                                    <div class="mt-2 max-h-40 overflow-y-auto space-y-1">
                                        <p class="text-[11px] text-amber-700 dark:text-amber-400">
                                            {format!(
                                                "命中 {} 个章节（按命中次数排序）—— 这些正文可能需要同步修改：",
                                                list.len(),
                                            )}
                                        </p>
                                        {list
                                            .into_iter()
                                            .map(|c| {
                                                view! {
                                                    <div class="text-[11px] text-slate-600 dark:text-slate-300 flex items-start gap-2">
                                                        <span class="font-semibold shrink-0">{c.title}</span>
                                                        <span class="px-1.5 rounded bg-amber-200/70 dark:bg-amber-900/50 text-amber-800 dark:text-amber-200 shrink-0">
                                                            {format!("{} 处", c.hits)}
                                                        </span>
                                                        <span class="truncate text-slate-500 dark:text-slate-400">
                                                            {c.snippet}
                                                        </span>
                                                    </div>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </div>
                                }
                                    .into_any()
                            }}
                        </div>

                        // Main Content: List of changes
                        <div class="flex-1 overflow-y-auto p-6 space-y-4">
                            {move || {
                                if is_loading.get() {
                                    return view! {
                                        <div class="p-12 text-center text-xs text-slate-400">
                                            <span class="inline-block animate-spin mr-1">"↻"</span>
                                            "加载改动记录..."
                                        </div>
                                    }.into_any();
                                }

                                let filtered_list = get_filtered_list();

                                if filtered_list.is_empty() {
                                    return view! {
                                        <div class="p-16 text-center text-xs text-slate-400">
                                            <p class="text-sm font-semibold text-slate-600 dark:text-slate-400">"暂无设定改动记录"</p>
                                            <p class="mt-1 text-slate-400">"当设定库发生编辑修改或AI冲突调整时，改动前后的快照会自动记录于此。"</p>
                                        </div>
                                    }.into_any();
                                }

                                view! {
                                    <div class="space-y-4">
                                        {filtered_list.into_iter().map(|item| {
                                            let change_id = item.id.clone();
                                            let is_rev = item.is_reverted;
                                            let is_this_reverting = move || reverting_id.get().as_deref() == Some(&change_id);

                                            let (mod_name, mod_class) = match item.target_type.as_str() {
                                                "character" => ("角色", "bg-emerald-100 dark:bg-emerald-950/60 text-emerald-700 dark:text-emerald-300 border-emerald-300 dark:border-emerald-800"),
                                                "plot" => ("剧情", "bg-purple-100 dark:bg-purple-950/60 text-purple-700 dark:text-purple-300 border-purple-300 dark:border-purple-800"),
                                                _ => ("世界观", "bg-blue-100 dark:bg-blue-950/60 text-blue-700 dark:text-blue-300 border-blue-300 dark:border-blue-800"),
                                            };

                                            let cid_for_btn = item.id.clone();
                                            let item_title = if item.target_title.is_empty() { "设定项".to_string() } else { item.target_title };
                                            let item_reason = item.reason;
                                            let item_created_at = item.created_at;
                                            let item_before = if item.before_content.is_empty() { "（新建条目，无历史内容）".to_string() } else { item.before_content };
                                            let item_after = if item.after_content.is_empty() { "（内容已清空）".to_string() } else { item.after_content };

                                            view! {
                                                <div class="p-4 rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-850 shadow-xs">
                                                    // Top Row: Module, Title, Reason, Time, Action
                                                    <div class="flex items-center justify-between gap-3 pb-3 border-b border-slate-200/60 dark:border-slate-800">
                                                        <div class="flex items-center gap-2">
                                                            <span class=format!("text-xs px-2 py-0.5 rounded border font-semibold {}", mod_class)>
                                                                {mod_name}
                                                            </span>
                                                            <strong class="text-sm font-bold text-slate-900 dark:text-slate-100">
                                                                {item_title}
                                                            </strong>
                                                            <span class="text-xs px-2 py-0.5 rounded bg-slate-200/70 dark:bg-slate-700/50 text-slate-600 dark:text-slate-300">
                                                                {item_reason}
                                                            </span>
                                                        </div>

                                                        <div class="flex items-center gap-3">
                                                            <span class="text-xs text-slate-400 font-mono">
                                                                {item_created_at}
                                                            </span>

                                                            {if is_rev {
                                                                view! {
                                                                    <span class="text-xs px-2.5 py-1 rounded-md bg-slate-200 dark:bg-slate-800 text-slate-500 font-medium">
                                                                        "✓ 已撤销此改动"
                                                                    </span>
                                                                }.into_any()
                                                            } else {
                                                                view! {
                                                                    <button
                                                                        class="px-3 py-1 rounded-lg text-xs font-semibold bg-rose-50 hover:bg-rose-100 dark:bg-rose-950/40 dark:hover:bg-rose-900/60 text-rose-700 dark:text-rose-300 border border-rose-300 dark:border-rose-700/60 shadow-xs flex items-center gap-1 transition-colors cursor-pointer disabled:opacity-50"
                                                                        disabled=is_this_reverting
                                                                        on:click=move |_| handle_revert(cid_for_btn.clone())
                                                                    >
                                                                        <span>"⎌"</span>
                                                                        {if is_this_reverting() { "正在撤销..." } else { "撤销此改动" }}
                                                                    </button>
                                                                }.into_any()
                                                            }}
                                                        </div>
                                                    </div>

                                                    // Diff Body (Before vs After)
                                                    <div class="grid grid-cols-2 gap-4 mt-3 text-xs">
                                                        <div class="p-3 rounded-lg bg-rose-50/40 dark:bg-rose-950/20 border border-rose-200 dark:border-rose-900/40">
                                                            <div class="font-bold text-rose-700 dark:text-rose-400 mb-1 flex items-center gap-1">
                                                                <span>"⊖"</span>
                                                                "改动前状态"
                                                            </div>
                                                            <p class="whitespace-pre-wrap leading-relaxed text-slate-600 dark:text-slate-300 max-h-36 overflow-y-auto select-text font-serif">
                                                                {item_before}
                                                            </p>
                                                        </div>

                                                        <div class="p-3 rounded-lg bg-emerald-50/40 dark:bg-emerald-950/20 border border-emerald-200 dark:border-emerald-900/40">
                                                            <div class="font-bold text-emerald-700 dark:text-emerald-400 mb-1 flex items-center gap-1">
                                                                <span>"⊕"</span>
                                                                "改动后状态"
                                                            </div>
                                                            <p class="whitespace-pre-wrap leading-relaxed text-slate-700 dark:text-slate-200 max-h-36 overflow-y-auto select-text font-serif">
                                                                {item_after}
                                                            </p>
                                                        </div>
                                                    </div>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
