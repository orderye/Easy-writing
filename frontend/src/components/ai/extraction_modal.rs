use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::ExtractionCandidate;

#[derive(Serialize)]
struct ResolveCandidateArgs {
    candidate_id: String,
    action: String,
    custom_payload: Option<String>,
}

#[derive(Serialize)]
struct TriggerExtractArgs {
    novel_id: String,
    chapter_id: String,
}

#[component]
pub fn ExtractionModal(
    app_ctx: AppContext,
    is_open: RwSignal<bool>,
    candidates: RwSignal<Vec<ExtractionCandidate>>,
    is_extracting: RwSignal<bool>,
) -> impl IntoView {
    let on_resolve = move |candidate_id: String, action: &'static str| {
        let cid = candidate_id.clone();
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>(
                "resolve_candidate",
                &ResolveCandidateArgs {
                    candidate_id: cid.clone(),
                    action: action.to_string(),
                    custom_payload: None,
                },
            )
            .await;

            let mut list = candidates.get();
            list.retain(|c| c.id != cid);
            candidates.set(list);
        });
    };

    let on_accept_all = move |_| {
        let current_list = candidates.get();
        leptos::task::spawn_local(async move {
            for c in current_list {
                let _ = invoke::<(), _>(
                    "resolve_candidate",
                    &ResolveCandidateArgs {
                        candidate_id: c.id,
                        action: "accept".to_string(),
                        custom_payload: None,
                    },
                )
                .await;
            }
            candidates.set(Vec::new());
        });
    };

    let on_reextract = move |_| {
        if let (Some(novel), Some(chapter)) = (app_ctx.current_novel.get(), app_ctx.current_chapter.get()) {
            let nid = novel.id;
            let cid = chapter.id;
            is_extracting.set(true);
            leptos::task::spawn_local(async move {
                if let Ok(cands) = invoke::<Vec<ExtractionCandidate>, _>(
                    "trigger_chapter_extract",
                    &TriggerExtractArgs {
                        novel_id: nid,
                        chapter_id: cid,
                    },
                )
                .await
                {
                    candidates.set(cands);
                }
                is_extracting.set(false);
            });
        }
    };

    view! {
        {move || {
            if is_open.get() {
                let extracting = is_extracting.get();

                view! {
                    <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-50 p-6">
                        <div class="bg-white dark:bg-slate-900 rounded-xl shadow-2xl max-w-3xl w-full border border-slate-200 dark:border-slate-800 overflow-hidden flex flex-col max-h-[85vh] animate-in fade-in zoom-in-95">
                            // Header
                            <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-amber-50/50 dark:bg-amber-950/20">
                                <div class="flex items-center gap-2">
                                    <span class="text-amber-500 font-bold">"✨"</span>
                                    <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100">"AI 自动提炼设定审核看板"</h3>
                                    <span class="text-xs text-amber-700 dark:text-amber-300 bg-amber-100 dark:bg-amber-950/80 px-2 py-0.5 rounded-full font-medium border border-amber-200 dark:border-amber-800">
                                        "待确认候选: " {move || candidates.get().len()} " 条"
                                    </span>
                                </div>
                                <div class="flex items-center gap-2">
                                    <button
                                        class="px-2 py-1 text-xs text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-950/40 rounded transition-colors cursor-pointer font-medium disabled:opacity-50"
                                        disabled=extracting
                                        on:click=on_reextract
                                        title="重新并发调用 4 大 Agent 分析当前章节"
                                    >
                                        "🔄 重新提炼"
                                    </button>
                                    <button
                                        class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 font-mono text-lg cursor-pointer px-1"
                                        on:click=move |_| is_open.set(false)
                                    >
                                        "✕"
                                    </button>
                                </div>
                            </div>

                            // Body
                            <div class="p-5 flex-1 overflow-y-auto space-y-3">
                                {move || if is_extracting.get() {
                                    view! {
                                        <div class="py-12 flex flex-col items-center justify-center text-slate-500 dark:text-slate-400 space-y-3">
                                            <div class="animate-spin text-3xl">"✨"</div>
                                            <p class="text-xs font-medium">"世界观、角色、剧情、大纲 4 大 Agent 正在受限并行提炼本章设定……"</p>
                                        </div>
                                    }.into_any()
                                } else if candidates.get().is_empty() {
                                    view! {
                                        <div class="py-12 text-center text-slate-400 text-xs">
                                            "当前章节暂无可提炼的全新设定，或所有候选已完成确认。"
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <For
                                            each=move || candidates.get()
                                            key=|c| c.id.clone()
                                            children=move |cand| {
                                                let c_id = cand.id.clone();
                                                let c_id_for_accept = c_id.clone();
                                                let c_id_for_ignore = c_id.clone();
                                                let kind_label = match cand.kind.as_str() {
                                                    "worldview" => "世界观",
                                                    "character" => "角色",
                                                    "plot" => "剧情伏笔",
                                                    _ => "章纲摘要",
                                                };

                                                view! {
                                                    <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-3 bg-white dark:bg-slate-850 shadow-2xs space-y-2">
                                                        <div class="flex items-center justify-between">
                                                            <div class="flex items-center gap-2">
                                                                <span class="text-xs font-bold text-blue-700 dark:text-blue-300 bg-blue-50 dark:bg-blue-950/60 px-2 py-0.5 rounded border border-blue-200 dark:border-blue-800">
                                                                    {kind_label}
                                                                </span>
                                                                <span class="text-xs font-semibold text-slate-800 dark:text-slate-200">
                                                                    {cand.action}
                                                                </span>
                                                            </div>
                                                            <div class="flex items-center gap-2">
                                                                <button
                                                                    class="px-2.5 py-1 text-xs text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-750 rounded transition-colors cursor-pointer"
                                                                    on:click=move |_| on_resolve(c_id_for_ignore.clone(), "ignore")
                                                                >
                                                                    "忽略"
                                                                </button>
                                                                <button
                                                                    class="px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded font-medium shadow-xs transition-colors cursor-pointer"
                                                                    on:click=move |_| on_resolve(c_id_for_accept.clone(), "accept")
                                                                >
                                                                    "✓ 采纳"
                                                                </button>
                                                            </div>
                                                        </div>

                                                        <p class="text-xs text-slate-700 dark:text-slate-200 whitespace-pre-wrap leading-relaxed bg-slate-50 dark:bg-slate-900 p-2.5 rounded border border-slate-100 dark:border-slate-800">
                                                            {cand.payload.clone()}
                                                        </p>

                                                        {if !cand.evidence.is_empty() {
                                                            let ev = cand.evidence.clone();
                                                            view! {
                                                                <div class="text-[11px] text-slate-400 dark:text-slate-500 italic">
                                                                    "依据原文: \"" {ev} "\""
                                                                </div>
                                                            }.into_any()
                                                        } else {
                                                            view! { <div></div> }.into_any()
                                                        }}
                                                    </div>
                                                }
                                            }
                                        />
                                    }.into_any()
                                }}
                            </div>

                            // Footer
                            <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-850 flex items-center justify-between">
                                <span class="text-xs text-slate-400 dark:text-slate-500">"采纳后将正式归档至对应设定模块；已忽略候选不再重复出现。"</span>
                                <div class="flex items-center gap-2">
                                    <button
                                        class="px-3.5 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 rounded-md font-medium transition-colors cursor-pointer"
                                        on:click=move |_| is_open.set(false)
                                    >
                                        "关闭"
                                    </button>
                                    <button
                                        class="px-4 py-1.5 text-xs bg-emerald-600 hover:bg-emerald-700 text-white rounded-md font-semibold shadow-xs transition-colors cursor-pointer disabled:opacity-50"
                                        disabled=move || candidates.get().is_empty()
                                        on:click=on_accept_all
                                    >
                                        "全部采纳"
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }
        }}
    }
}
