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
    // 提炼成本与去重（T07）：parallel_4（默认）/ single_call；超长章节分块阈值
    let extraction_mode = RwSignal::new("parallel_4".to_string());
    let chunk_threshold = RwSignal::new("12000".to_string());
    // 冲突检测（T08）：前置检查开关、检查范围、置信度阈值
    let conflict_check_enabled = RwSignal::new(true);
    let conflict_scope = RwSignal::new("all".to_string());
    let conflict_threshold = RwSignal::new("0.6".to_string());
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
            if let Ok(Some(val)) = read_setting("extraction_mode").await {
                extraction_mode.set(val);
            }
            if let Ok(Some(val)) = read_setting("extraction_chunk_threshold").await {
                chunk_threshold.set(val);
            }
            if let Ok(Some(val)) = read_setting("conflict_check_enabled").await {
                conflict_check_enabled.set(val == "true" || val == "1");
            }
            if let Ok(Some(val)) = read_setting("conflict_check_scope").await {
                conflict_scope.set(val);
            }
            if let Ok(Some(val)) = read_setting("conflict_confidence_threshold").await {
                conflict_threshold.set(val);
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
        let current_mode = extraction_mode.get();
        let current_threshold = chunk_threshold.get();
        let current_conflict_enabled = conflict_check_enabled.get();
        let current_conflict_scope = conflict_scope.get();
        let current_conflict_threshold = conflict_threshold.get();

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
            let _ = write_setting("extraction_mode", current_mode).await;
            let _ = write_setting("extraction_chunk_threshold", current_threshold).await;
            // 阈值在保存时夹到 [0,1]：写进 1.5 会让所有提示都被抑制，
            // 而作者以为自己只是「稍微严格一点」。
            let threshold_value = current_conflict_threshold
                .parse::<f32>()
                .unwrap_or(0.6)
                .clamp(0.0, 1.0);
            let _ = write_setting(
                "conflict_check_enabled",
                if current_conflict_enabled {
                    "true".to_string()
                } else {
                    "false".to_string()
                },
            )
            .await;
            let _ = write_setting("conflict_check_scope", current_conflict_scope).await;
            let _ = write_setting(
                "conflict_confidence_threshold",
                format!("{:.2}", threshold_value),
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
            <div class="space-y-2 p-3 rounded-lg border border-slate-200 dark:border-slate-700">
                <div class="flex items-start justify-between gap-4">
                    <div>
                        <label class="font-semibold block text-slate-800 dark:text-slate-200">"插入正文前检查设定冲突"</label>
                        <span class="text-[11px] text-slate-400 dark:text-slate-500">
                            "开启后，AI 生成的文字写入正文前会先对照设定检查；关闭后不再发起这类请求，文字直接插入。"</span>
                    </div>
                    <input
                        type="checkbox"
                        class="w-4 h-4 mt-0.5 text-blue-600 rounded cursor-pointer shrink-0"
                        prop:checked=move || conflict_check_enabled.get()
                        on:change=move |ev| conflict_check_enabled.set(event_target_checked(&ev))
                    />
                </div>
                <div class="grid grid-cols-2 gap-4 pt-1">
                    <div>
                        <label class="font-semibold block mb-1 text-slate-800 dark:text-slate-200">"检查范围"</label>
                        <select
                            class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                            on:change=move |ev| conflict_scope.set(event_target_value(&ev))
                        >
                            <option value="all" selected=move || conflict_scope.get() == "all">
                                "全部设定（默认）"
                            </option>
                            <option value="pinned" selected=move || conflict_scope.get() == "pinned">
                                "仅硬性设定（已标记「恒入」的条目）"
                            </option>
                        </select>
                        <span class="text-[11px] text-slate-400 dark:text-slate-500">
                            "「仅硬性设定」只看在设定面板标记为恒入的条目；没有标记时不会凭空发起检测。"</span>
                    </div>
                    <div>
                        <label class="font-semibold block mb-1 text-slate-800 dark:text-slate-200">"置信度阈值 (0–1)"</label>
                        <input
                            type="number"
                            min="0"
                            max="1"
                            step="0.05"
                            class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                            prop:value=move || conflict_threshold.get()
                            on:input=move |ev| conflict_threshold.set(event_target_value(&ev))
                        />
                        <span class="text-[11px] text-slate-400 dark:text-slate-500">
                            "低于此值的结果不弹窗（默认 0.6）。模型未给出置信度时不按低置信处理。"</span>
                    </div>
                </div>
            </div>
            <div class="grid grid-cols-2 gap-4">
                <div>
                    <label class="font-semibold block mb-1 text-slate-800 dark:text-slate-200">"提炼模式"</label>
                    <select
                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                        on:change=move |ev| extraction_mode.set(event_target_value(&ev))
                    >
                        <option value="parallel_4" selected=move || extraction_mode.get() == "parallel_4">
                            "并行四 Agent（默认，质量更稳）"
                        </option>
                        <option value="single_call" selected=move || extraction_mode.get() == "single_call">
                            "单次调用（省请求，长章节易截断）"
                        </option>
                    </select>
                    <span class="text-[11px] text-slate-400 dark:text-slate-500">
                        "并行四 Agent 分别提炼世界观 / 角色 / 剧情 / 章摘要；单次调用一次返回四类，省 3 次请求。"</span>
                </div>
                <div>
                    <label class="font-semibold block mb-1 text-slate-800 dark:text-slate-200">"章节分块阈值 (字符)"</label>
                    <input
                        type="number"
                        min="500"
                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                        prop:value=move || chunk_threshold.get()
                        on:input=move |ev| chunk_threshold.set(event_target_value(&ev))
                    />
                    <span class="text-[11px] text-slate-400 dark:text-slate-500">
                        "超过此长度的章节会按段落分块提炼后合并；默认 12000，低于 500 按默认处理。"</span>
                </div>
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
