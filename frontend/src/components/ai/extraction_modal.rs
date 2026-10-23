use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use shared_types::{Character, ExtractionCandidate};

/// 后端返回的单条 Agent 失败记录（已脱敏，不含正文与密钥）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentFailure {
    pub agent: String,
    pub kind: String,
    pub reason: String,
}

/// 一次提炼的完整结果。
///
/// 关键点：`all_succeeded` 与 `failures` 让前端能区分
/// 「本章确实没有新设定」和「提炼失败」—— 后者必须提示作者，
/// 否则 Key 失效会被静默成「模型认为本章无新内容」。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractionOutcome {
    pub candidates: Vec<ExtractionCandidate>,
    pub succeeded_agents: usize,
    pub failures: Vec<AgentFailure>,
    pub skipped_fingerprint_match: bool,
    pub all_succeeded: bool,
}

fn failure_hint(outcome: &ExtractionOutcome) -> Option<String> {
    if outcome.failures.is_empty() {
        return None;
    }
    let is_auth = outcome.failures.iter().any(|f| f.kind == "auth");
    if is_auth {
        return Some(
            "模型服务拒绝了本次请求，请检查 API Key 是否正确、是否已过期或欠费。".to_string(),
        );
    }
    let agents: Vec<&str> = outcome.failures.iter().map(|f| f.agent.as_str()).collect();
    Some(format!(
        "{} / {} 个 Agent 未完成（{}）。详情：{}",
        outcome.succeeded_agents,
        outcome.succeeded_agents + outcome.failures.len(),
        agents.join("、"),
        outcome.failures[0].reason
    ))
}

