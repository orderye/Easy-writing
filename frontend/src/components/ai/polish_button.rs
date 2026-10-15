use crate::components::ai::polish_modal::PolishModal;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;

#[component]
pub fn PolishButton(
    module_key: &'static str,
    content: RwSignal<String>,
    #[prop(optional)] label: Option<&'static str>,
) -> impl IntoView {
    let is_open = RwSignal::new(false);
    let is_loading = RwSignal::new(false);
    let original_draft = RwSignal::new(String::new());
    let polished_draft = RwSignal::new(String::new());

    let button_label = label.unwrap_or("AI 修饰");

    let on_click_polish = move |_| {
        let current_text = content.get();
        original_draft.set(current_text.clone());
        polished_draft.set("正在调用专业 Agent 分析与润色...".to_string());
        is_open.set(true);
        is_loading.set(true);

        let mod_key = module_key.to_string();
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("ai_polish")
                .arg("module_key", mod_key)
                .arg("current_content", current_text)
                .send::<String>()
                .await;

            match res {
                Ok(result_text) => {
                    polished_draft.set(result_text);
                }
                Err(err) => {
                    polished_draft.set(format!(
                        "修饰请求失败: {}\n请检查系统设置中的模型接口配置与 API Key。",
                        err
                    ));
                }
            }
            is_loading.set(false);
        });
    };

    let on_adopt = move |new_text: String| {
        content.set(new_text);
    };

    view! {
        <button
            type="button"
            class="inline-flex items-center gap-1 text-[11px] text-amber-700 bg-amber-50 hover:bg-amber-100 px-2 py-0.5 rounded border border-amber-200 transition-colors font-medium cursor-pointer shadow-2xs shrink-0 select-none"
            title="调用对应专业 Agent 进行润色修饰"
            on:click=on_click_polish
        >
            <span class="text-amber-500 font-bold">"✨"</span>
            <span>{button_label}</span>
        </button>

        {move || {
            if is_open.get() {
                view! {
                    <PolishModal
                        is_open=is_open
                        original_text=original_draft.get()
                        polished_text=polished_draft
                        is_loading=is_loading
                        on_adopt=on_adopt
                    />
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }
        }}
    }
}
