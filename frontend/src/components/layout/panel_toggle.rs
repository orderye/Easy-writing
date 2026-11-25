use crate::components::icons::{IconChevronLeft, IconChevronRight};
use leptos::prelude::*;

#[component]
pub fn LeftToggle(is_open: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="p-1.5 rounded-md hover:bg-slate-200/80 dark:hover:bg-slate-700/70 text-slate-500 hover:text-slate-800 dark:hover:text-slate-200 transition-all flex items-center justify-center cursor-pointer"
            title=move || if is_open.get() { "折叠设定区" } else { "展开设定区" }
            on:click=move |_| is_open.update(|v| *v = !*v)
        >
            <span class=move || if is_open.get() { "block" } else { "hidden" }>
                <IconChevronLeft class="w-3.5 h-3.5" />
            </span>
            <span class=move || if is_open.get() { "hidden" } else { "block" }>
                <IconChevronRight class="w-3.5 h-3.5" />
            </span>
        </button>
    }
}

#[component]
pub fn RightToggle(is_open: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="p-1.5 rounded-md hover:bg-slate-200/80 dark:hover:bg-slate-700/70 text-slate-500 hover:text-slate-800 dark:hover:text-slate-200 transition-all flex items-center justify-center cursor-pointer"
            title=move || if is_open.get() { "折叠目录区" } else { "展开目录区" }
            on:click=move |_| is_open.update(|v| *v = !*v)
        >
            <span class=move || if is_open.get() { "block" } else { "hidden" }>
                <IconChevronRight class="w-3.5 h-3.5" />
            </span>
            <span class=move || if is_open.get() { "hidden" } else { "block" }>
                <IconChevronLeft class="w-3.5 h-3.5" />
            </span>
        </button>
    }
}