#[component]
pub fn ExtractionModal(
    app_ctx: AppContext,
    is_open: RwSignal<bool>,
    candidates: RwSignal<Vec<ExtractionCandidate>>,
    is_extracting: RwSignal<bool>,
) -> impl IntoView {
    let failure_message: RwSignal<Option<String>> = RwSignal::new(None);
    // 疑似同人提示需要把锚点 id 翻成可读的名字：先查角色，查不到再查候选池。
    let characters = RwSignal::new(Vec::<Character>::new());
    let reload_characters = move || {
        if let Some(novel) = app_ctx.current_novel.get_untracked() {
            leptos::task::spawn_local(async move {
                if let Ok(list) = invoke_cmd("list_characters")
                    .arg("novel_id", novel.id)
                    .send::<Vec<Character>>()
                    .await
                {
                    characters.set(list);
                }
            });
        }
    };
    // 打开面板时刷新一次：合并会改角色，缓存过期会把已合并的提示又显示出来。
    Effect::new(move |_| {
        if is_open.get() {
            reload_characters();
        }
    });

    let on_merge = move |candidate_id: String, target_character_id: String| {
        let cid = candidate_id.clone();
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("merge_character_candidate")
                .arg("candidate_id", cid.clone())
                .arg("target_character_id", target_character_id)
                .send_unit()
                .await;
            if res.is_ok() {
                app_ctx.lexicon_version.update(|v| *v += 1);
            }
            let mut list = candidates.get();
            list.retain(|c| c.id != cid);
            candidates.set(list);
        });
        reload_characters();
    };

    let on_resolve = move |candidate_id: String, action: &'static str| {
        let cid = candidate_id.clone();
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("resolve_candidate")
                .arg("candidate_id", cid.clone())
                .arg("action", action.to_string())
                .opt("custom_payload", None::<String>)
                .send_unit()
                .await;

            if action == "accept" {
                app_ctx.lexicon_version.update(|v| *v += 1);
            }

            let mut list = candidates.get();
            list.retain(|c| c.id != cid);
            candidates.set(list);
        });
    };

    let on_accept_all = move |_| {
        let current_list = candidates.get();
        leptos::task::spawn_local(async move {
            for c in current_list {
                let _ = invoke_cmd("resolve_candidate")
                    .arg("candidate_id", c.id)
                    .arg("action", "accept".to_string())
                    .opt("custom_payload", None::<String>)
                    .send_unit()
                    .await;
            }
            app_ctx.lexicon_version.update(|v| *v += 1);
            candidates.set(Vec::new());
        });
    };

    let on_reextract = move |_| {
        if let (Some(novel), Some(chapter)) =
            (app_ctx.current_novel.get(), app_ctx.current_chapter.get())
        {
            let nid = novel.id;
            let cid = chapter.id;
            is_extracting.set(true);
            failure_message.set(None);
            leptos::task::spawn_local(async move {
                match invoke_cmd("trigger_chapter_extract")
                    .arg("novel_id", nid)
                    .arg("chapter_id", cid)
                    .send::<ExtractionOutcome>()
                    .await
                {
                    Ok(outcome) => {
                        if let Some(msg) = failure_hint(&outcome) {
                            // 提炼失败必须让作者看见 —— 静默成「无新设定」
                            // 会让配置错误永久无法被发现。
                            failure_message.set(Some(msg));
                        }
                        candidates.set(outcome.candidates);
                    }
                    Err(e) => {
                        failure_message.set(Some(format!("提炼请求失败：{e}")));
                    }
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
                                {move || failure_message.get().map(|msg| {
                                    view! {
                                        <div class="rounded-lg border border-red-300 dark:border-red-900 bg-red-50 dark:bg-red-950/40 px-4 py-3">
                                            <p class="text-xs font-semibold text-red-800 dark:text-red-300">
                                                "⚠ 本次提炼未完整完成"
                                            </p>
                                            <p class="text-xs text-red-700 dark:text-red-400 mt-1 leading-relaxed">
                                                {msg}
                                            </p>
                                            <p class="text-[11px] text-red-600/80 dark:text-red-500/80 mt-1.5">
                                                "下方候选仅来自成功的 Agent，未完成的部分不会计入本章提炼记录，可稍后重试补齐。"
                                            </p>
                                        </div>
                                    }
                                })}
                                {move || if is_extracting.get() {
                                    view! {
                                        <div class="py-12 flex flex-col items-center justify-center text-slate-500 dark:text-slate-400 space-y-3">
                                            <div class="animate-spin text-3xl">"✨"</div>
                                            <p class="text-xs font-medium">"世界观、角色卡、剧情、大纲 4 大 Agent 正在受限并行提炼本章设定……"</p>
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
                                                let (kind_label, kind_class) = match cand.kind.as_str() {
                                                    "worldview" => ("世界观设定", "text-blue-700 dark:text-blue-300 bg-blue-50 dark:bg-blue-950/60 border-blue-200 dark:border-blue-800"),
                                                    "character" => ("角色·角色卡", "text-purple-700 dark:text-purple-300 bg-purple-50 dark:bg-purple-950/60 border-purple-200 dark:border-purple-800"),
                                                    "plot" => ("剧情伏笔", "text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/60 border-amber-200 dark:border-amber-800"),
                                                    _ => ("大纲章纲", "text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/60 border-emerald-200 dark:border-emerald-800"),
                                                };

                                                let is_create = cand.action == "create";
                                                let title_str = if !cand.title.trim().is_empty() {
                                                    cand.title.clone()
                                                } else {
                                                    "未命名条目".to_string()
                                                };
                                                // 疑似同人提示：锚点可能是角色 id，也可能是候选 id
                                                let dup_anchor = cand.possible_duplicate_of.clone();
                                                let c_id_for_merge = c_id.clone();

                                                view! {
                                                    <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-3 bg-white dark:bg-slate-850 shadow-2xs space-y-2">
                                                        <div class="flex items-center justify-between">
                                                            <div class="flex items-center gap-2">
                                                                <span class=format!("text-xs font-bold px-2 py-0.5 rounded border {}", kind_class)>
                                                                    {kind_label}
                                                                </span>
                                                                <span class="text-xs font-bold text-slate-800 dark:text-slate-100">
                                                                    {title_str}
                                                                </span>
                                                                <span class=format!(
                                                                    "text-[10px] px-1.5 py-0.5 rounded font-medium border {}",
                                                                    if is_create {
                                                                        "bg-emerald-50 dark:bg-emerald-950/60 text-emerald-700 dark:text-emerald-300 border-emerald-200 dark:border-emerald-800"
                                                                    } else {
                                                                        "bg-amber-50 dark:bg-amber-950/60 text-amber-700 dark:text-amber-300 border-amber-200 dark:border-amber-800"
                                                                    }
                                                                )>
                                                                    {if is_create { "新建条目" } else { "更新既有" }}
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

                                                        {move || {
                                                            let Some(anchor) = dup_anchor.clone() else {
                                                                return view! { <span></span> }.into_any();
                                                            };
                                                            // 先查角色（可直接合并），查不到再查候选池
                                                            if let Some(target) = characters.get().into_iter().find(|c| c.id == anchor) {
                                                                let target_id = target.id.clone();
                                                                let target_name = target.name.clone();
                                                                let merge_cid = c_id_for_merge.clone();
                                                                let name_for_button = target_name.clone();
                                                                return view! {
                                                                    <div class="flex items-center gap-2 rounded-md border border-amber-300 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/40 px-2.5 py-1.5">
                                                                        <span class="text-[11px] text-amber-800 dark:text-amber-300">
                                                                            "疑似与已有角色「" {target_name} "」是同一人"
                                                                        </span>
                                                                        {if is_create {
                                                                            view! {
                                                                                <button
                                                                                    class="ml-auto px-2 py-0.5 text-[11px] bg-amber-600 hover:bg-amber-700 text-white rounded cursor-pointer"
                                                                                    title="把此名称写入该角色的别名，并把描述并入角色卡"
                                                                                    on:click=move |_| on_merge(merge_cid.clone(), target_id.clone())
                                                                                >
                                                                                    "合并到「" {name_for_button} "」"
                                                                                </button>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! {
                                                                                <span class="ml-auto text-[11px] text-amber-700 dark:text-amber-400">
                                                                                    "采纳将更新该角色"
                                                                                </span>
                                                                            }.into_any()
                                                                        }}
                                                                    </div>
                                                                }.into_any();
                                                            }
                                                            // 候选池锚点：只能提示，合并要等对方先入库
                                                            let other_name = candidates.get().into_iter()
                                                                .find(|c| c.id == anchor)
                                                                .map(|c| c.title);
                                                            if let Some(name) = other_name {
                                                                return view! {
                                                                    <div class="rounded-md border border-amber-200 dark:border-amber-900 bg-amber-50/60 dark:bg-amber-950/30 px-2.5 py-1.5">
                                                                        <span class="text-[11px] text-amber-700 dark:text-amber-300">
                                                                            "与本批候选「" {name} "」疑似同人，建议先处理那一条"
                                                                        </span>
                                                                    </div>
                                                                }.into_any();
                                                            }
                                                            view! { <span></span> }.into_any()
                                                        }}

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
