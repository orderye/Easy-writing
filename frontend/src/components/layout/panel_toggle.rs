use leptos::prelude::*;

#[component]
pub fn LeftToggle(is_open: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="p-1 rounded hover:bg-slate-200 text-slate-500 hover:text-slate-800 transition-colors flex items-center gap-1 cursor-pointer"
            title=move || if is_open.get() { "折叠设定区" } else { "展开设定区" }
            on:click=move |_| is_open.update(|v| *v = !*v)
        >
            <span class="text-xs font-mono font-bold">
                {move || if is_open.get() { "◀" } else { "▶" }}
            </span>
        </button>
    }
}

#[component]
pub fn RightToggle(is_open: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="p-1 rounded hover:bg-slate-200 text-slate-500 hover:text-slate-800 transition-colors flex items-center gap-1 cursor-pointer"
            title=move || if is_open.get() { "折叠目录区" } else { "展开目录区" }
            on:click=move |_| is_open.update(|v| *v = !*v)
        >
            <span class="text-xs font-mono font-bold">
                {move || if is_open.get() { "▶" } else { "◀" }}
            </span>
        </button>
    }
}
