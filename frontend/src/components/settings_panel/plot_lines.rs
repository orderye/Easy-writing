use crate::components::ai::polish_button::PolishButton;
use crate::components::settings_panel::loaders::load_list_for_novel;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::PlotItem;

#[component]
pub fn PlotLinesView(app_ctx: AppContext) -> impl IntoView {
    let plots = RwSignal::new(Vec::<PlotItem>::new());

    let load_plots = move || {
        load_list_for_novel(app_ctx, "list_plots", plots);
    };

    Effect::new(move |_| {
        load_plots();
    });

    let on_add_plot = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            // 传当前打开的章节 id：人工新建伏笔才有埋设章节，看板才能算「距今章数」
            let chapter_id = app_ctx.current_chapter.get().map(|c| c.id);
            leptos::task::spawn_local(async move {
                let res = invoke_cmd("save_plot")
                    .opt("id", None::<String>)
                    .arg("novel_id", novel_id)
                    .arg("title", "新事件/伏笔".to_string())
                    .arg("content", "".to_string())
                    .arg("kind", "foreshadow".to_string())
                    .arg("status", "planted".to_string())
                    .opt("chapter_id", chapter_id)
                    .send::<PlotItem>()
                    .await;

                if let Ok(new_p) = res {
                    let mut list = plots.get();
                    list.push(new_p);
                    plots.set(list);
                }
            });
        }
    };

    let on_delete_plot = move |id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("delete_plot").arg("id", id).send_unit().await;
            load_plots();
        });
    };

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
            <div class="flex items-center justify-between text-xs pb-1 border-b border-slate-200 dark:border-slate-800">
                <span class="font-bold text-slate-700 dark:text-slate-300">"剧情与伏笔"</span>
                <button
                    class="text-xs text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-semibold px-2 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 transition-colors cursor-pointer"
                    on:click=on_add_plot
                >
                    "+ 新增伏笔/事件"
                </button>
            </div>

            {move || {
                let query = app_ctx.settings_search_query.get().to_lowercase().trim().to_string();
                let all_plots = plots.get();

                let filtered = if query.is_empty() {
                    all_plots
                } else {
                    all_plots.into_iter().filter(|p| {
                        p.title.to_lowercase().contains(&query)
                            || p.content.to_lowercase().contains(&query)
                    }).collect()
                };

                if filtered.is_empty() {
                    return view! {
                        <div class="py-8 text-center text-xs text-slate-400">
                            {if query.is_empty() { "暂无剧情伏笔，点击右上角「+ 新增伏笔/事件」" } else { "未找到匹配的剧情或伏笔" }}
                        </div>
                    }.into_any();
                }

                view! {
                    <div class="space-y-3">
                        {filtered.into_iter().map(|plot| {
                            let p_id = plot.id.clone();
                            let del_id = plot.id.clone();
                            let title = RwSignal::new(plot.title.clone());
                            let content = RwSignal::new(plot.content.clone());
                            let status = RwSignal::new(plot.status.clone());
                            let kind = plot.kind.clone();

                            let save_plot = move |new_title: String, new_content: String, new_status: String| {
                                if let Some(novel) = app_ctx.current_novel.get() {
                                    let nid = novel.id;
                                    let pid = p_id.clone();
                                    let k = kind.clone();
                                    leptos::task::spawn_local(async move {
                                        let _ = invoke_cmd("save_plot")
                                            .opt("id", Some(pid))
                                            .arg("novel_id", nid)
                                            .arg("title", new_title)
                                            .arg("content", new_content)
                                            .arg("kind", k)
                                            .arg("status", new_status)
                                            .opt("chapter_id", None::<String>)
                                            .send::<PlotItem>()
                                            .await;
                                    });
                                }
                            };

                            Effect::new(move |_| {
                                let t = title.get();
                                let c = content.get();
                                let s = status.get();
                                save_plot(t, c, s);
                            });

                            view! {
                                <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-2.5 bg-white dark:bg-slate-900 shadow-2xs space-y-2 hover:border-slate-300 dark:hover:border-slate-700 transition-colors">
                                    <div class="flex items-center justify-between gap-2">
                                        <div class="flex items-center gap-1.5 flex-1 min-w-0">
                                            <span class="text-sm">"📌"</span>
                                            <input
                                                type="text"
                                                class="font-semibold text-xs text-slate-800 dark:text-slate-100 bg-transparent border-b border-transparent hover:border-slate-300 dark:hover:border-slate-700 focus:border-blue-500 focus:outline-hidden flex-1 truncate px-1"
                                                prop:value=move || title.get()
                                                on:input=move |ev| title.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div class="flex items-center gap-1.5 shrink-0">
                                            <button
                                                class=move || {
                                                    if status.get() == "planted" {
                                                        "text-[10px] bg-amber-100 dark:bg-amber-950/60 text-amber-800 dark:text-amber-300 px-1.5 py-0.5 rounded border border-amber-300 dark:border-amber-700 font-medium cursor-pointer"
                                                    } else {
                                                        "text-[10px] bg-emerald-100 dark:bg-emerald-950/60 text-emerald-800 dark:text-emerald-300 px-1.5 py-0.5 rounded border border-emerald-300 dark:border-emerald-700 font-medium cursor-pointer"
                                                    }
                                                }
                                                on:click=move |_| {
                                                    status.update(|s| {
                                                        *s = if *s == "planted" { "resolved".to_string() } else { "planted".to_string() };
                                                    });
                                                }
                                            >
                                                {move || if status.get() == "planted" { "已埋设" } else { "已回收" }}
                                            </button>
                                            <PolishButton module_key="plot" content=content />
                                            <button
                                                class="text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-1 rounded hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors cursor-pointer"
                                                title="删除伏笔/事件（移入回收站）"
                                                on:click=move |_| on_delete_plot(del_id.clone())
                                            >
                                                "✕"
                                            </button>
                                        </div>
                                    </div>
                                    <textarea
                                        class="w-full text-xs text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-800 rounded p-1.5 focus:outline-hidden focus:ring-1 focus:ring-blue-500 min-h-[50px] leading-relaxed font-serif"
                                        placeholder="输入戏剧冲突、伏笔埋设细节或回收节点……"
                                        prop:value=move || content.get()
                                        on:input=move |ev| content.set(event_target_value(&ev))
                                    ></textarea>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}
