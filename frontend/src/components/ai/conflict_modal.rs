use leptos::prelude::*;
use shared_types::ConflictInfo;

#[component]
pub fn ConflictModal<F1, F2>(
    is_open: RwSignal<bool>,
    conflict_info: RwSignal<Option<ConflictInfo>>,
    on_update_setting_and_continue: F1,
    on_rewrite_without_conflict: F2,
) -> impl IntoView
where
    F1: Fn(String) + Send + Sync + Copy + 'static,
    F2: Fn() + Send + Sync + Copy + 'static,
{
    view! {
        {move || {
            if is_open.get() {
                if let Some(info) = conflict_info.get() {
                    let proposed_change = RwSignal::new(info.proposed_change.clone().unwrap_or_default());
                    view! {
                        <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-50 p-6">
                            <div class="bg-white rounded-xl shadow-2xl max-w-2xl w-full border border-red-200 overflow-hidden flex flex-col max-h-[85vh] animate-in fade-in zoom-in-95">
                                // Warning Header
                                <div class="px-5 py-3.5 bg-red-50 border-b border-red-200 flex items-center justify-between">
                                    <div class="flex items-center gap-2 text-red-700">
                                        <span class="text-lg">"⚠️"</span>
                                        <h3 class="font-bold text-sm">"设定冲突警告（Strict Consistency Check）"</h3>
                                    </div>
                                    <button
                                        class="text-slate-400 hover:text-slate-600 font-mono text-lg"
                                        on:click=move |_| is_open.set(false)
                                    >
                                        "✕"
                                    </button>
                                </div>

                                // Body
                                <div class="p-5 space-y-4 overflow-y-auto text-xs text-slate-700">
                                    <div>
                                        <span class="font-bold text-slate-900 block mb-1">"【冲突核心点】"</span>
                                        <p class="p-2.5 bg-red-50/60 border border-red-200 rounded-lg text-red-800">
                                            {info.conflict_point.clone()}
                                        </p>
                                    </div>

                                    <div class="grid grid-cols-2 gap-3">
                                        <div>
                                            <span class="font-semibold text-slate-500">"涉及模块："</span>
                                            <span class="font-bold text-slate-800">{info.setting_module.clone()}</span>
                                        </div>
                                        <div>
                                            <span class="font-semibold text-slate-500">"涉及条目："</span>
                                            <span class="font-bold text-slate-800">{info.setting_title.clone()}</span>
                                        </div>
                                    </div>

                                    <div>
                                        <span class="font-bold text-slate-900 block mb-1">"【判定依据与冲突原因】"</span>
                                        <p class="p-2.5 bg-slate-50 border border-slate-200 rounded-lg text-slate-600 leading-relaxed">
                                            {info.reason.clone()}
                                        </p>
                                    </div>

                                    <div>
                                        <span class="font-bold text-blue-700 block mb-1">"【AI 建议的设定改动方案（可直接编辑）】"</span>
                                        <textarea
                                            class="w-full p-2.5 bg-blue-50/40 border border-blue-200 rounded-lg focus:outline-blue-500 min-h-[70px]"
                                            prop:value=move || proposed_change.get()
                                            on:input=move |ev| proposed_change.set(event_target_value(&ev))
                                        ></textarea>
                                    </div>
                                </div>

                                // Choices footer
                                <div class="px-5 py-3 border-t border-slate-200 bg-slate-50 flex items-center justify-end gap-2.5">
                                    <button
                                        class="px-3.5 py-1.5 text-xs text-slate-600 hover:bg-slate-200 rounded-md font-medium transition-colors"
                                        on:click=move |_| is_open.set(false)
                                    >
                                        "取消本次写入"
                                    </button>
                                    <button
                                        class="px-3.5 py-1.5 text-xs bg-amber-100 hover:bg-amber-200 text-amber-900 rounded-md font-semibold transition-colors"
                                        on:click=move |_| {
                                            on_rewrite_without_conflict();
                                            is_open.set(false);
                                        }
                                    >
                                        "换一种不冲突的写法"
                                    </button>
                                    <button
                                        class="px-4 py-1.5 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded-md font-semibold shadow-xs transition-colors"
                                        on:click=move |_| {
                                            on_update_setting_and_continue(proposed_change.get());
                                            is_open.set(false);
                                        }
                                    >
                                        "修改设定后写入正文"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            } else {
                view! { <div></div> }.into_any()
            }
        }}
    }
}
