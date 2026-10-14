use leptos::prelude::*;

#[component]
pub fn LeftToggle(is_open: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="px-2 py-1 text-xs font-medium rounded hover:bg-slate-200 transition-colors flex items-center gap-1 text-slate-600"
            title=move || if is_open.get() { "折叠设定区" } else { "展开设定区" }
            on:click=move |_| is_open.update(|v| *v = !*v)
        >
            <span class="text-sm font-mono">
                {move || if is_open.get() { "◀" } else { "▶ 设定" }}
            </span>
        </button>
    }
}

#[component]
pub fn RightToggle(is_open: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="px-2 py-1 text-xs font-medium rounded hover:bg-slate-200 transition-colors flex items-center gap-1 text-slate-600"
            title=move || if is_open.get() { "折叠目录区" } else { "展开目录区" }
            on:click=move |_| is_open.update(|v| *v = !*v)
        >
            <span class="text-sm font-mono">
                {move || if is_open.get() { "▶" } else { "卷目 ◀" }}
            </span>
        </button>
    }
}
