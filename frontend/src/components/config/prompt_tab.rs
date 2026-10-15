use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::PromptTemplate;

/// 「修饰提示词模板」标签页。
///
/// 从原 config_modal.rs 拆出：该标签页只依赖模板列表，与其他标签页无状态耦合。
/// 每个模板的编辑草稿与保存提示都是该行局部信号，留在循环内部即可。
#[component]
pub fn PromptTab(prompt_templates: RwSignal<Vec<PromptTemplate>>) -> impl IntoView {
    view! {
        <div class="space-y-4">
            <div class="flex items-center justify-between pb-2 border-b border-slate-200 dark:border-slate-700">
                <div>
                    <h4 class="text-xs font-bold text-slate-800 dark:text-slate-100">"6 处修饰模块独立 Prompt 模板"</h4>
                    <p class="text-[11px] text-slate-500 dark:text-slate-400">"自定义世界观、角色卡、剧情伏笔、全篇梗概、卷简介及章摘要的 AI 润色修饰指令，支持一键恢复出厂预设。"</p>
                </div>
            </div>

            <div class="space-y-3">
                {move || {
                    let tmpls = prompt_templates.get();
                    tmpls.into_iter().map(|item| {
                        let key = StoredValue::new(item.key);
                        let name = item.name;
                        let def_tmpl = StoredValue::new(item.default_template);
                        let tmpl_signal = RwSignal::new(item.template);
                        let save_msg = RwSignal::new(String::new());

                        view! {
                            <div class="border border-slate-200 dark:border-slate-700 rounded-lg p-3 bg-slate-50/50 dark:bg-slate-850/50 space-y-2">
                                <div class="flex items-center justify-between">
                                    <div class="flex items-center gap-2">
                                        <span class="font-bold text-xs text-slate-800 dark:text-slate-100">{name}</span>
                                        <span class="text-[10px] font-mono px-1.5 py-0.2 bg-slate-200 dark:bg-slate-750 text-slate-600 dark:text-slate-300 rounded">
                                            {move || key.get_value()}
                                        </span>
                                        {move || if tmpl_signal.get() != def_tmpl.get_value() {
                                            view! {
                                                <span class="text-[10px] px-1.5 py-0.2 bg-amber-100 dark:bg-amber-950/60 text-amber-800 dark:text-amber-300 rounded border border-amber-200 dark:border-amber-700">
                                                    "已自定义"
                                                </span>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <span class="text-[10px] px-1.5 py-0.2 bg-slate-100 dark:bg-slate-800 text-slate-400 rounded">
                                                    "默认预设"
                                                </span>
                                            }.into_any()
                                        }}
                                    </div>
                                    <div class="flex items-center gap-2">
                                        <span class="text-[11px] text-emerald-600 dark:text-emerald-400">{move || save_msg.get()}</span>
                                        {move || if tmpl_signal.get() != def_tmpl.get_value() {
                                            view! {
                                                <button
                                                    class="text-xs text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 px-2 py-1 rounded transition-colors cursor-pointer"
                                                    on:click=move |_| {
                                                        let k = key.get_value();
                                                        let dt = def_tmpl.get_value();
                                                        leptos::task::spawn_local(async move {
                                                            if invoke_cmd("reset_prompt_template")
                                                                .arg("key", k)
                                                                .send_unit()
                                                                .await
                                                                .is_ok()
                                                            {
                                                                tmpl_signal.set(dt);
                                                                save_msg.set("已恢复出厂默认预设 ✓".to_string());
                                                            }
                                                        });
                                                    }
                                                >
                                                    "恢复默认"
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! { <div></div> }.into_any()
                                        }}
                                        <button
                                            class="text-xs bg-blue-600 hover:bg-blue-700 text-white px-3 py-1 rounded font-medium shadow-xs transition-colors cursor-pointer"
                                            on:click=move |_| {
                                                let k = key.get_value();
                                                let t = tmpl_signal.get();
                                                leptos::task::spawn_local(async move {
                                                    if invoke_cmd("save_prompt_template")
                                                        .arg("key", k)
                                                        .arg("template", t)
                                                        .send_unit()
                                                        .await
                                                        .is_ok()
                                                    {
                                                        save_msg.set("已保存自定义模板 ✓".to_string());
                                                    }
                                                });
                                            }
                                        >
                                            "保存修改"
                                        </button>
                                    </div>
                                </div>
                                <textarea
                                    class="w-full text-xs text-slate-700 dark:text-slate-200 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md p-2 min-h-[70px] leading-relaxed font-mono focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                    prop:value=move || tmpl_signal.get()
                                    on:input=move |ev| tmpl_signal.set(event_target_value(&ev))
                                ></textarea>
                            </div>
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>
        </div>
    }
}
