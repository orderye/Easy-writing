use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::Agent;

/// 「多 Agent 接口绑定」标签页。
///
/// 从原 config_modal.rs 拆出：该标签页只依赖 Agent 列表，与其他标签页无状态耦合。
/// 原先所有标签页共处一个组件、共用一份信号，任一标签页保存都会牵动全部状态；
/// 拆开后每个标签页各自持有状态，副作用范围收敛。
#[component]
pub fn AgentTab(agents: RwSignal<Vec<Agent>>) -> impl IntoView {
    // 每个 Agent 的提示词草稿（按 agent.id 独立），避免各行互相干扰
    let drafts = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let save_status = RwSignal::new(String::new());

    let on_save_agent = move |ag: Agent| {
        let prompt_text = drafts
            .get()
            .get(&ag.id)
            .cloned()
            .unwrap_or_else(|| ag.system_prompt.clone());
        let key = ag.key.clone();
        let profile_id = ag.llm_profile_id.clone();
        let enabled = ag.enabled;

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("update_agent")
                .arg("key", key)
                .arg("system_prompt", prompt_text)
                .opt("llm_profile_id", profile_id)
                .arg("enabled", enabled)
                .send_unit()
                .await;

            match res {
                Ok(_) => save_status.set("✓ 已保存该 Agent 的提示词".to_string()),
                Err(e) => save_status.set(format!("✕ 保存失败: {}", e)),
            }
        });
    };

    view! {
        <div class="space-y-4">
            <p class="text-xs text-slate-500 dark:text-slate-400">
                "每个模块由独立预设 Agent 负责，各 Agent 可绑定不同的大模型 API 配置。"
            </p>

            {move || {
                let msg = save_status.get();
                if msg.is_empty() {
                    view! { <div></div> }.into_any()
                } else if msg.starts_with('✓') {
                    view! {
                        <div class="p-2.5 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-md text-xs font-semibold text-emerald-800 dark:text-emerald-300">
                            {msg}
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="p-2.5 bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-800 rounded-md text-xs font-semibold text-red-800 dark:text-red-300">
                            {msg}
                        </div>
                    }.into_any()
                }
            }}

            <div class="space-y-3">
                <For
                    each=move || agents.get()
                    key=|a| a.id.clone()
                    children=move |ag| {
                        let ag_for_save = ag.clone();
                        let ag_id = ag.id.clone();
                        let initial = ag.system_prompt.clone();

                        // 初始化草稿（若尚未存在）
                        let current = {
                            let id = ag_id.clone();
                            move || {
                                drafts
                                    .get()
                                    .get(&id)
                                    .cloned()
                                    .unwrap_or_else(|| initial.clone())
                            }
                        };

                        view! {
                            <div class="border border-slate-200 dark:border-slate-700 rounded-lg p-3 bg-white dark:bg-slate-900 space-y-2">
                                <div class="flex items-center justify-between">
                                    <div class="flex items-center gap-2">
                                        <span class="font-bold text-xs text-slate-800 dark:text-slate-100">
                                            {ag.name.clone()}
                                        </span>
                                        <span class="text-[10px] text-slate-400 font-mono">
                                            "key: " {ag.key.clone()}
                                        </span>
                                    </div>
                                    <div class="flex items-center gap-2">
                                        <span class="text-[10px] bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 px-2 py-0.5 rounded">
                                            {if ag.enabled { "已启用" } else { "已停用" }}
                                        </span>
                                        <button
                                            class="text-[11px] bg-blue-600 hover:bg-blue-700 text-white px-2.5 py-1 rounded font-medium transition-colors cursor-pointer"
                                            on:click=move |_| on_save_agent(ag_for_save.clone())
                                        >
                                            "保存"
                                        </button>
                                    </div>
                                </div>
                                <textarea
                                    class="w-full text-xs text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:bg-white dark:focus:bg-slate-900 focus:outline-blue-500 min-h-[50px]"
                                    prop:value=current
                                    on:input={
                                        let id = ag_id.clone();
                                        move |ev| {
                                            let v = event_target_value(&ev);
                                            drafts.update(|m| { m.insert(id.clone(), v); });
                                        }
                                    }
                                ></textarea>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}
