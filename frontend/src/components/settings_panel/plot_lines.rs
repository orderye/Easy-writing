use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::PlotItem;

#[derive(Serialize)]
struct ListPlotArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct SavePlotArgs {
    id: Option<String>,
    novel_id: String,
    title: String,
    content: String,
    kind: String,
    status: String,
    chapter_id: Option<String>,
}

#[component]
pub fn PlotLinesView(app_ctx: AppContext) -> impl IntoView {
    let plots = RwSignal::new(Vec::<PlotItem>::new());

    Effect::new(move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                if let Ok(list) =
                    invoke::<Vec<PlotItem>, _>("list_plots", &ListPlotArgs { novel_id }).await
                {
                    plots.set(list);
                }
            });
        }
    });

    let on_add_plot = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<PlotItem, _>(
                    "save_plot",
                    &SavePlotArgs {
                        id: None,
                        novel_id,
                        title: "新事件/伏笔".to_string(),
                        content: "".to_string(),
                        kind: "foreshadow".to_string(),
                        status: "planted".to_string(),
                        chapter_id: None,
                    },
                )
                .await;

                if let Ok(new_p) = res {
                    let mut list = plots.get();
                    list.push(new_p);
                    plots.set(list);
                }
            });
        }
    };

    view! {
        <div class="flex-1 overflow-y-auto p-3 space-y-3">
            <div class="flex items-center justify-between">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"剧情与伏笔"</span>
                <button
                    class="text-xs text-blue-600 hover:text-blue-800 font-medium px-2 py-0.5 rounded hover:bg-blue-50 transition-colors"
                    on:click=on_add_plot
                >
                    "+ 新增伏笔/事件"
                </button>
            </div>

            <For
                each=move || plots.get()
                key=|p| p.id.clone()
                children=move |plot| {
                    let p_id = plot.id.clone();
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
                                let _ = invoke::<PlotItem, _>(
                                    "save_plot",
                                    &SavePlotArgs {
                                        id: Some(pid),
                                        novel_id: nid,
                                        title: new_title,
                                        content: new_content,
                                        kind: k,
                                        status: new_status,
                                        chapter_id: None,
                                    },
                                )
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
                        <div class="border border-slate-200 rounded-lg p-2.5 bg-white shadow-2xs space-y-2">
                            <div class="flex items-center justify-between gap-2">
                                <input
                                    type="text"
                                    class="font-semibold text-xs text-slate-800 bg-transparent border-b border-transparent hover:border-slate-300 focus:border-blue-500 focus:outline-none flex-1 truncate"
                                    prop:value=move || title.get()
                                    on:input=move |ev| title.set(event_target_value(&ev))
                                />
                                <div class="flex items-center gap-1.5 shrink-0">
                                    <button
                                        class=move || {
                                            if status.get() == "planted" {
                                                "text-[10px] bg-amber-100 text-amber-800 px-1.5 py-0.5 rounded border border-amber-300 font-medium"
                                            } else {
                                                "text-[10px] bg-emerald-100 text-emerald-800 px-1.5 py-0.5 rounded border border-emerald-300 font-medium"
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
                                </div>
                            </div>
                            <textarea
                                class="w-full text-xs text-slate-600 bg-slate-50 border border-slate-200 rounded p-1.5 focus:bg-white focus:outline-blue-500 min-h-[50px]"
                                placeholder="输入戏剧冲突、伏笔埋设细节或回收节点……"
                                prop:value=move || content.get()
                                on:input=move |ev| content.set(event_target_value(&ev))
                            ></textarea>
                        </div>
                    }
                }
            />
        </div>
    }
}
