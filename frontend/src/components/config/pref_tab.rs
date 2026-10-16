use crate::components::config::app_settings::{read_setting, write_setting};
use crate::state::AppContext;
use leptos::prelude::*;

/// 「写作偏好」标签页。
///
/// 从原 config_modal.rs 拆出：该标签页的表单草稿与保存逻辑自成一体，
/// 与其他标签页无状态耦合。拆开后每个标签页各自持有状态，副作用范围收敛。
#[component]
pub fn PrefTab(app_ctx: AppContext) -> impl IntoView {
    // App Preferences state
    let theme = RwSignal::new("system".to_string());
    let font_size = RwSignal::new("16".to_string());
    let line_height = RwSignal::new("1.8".to_string());
    let auto_save_interval = RwSignal::new("30".to_string());
    // 保存章节后自动提炼（PRD §4.2 P1，默认关闭）
    let auto_extract = RwSignal::new(false);
    let pref_status = RwSignal::new(String::new());

    // 组件挂载时加载该标签页自己的偏好；原先由父组件 load_data 统一读取
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            // Load Preferences
            if let Ok(Some(val)) = read_setting("theme").await {
                theme.set(val);
            }
            if let Ok(Some(val)) = read_setting("font_size").await {
                font_size.set(val);
            }
            if let Ok(Some(val)) = read_setting("line_height").await {
                line_height.set(val);
            }
            if let Ok(Some(val)) = read_setting("auto_save_interval").await {
                auto_save_interval.set(val);
            }
            if let Ok(Some(val)) = read_setting("auto_extract").await {
                auto_extract.set(val == "true" || val == "1");
            }
        });
    });

    // Save Preferences
    let on_save_preferences = move |_| {
        pref_status.set("正在保存偏好设置...".to_string());
        let current_theme = theme.get();
        let current_fs = font_size.get();
        let current_lh = line_height.get();
        let current_intv = auto_save_interval.get();
        let current_auto_extract = auto_extract.get();

        leptos::task::spawn_local(async move {
            let _ = write_setting("theme", current_theme.clone()).await;
            let _ = write_setting("font_size", current_fs.clone()).await;
            let _ = write_setting("line_height", current_lh.clone()).await;
            let _ = write_setting("auto_save_interval", current_intv.clone()).await;
            let _ = write_setting(
                "auto_extract",
                if current_auto_extract {
                    "true".to_string()
                } else {
                    "false".to_string()
                },
            )
            .await;

            // Apply live
            app_ctx.theme.set(current_theme.clone());
            crate::bridge::theme::apply_app_theme(&current_theme);

            let fs_num = current_fs.parse::<u32>().unwrap_or(16);
            app_ctx.font_size.set(fs_num);
            app_ctx.line_height.set(current_lh.clone());
            crate::bridge::theme::set_editor_style(fs_num, &current_lh);

            if let Ok(intv_num) = current_intv.parse::<u32>() {
                app_ctx.auto_save_interval.set(intv_num);
            }

            pref_status.set("✓ 偏好设置已更新并持久化！".to_string());
        });
    };

    view! {
        <div class="max-w-md space-y-5 text-xs text-slate-700 dark:text-slate-300">
            <div class="space-y-1.5">
                <label class="font-semibold block text-slate-800 dark:text-slate-200">"外观主题"</label>
                <div class="grid grid-cols-3 gap-2">
                    <button
                        type="button"
                        class=move || format!(
                            "px-3 py-2.5 rounded border text-center transition-colors cursor-pointer {}",
                            if theme.get() == "system" {
                                "border-blue-600 bg-blue-50 text-blue-700 font-bold dark:bg-blue-950/70 dark:border-blue-500 dark:text-blue-300"
                            } else {
                                "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/60"
                            }
                        )
                        on:click=move |_| {
                            theme.set("system".to_string());
                            crate::bridge::theme::apply_app_theme("system");
                        }
                    >
                        "🌓 跟随系统"
                    </button>
                    <button
                        type="button"
                        class=move || format!(
                            "px-3 py-2.5 rounded border text-center transition-colors cursor-pointer {}",
                            if theme.get() == "light" {
                                "border-blue-600 bg-blue-50 text-blue-700 font-bold dark:bg-blue-950/70 dark:border-blue-500 dark:text-blue-300"
                            } else {
                                "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/60"
                            }
                        )
                        on:click=move |_| {
                            theme.set("light".to_string());
                            crate::bridge::theme::apply_app_theme("light");
                        }
                    >
                        "☀️ 浅色模式"
                    </button>
                    <button
                        type="button"
                        class=move || format!(
                            "px-3 py-2.5 rounded border text-center transition-colors cursor-pointer {}",
                            if theme.get() == "dark" {
                                "border-blue-600 bg-blue-50 text-blue-700 font-bold dark:bg-blue-950/70 dark:border-blue-500 dark:text-blue-300"
                            } else {
                                "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/60"
                            }
                        )
                        on:click=move |_| {
                            theme.set("dark".to_string());
                            crate::bridge::theme::apply_app_theme("dark");
                        }
                    >
                        "🌙 深色模式"
                    </button>
                </div>
            </div>
            <div class="space-y-1">
                <label class="font-semibold block text-slate-800 dark:text-slate-200">"正文字号 (px)"</label>
                <input
                    type="number"
                    class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                    prop:value=move || font_size.get()
                    on:input=move |ev| font_size.set(event_target_value(&ev))
                />
            </div>
            <div class="space-y-1">
                <label class="font-semibold block text-slate-800 dark:text-slate-200">"正文行高"</label>
                <input
                    type="text"
                    class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                    prop:value=move || line_height.get()
                    on:input=move |ev| line_height.set(event_target_value(&ev))
                />
            </div>
            <div class="space-y-1">
                <label class="font-semibold block text-slate-800 dark:text-slate-200">"自动保存间隔 (秒)"</label>
                <input
                    type="number"
                    class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                    prop:value=move || auto_save_interval.get()
                    on:input=move |ev| auto_save_interval.set(event_target_value(&ev))
                />
            </div>
            <div class="flex items-start justify-between gap-4 p-3 rounded-lg border border-slate-200 dark:border-slate-700">
                <div>
                    <label class="font-semibold block text-slate-800 dark:text-slate-200">"保存章节后自动提炼"</label>
                    <span class="text-[11px] text-slate-400 dark:text-slate-500">
                        "开启后，保存正文时后台静默提炼世界观 / 角色 / 剧情 / 章摘要候选；默认关闭，仅手动点「提炼本章」。"</span>
                </div>
                <input
                    type="checkbox"
                    class="w-4 h-4 mt-0.5 text-blue-600 rounded cursor-pointer shrink-0"
                    prop:checked=move || auto_extract.get()
                    on:change=move |ev| auto_extract.set(event_target_checked(&ev))
                />
            </div>
            <button
                class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium shadow-xs transition-colors cursor-pointer"
                on:click=on_save_preferences
            >
                "保存写作偏好"
            </button>

            {move || if !pref_status.get().is_empty() {
                view! {
                    <div class="p-2.5 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-md text-xs font-semibold text-emerald-800 dark:text-emerald-300">
                        {pref_status.get()}
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}
        </div>
    }
}
