use leptos::prelude::*;

#[component]
pub fn PolishModal<F>(
    is_open: RwSignal<bool>,
    original_text: String,
    polished_text: RwSignal<String>,
    is_loading: RwSignal<bool>,
    on_adopt: F,
) -> impl IntoView
where
    F: Fn(String) + Send + Sync + Copy + 'static,
{
    view! {
        {move || {
            if is_open.get() {
                let orig = original_text.clone();
                let current_polished = polished_text.get();
                view! {
                    <div class="fixed inset-0 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center z-50 p-6">
                        <div class="bg-white rounded-xl shadow-2xl max-w-3xl w-full border border-slate-200 overflow-hidden flex flex-col max-h-[85vh] animate-in fade-in zoom-in-95">
                            // Modal Header
                            <div class="px-5 py-3.5 border-b border-slate-200 flex items-center justify-between bg-slate-50">
                                <div class="flex items-center gap-2">
                                    <span class="text-amber-500 font-bold">"✨"</span>
                                    <h3 class="font-bold text-sm text-slate-800">"AI 修饰对比与确认"</h3>
                                </div>
                                <button
                                    class="text-slate-400 hover:text-slate-600 font-mono text-lg"
                                    on:click=move |_| is_open.set(false)
                                >
                                    "✕"
                                </button>
                            </div>

                            // Diff comparison body
                            <div class="p-5 flex-1 overflow-y-auto grid grid-cols-2 gap-4">
                                // Original
                                <div class="flex flex-col space-y-1.5">
                                    <span class="text-xs font-semibold text-slate-500">"【原文草稿】"</span>
                                    <div class="flex-1 p-3 bg-slate-50 border border-slate-200 rounded-lg text-xs text-slate-700 whitespace-pre-wrap leading-relaxed select-text min-h-[150px]">
                                        {orig}
                                    </div>
                                </div>

                                // Polished
                                <div class="flex flex-col space-y-1.5">
                                    <div class="flex items-center justify-between">
                                        <span class="text-xs font-semibold text-blue-600">"【AI 润色与补全结果】"</span>
                                        {move || if is_loading.get() {
                                            view! { <span class="text-[10px] text-amber-500 animate-spin">"↻ 正在生成..."</span> }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }}
                                    </div>
                                    <textarea
                                        class="flex-1 p-3 bg-blue-50/40 border border-blue-200 rounded-lg text-xs text-slate-800 focus:outline-blue-500 leading-relaxed min-h-[150px]"
                                        prop:value=move || polished_text.get()
                                        on:input=move |ev| polished_text.set(event_target_value(&ev))
                                    ></textarea>
                                </div>
                            </div>

                            // Actions footer
                            <div class="px-5 py-3 border-t border-slate-200 bg-slate-50 flex items-center justify-end gap-2.5">
                                <button
                                    class="px-3.5 py-1.5 text-xs text-slate-600 hover:bg-slate-200 rounded-md font-medium transition-colors"
                                    on:click=move |_| is_open.set(false)
                                >
                                    "放弃"
                                </button>
                                <button
                                    class="px-4 py-1.5 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded-md font-semibold shadow-xs transition-colors"
                                    on:click=move |_| {
                                        on_adopt(current_polished.clone());
                                        is_open.set(false);
                                    }
                                >
                                    "✓ 采纳并写回"
                                </button>
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
