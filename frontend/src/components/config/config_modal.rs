use crate::components::config::agent_tab::AgentTab;
use crate::components::config::app_settings::{read_setting, write_setting};
use crate::components::config::pref_tab::PrefTab;
use crate::components::config::prompt_tab::PromptTab;
use crate::components::config::webdav_tab::WebdavTab;
use crate::state::AppContext;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use leptos::prelude::*;
use shared_types::{
    Agent, AppUpdateInfo, GenerationParams, LlmProfile, PromptTemplate, RemoteModel,
};

/// 把 llm_profile.params 里的生成参数渲染成一行摘要文案（PRD §5.1 P0）。
fn gen_params_summary(params_json: &str) -> String {
    let params = GenerationParams::parse_from_str(params_json);
    if params.is_empty() {
        return "生成参数：接口默认（temperature / 最大输出长度 / 上下文窗口 / 流式均未覆盖）".to_string();
    }

    let temp = params
        .temperature
        .map(|t| t.to_string())
        .unwrap_or_else(|| "默认".to_string());
    let max_tokens = params
        .max_tokens
        .map(|m| m.to_string())
        .unwrap_or_else(|| "默认".to_string());
    let context_window = params
        .context_window
        .map(|c| c.to_string())
        .unwrap_or_else(|| "默认".to_string());
    let stream = params.stream.unwrap_or(true);

    format!(
        "生成参数：temperature {} · 最大输出长度 {} · 上下文窗口 {} · {}",
        temp,
        max_tokens,
        context_window,
        if stream {
            "流式输出"
        } else {
            "一次性返回"
        }
    )
}

#[component]
pub fn ConfigModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    // 标签页状态提到 AppContext：跨设备冲突提示条需要跳到「WebDAV 整库同步」页。
    let active_tab = app_ctx.config_active_tab; // "llm" | "agent" | "prompt" | "webdav" | "pref"
    let profiles = RwSignal::new(Vec::<LlmProfile>::new());
    let agents = RwSignal::new(Vec::<Agent>::new());
    let prompt_templates = RwSignal::new(Vec::<PromptTemplate>::new());

    // New profile form state
    let form_name = RwSignal::new(String::new());
    let form_protocol = RwSignal::new("openai".to_string());
    let form_base_url = RwSignal::new("https://api.openai.com/v1".to_string());
    let form_model = RwSignal::new("gpt-4o".to_string());
    let form_api_key = RwSignal::new(String::new());

    // 生成参数（PRD §5.1 P0）：temperature、最大输出长度、是否流式
    let form_temperature = RwSignal::new(String::new());
    let form_max_tokens = RwSignal::new(String::new());
    let form_stream = RwSignal::new(true);
    // 上下文窗口（T06）：决定装配进 prompt 的设定预算
    let form_context_window = RwSignal::new(String::new());

    // 「拉取模型列表」（PRD §5.1 P0）
    let remote_models = RwSignal::new(Vec::<RemoteModel>::new());
    let is_fetching_models = RwSignal::new(false);

    // 编辑已有配置时的生成参数（进入编辑态后展示）
    let editing_id = RwSignal::new(Option::<String>::None);
    let edit_temperature = RwSignal::new(String::new());
    let edit_max_tokens = RwSignal::new(String::new());
    let edit_stream = RwSignal::new(true);
    let edit_context_window = RwSignal::new(String::new());

    let test_status = RwSignal::new(String::new());

    // App Update state
    let auto_check_update = RwSignal::new(true);
    let custom_update_url = RwSignal::new(String::new());
    let update_status_msg = RwSignal::new(String::new());
    let last_check_time_str = RwSignal::new(String::new());

    let load_data = move || {
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_no_args::<Vec<LlmProfile>>("list_llm_profiles").await {
                profiles.set(list);
            }
            if let Ok(list) = invoke_no_args::<Vec<Agent>>("list_agents").await {
                agents.set(list);
            }

            // Load Prompt Templates
            if let Ok(list) = invoke_no_args::<Vec<PromptTemplate>>("list_prompt_templates").await {
                prompt_templates.set(list);
            }

            // Load Update Preferences
            if let Ok(Some(val)) = read_setting("auto_check_update").await {
                auto_check_update.set(val != "false");
            }
            if let Ok(Some(val)) = read_setting("update_check_url").await {
                custom_update_url.set(val);
            }
            if let Ok(Some(val)) = read_setting("last_update_check_time").await {
                last_check_time_str.set(val);
            }
        });
    };

    Effect::new(move |_| {
        if is_open.get() {
            load_data();
        }
    });

    // 把表单里的可选数字解析为 Option，空字符串表示「未设置 / 使用默认」
    let parse_opt_f32 = |raw: String| -> Result<Option<f32>, String> {
        let t = raw.trim();
        if t.is_empty() {
            Ok(None)
        } else {
            t.parse::<f32>()
                .map(Some)
                .map_err(|_| "temperature 需为 0 ~ 2 之间的数字".to_string())
        }
    };
    let parse_opt_u32 = |raw: String, field: &str| -> Result<Option<u32>, String> {
        let t = raw.trim();
        if t.is_empty() {
            Ok(None)
        } else {
            t.parse::<u32>()
                .map(Some)
                .map_err(|_| format!("{}需为正整数", field))
        }
    };

    let on_add_profile = move |_| {
        let key_val = form_api_key.get().trim().to_string();
        if key_val.is_empty() {
            test_status.set(
                "✕ 拦截提示：未填写 API Key 密钥，请输入以 sk- 开头的有效密钥后再保存配置"
                    .to_string(),
            );
            return;
        }

        let temperature = match parse_opt_f32(form_temperature.get()) {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };
        let max_tokens = match parse_opt_u32(form_max_tokens.get(), "最大输出长度") {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };
        let context_window = match parse_opt_u32(form_context_window.get(), "上下文窗口") {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("create_llm_profile")
                .arg("name", form_name.get())
                .arg("protocol", form_protocol.get())
                .arg("base_url", form_base_url.get())
                .arg("model", form_model.get())
                .arg("api_key", form_api_key.get())
                .arg("is_default", profiles.get().is_empty())
                .opt("temperature", temperature)
                .opt("max_tokens", max_tokens)
                .opt("stream", Some(form_stream.get()))
                .opt("context_window", context_window)
                .send::<LlmProfile>()
                .await;

            match res {
                Ok(new_p) => {
                    let mut list = profiles.get();
                    list.push(new_p);
                    profiles.set(list);

                    form_name.set(String::new());
                    form_api_key.set(String::new());
                    form_temperature.set(String::new());
                    form_max_tokens.set(String::new());
                    form_context_window.set(String::new());
                    form_stream.set(true);
                    remote_models.set(Vec::new());
                    test_status.set(
                        "✓ 接口配置已保存至钥匙串，可点击上方「测试连接」验证连通性".to_string(),
                    );
                }
                Err(err) => {
                    test_status.set(format!("✕ 保存失败: {}", err));
                }
            }
        });
    };

    // 「拉取模型列表」：用当前表单的协议 / Base URL / API Key 拉取远端可用模型
    let on_fetch_models = move |_| {
        is_fetching_models.set(true);
        test_status.set("正在拉取模型列表...".to_string());

        let protocol = form_protocol.get();
        let base_url = form_base_url.get();
        let key = form_api_key.get().trim().to_string();
        let api_key = if key.is_empty() { None } else { Some(key) };

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("fetch_remote_models")
                .arg("protocol", protocol)
                .arg("base_url", base_url)
                .opt("api_key", api_key)
                .opt("profile_id", None::<String>)
                .send::<Vec<RemoteModel>>()
                .await;

            match res {
                Ok(list) => {
                    let n = list.len();
                    remote_models.set(list);
                    test_status.set(format!("✓ 已拉取到 {} 个可用模型，点击下方列表即可选用", n));
                }
                Err(err) => {
                    remote_models.set(Vec::new());
                    test_status.set(format!("✕ 拉取模型列表失败: {}", err));
                }
            }
            is_fetching_models.set(false);
        });
    };

    // 保存已有配置的生成参数（PRD §5.1 P0）
    let on_save_profile_params = move |profile: LlmProfile| {
        let temperature = match parse_opt_f32(edit_temperature.get()) {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };
        let max_tokens = match parse_opt_u32(edit_max_tokens.get(), "最大输出长度") {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };
        let context_window = match parse_opt_u32(edit_context_window.get(), "上下文窗口") {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };

        let stream = Some(edit_stream.get());
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("update_llm_profile")
                .arg("id", profile.id.clone())
                .arg("name", profile.name.clone())
                .arg("protocol", profile.protocol.clone())
                .arg("base_url", profile.base_url.clone())
                .arg("model", profile.model.clone())
                .arg("is_default", profile.is_default)
                .opt("temperature", temperature)
                .opt("max_tokens", max_tokens)
                .opt("stream", stream)
                .opt("context_window", context_window)
                .send::<LlmProfile>()
                .await;

            match res {
                Ok(updated) => {
                    let mut list = profiles.get();
                    if let Some(slot) = list.iter_mut().find(|p| p.id == updated.id) {
                        *slot = updated.clone();
                    }
                    profiles.set(list);
                    editing_id.set(None);
                    test_status.set("✓ 生成参数已保存，后续调用该接口时立即生效".to_string());
                }
                Err(err) => {
                    test_status.set(format!("✕ 保存生成参数失败: {}", err));
                }
            }
        });
    };

    let on_delete_profile = move |id: String| {
        let p_id = id.clone();
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("delete_llm_profile")
                .arg("id", p_id.clone())
                .send_unit()
                .await;
            let mut list = profiles.get();
            list.retain(|p| p.id != p_id);
            profiles.set(list);
        });
    };

    let on_test_profile = move |id: String| {
        test_status.set("正在测试连接...".to_string());
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("test_llm_connection")
                .arg("profile_id", id)
                .send_unit()
                .await;
            match res {
                Ok(_) => test_status.set("✓ 连接成功！模型接口响应正常".to_string()),
                Err(err) => test_status.set(format!("✕ 连接失败: {}", err)),
            }
        });
    };

    let on_check_update_now = move |_| {
        app_ctx.is_checking_update.set(true);
        update_status_msg.set("正在连接远程版本源检查更新...".to_string());
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("check_app_update")
                .arg("force", true)
                .send::<AppUpdateInfo>()
                .await;
            app_ctx.is_checking_update.set(false);
            match res {
                Ok(info) => {
                    if info.has_update {
                        update_status_msg.set(format!(
                            "🎉 发现新版本 v{}！已为您打开更新详情",
                            info.latest_version
                        ));
                        app_ctx.available_update.set(Some(info));
                        app_ctx.has_update_badge.set(true);
                        app_ctx.is_update_modal_open.set(true);
                    } else {
                        update_status_msg.set(format!(
                            "✓ 当前已是最新版本 (v{})，暂无新版本发布",
                            info.current_version
                        ));
                    }
                }
                Err(e) => {
                    update_status_msg.set(format!("✕ 检查更新失败：{}", e));
                }
            }
        });
    };

    let on_save_update_settings = move |_| {
        let is_auto = auto_check_update.get();
        let custom_url = custom_update_url.get().trim().to_string();
        leptos::task::spawn_local(async move {
            let _ = write_setting(
                "auto_check_update",
                if is_auto {
                    "true".to_string()
                } else {
                    "false".to_string()
                },
            )
            .await;
            let _ = write_setting("update_check_url", custom_url).await;
            update_status_msg.set("✓ 更新检测设置已保存！".to_string());
        });
    };

    let on_open_external = move |url: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("open_external_url")
                .arg("url", url)
                .send_unit()
                .await;
        });
    };

    view! {
        {move || {
            if is_open.get() {
                let tab = active_tab.get();
                view! {
                    <div class="fixed inset-0 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center z-50 p-6 select-none">
                        <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-4xl w-full border border-slate-200 dark:border-slate-700 overflow-hidden flex flex-col h-[650px] animate-in fade-in zoom-in-95">
                            // Header
                            <div class="px-6 py-4 border-b border-slate-200 dark:border-slate-700 flex items-center justify-between bg-slate-50 dark:bg-slate-800/80">
                                <div class="flex items-center gap-2.5">
                                    <span class="text-xl">"⚙️"</span>
                                    <h3 class="font-bold text-base text-slate-800 dark:text-slate-100">"工作台系统设置"</h3>
                                </div>
                                <button
                                    class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 font-mono text-xl cursor-pointer"
                                    on:click=move |_| is_open.set(false)
                                >
                                    "✕"
                                </button>
                            </div>

                            // Tab navigation
                            <div class="flex border-b border-slate-200 dark:border-slate-700 bg-slate-100 dark:bg-slate-900/60 text-xs font-semibold text-slate-600 dark:text-slate-400 px-6">
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 {}", if tab == "llm" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("llm".to_string())
                                >
                                    <span>"大模型 API 接口"</span>
                                    {move || if app_ctx.missing_keys.get().iter().any(|k| k.scope == "llm_profile") {
                                        view! { <span class="px-1.5 py-0.2 rounded-full text-[9px] bg-red-500 text-white font-bold leading-none">"缺密钥"</span> }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }}
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "agent" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("agent".to_string())
                                >
                                    "多 Agent 接口绑定"
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "prompt" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("prompt".to_string())
                                >
                                    "修饰提示词模板"
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 {}", if tab == "webdav" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("webdav".to_string())
                                >
                                    <span>"WebDAV 整库同步"</span>
                                    {move || if app_ctx.missing_keys.get().iter().any(|k| k.scope == "webdav") {
                                        view! { <span class="px-1.5 py-0.2 rounded-full text-[9px] bg-red-500 text-white font-bold leading-none">"缺密钥"</span> }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }}
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "pref" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("pref".to_string())
                                >
                                    "写作偏好"
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 {}", if tab == "update" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("update".to_string())
                                >
                                    <span>"关于与更新"</span>
                                    {move || if app_ctx.has_update_badge.get() {
                                        view! { <span class="px-1.5 py-0.2 rounded-full text-[9px] bg-red-500 text-white font-bold leading-none animate-pulse">"新版"</span> }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
                                </button>
                            </div>

                            // Tab contents
                            <div class="flex-1 overflow-y-auto p-6 bg-white dark:bg-slate-800">
                                {move || match tab.as_str() {
                                    "agent" => view! { <AgentTab agents=agents /> }.into_any(),

                                    "prompt" => view! { <PromptTab prompt_templates=prompt_templates /> }.into_any(),

                                    "webdav" => view! { <WebdavTab app_ctx=app_ctx /> }.into_any(),
                                    "pref" => view! { <PrefTab app_ctx=app_ctx /> }.into_any(),

                                    "update" => view! {
                                        <div class="space-y-6 max-w-2xl">
                                            // Software Branding & Version Info Card
                                            <div class="flex items-center gap-4 p-4 rounded-xl bg-slate-50 dark:bg-slate-900/60 border border-slate-200 dark:border-slate-700/80">
                                                <div class="w-14 h-14 rounded-2xl bg-gradient-to-tr from-blue-600 to-indigo-500 flex items-center justify-center text-2xl text-white shadow-md shrink-0">
                                                    "✍️"
                                                </div>
                                                <div class="space-y-1">
                                                    <div class="flex items-center gap-2">
                                                        <h4 class="font-bold text-sm text-slate-800 dark:text-slate-100">"Easy-writing · 小说工作台"</h4>
                                                        <span class="px-2 py-0.5 rounded-full text-[10px] font-mono bg-blue-100 dark:bg-blue-900/60 text-blue-700 dark:text-blue-300 font-semibold border border-blue-200 dark:border-blue-800">
                                                            {concat!("v", env!("CARGO_PKG_VERSION"))}
                                                        </span>
                                                    </div>
                                                    <p class="text-xs text-slate-500 dark:text-slate-400">
                                                        "面向中长篇连载小说作者的专业级本地离线写作工作台 · AI 辅助提炼与设定协同"
                                                    </p>
                                                </div>
                                            </div>

                                            // Update Actions & Status Card
                                            <div class="p-4 rounded-xl border border-slate-200 dark:border-slate-700 space-y-4 bg-white dark:bg-slate-800">
                                                <div class="flex items-center justify-between">
                                                    <div>
                                                        <h5 class="font-bold text-xs text-slate-800 dark:text-slate-200">"版本在线更新检查"</h5>
                                                        <p class="text-[11px] text-slate-400 dark:text-slate-500">
                                                            {move || {
                                                                let time = last_check_time_str.get();
                                                                if time.is_empty() {
                                                                    "尚未检查过更新".to_string()
                                                                } else {
                                                                    format!("上次检测时间：{}", time.chars().take(19).collect::<String>().replace('T', " "))
                                                                }
                                                            }}
                                                        </p>
                                                    </div>
                                                    <button
                                                        class=move || {
                                                            format!(
                                                                "px-3.5 py-1.5 rounded-lg text-xs font-semibold shadow-xs transition-colors flex items-center gap-1.5 cursor-pointer {}",
                                                                if app_ctx.is_checking_update.get() {
                                                                    "bg-slate-200 dark:bg-slate-700 text-slate-400 cursor-not-allowed"
                                                                } else {
                                                                    "bg-blue-600 hover:bg-blue-700 active:bg-blue-800 text-white"
                                                                }
                                                            )
                                                        }
                                                        disabled=move || app_ctx.is_checking_update.get()
                                                        on:click=on_check_update_now
                                                    >
                                                        {move || if app_ctx.is_checking_update.get() {
                                                            view! {
                                                                <span>"🔄 检测中..."</span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span>"🔍 立即检查新版本"</span>
                                                            }.into_any()
                                                        }}
                                                    </button>
                                                </div>

                                                // Update status message banner
                                                {move || {
                                                    let msg = update_status_msg.get();
                                                    if msg.is_empty() {
                                                        view! { <div></div> }.into_any()
                                                    } else if msg.starts_with('✓') {
                                                        view! {
                                                            <div class="p-2.5 rounded-lg bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 text-xs text-emerald-800 dark:text-emerald-300 font-medium">
                                                                {msg}
                                                            </div>
                                                        }.into_any()
                                                    } else if msg.starts_with('✕') {
                                                        view! {
                                                            <div class="p-2.5 rounded-lg bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-800 text-xs text-red-800 dark:text-red-300 font-medium">
                                                                {msg}
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="p-2.5 rounded-lg bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800 text-xs text-blue-800 dark:text-blue-300 font-medium">
                                                                {msg}
                                                            </div>
                                                        }.into_any()
                                                    }
                                                }}

                                                // If an update is detected, show shortcut banner
                                                {move || {
                                                    if let Some(info) = app_ctx.available_update.get() {
                                                        let info_clone = info.clone();
                                                        view! {
                                                            <div class="p-3 bg-gradient-to-r from-blue-50 to-indigo-50 dark:from-blue-950/40 dark:to-indigo-950/40 border border-blue-200 dark:border-blue-800 rounded-xl flex items-center justify-between">
                                                                <div>
                                                                    <div class="font-bold text-xs text-blue-900 dark:text-blue-200">
                                                                        {format!("🎉 现已发布新版本：v{} ({})", info_clone.latest_version, info_clone.release_name)}
                                                                    </div>
                                                                    <div class="text-[11px] text-blue-600 dark:text-blue-400">
                                                                        "点击右侧按钮查看详细更新日志并前往下载安装。"
                                                                    </div>
                                                                </div>
                                                                <button
                                                                    class="px-3 py-1 rounded bg-blue-600 hover:bg-blue-700 text-white font-medium text-xs transition-colors shrink-0 cursor-pointer"
                                                                    on:click=move |_| app_ctx.is_update_modal_open.set(true)
                                                                >
                                                                    "查看更新详情"
                                                                </button>
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! { <div></div> }.into_any()
                                                    }
                                                }}
                                            </div>

                                            // Update Preferences Form
                                            <div class="p-4 rounded-xl border border-slate-200 dark:border-slate-700 space-y-3.5 bg-slate-50/50 dark:bg-slate-900/40">
                                                <h5 class="font-bold text-xs text-slate-800 dark:text-slate-200">"更新选项与远程源配置"</h5>

                                                <div class="flex items-center justify-between">
                                                    <div>
                                                        <label class="font-medium text-xs text-slate-700 dark:text-slate-300 block">"应用启动时自动检测新版本"</label>
                                                        <span class="text-[11px] text-slate-400">"工作台启动后将在后台静默检测，发现更新时以徽标与弹窗提醒"</span>
                                                    </div>
                                                    <input
                                                        type="checkbox"
                                                        class="w-4 h-4 text-blue-600 rounded cursor-pointer"
                                                        prop:checked=move || auto_check_update.get()
                                                        on:change=move |ev| auto_check_update.set(event_target_checked(&ev))
                                                    />
                                                </div>

                                                <div class="space-y-1">
                                                    <label class="font-medium text-xs text-slate-700 dark:text-slate-300 block">"自定义检测更新 API 源 (可选)"</label>
                                                    <input
                                                        type="text"
                                                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500 font-mono text-xs"
                                                        placeholder="默认使用官方 GitHub Releases API (留空即为默认)"
                                                        prop:value=move || custom_update_url.get()
                                                        on:input=move |ev| custom_update_url.set(event_target_value(&ev))
                                                    />
                                                    <p class="text-[10px] text-slate-400">"若在国内网络无法直接访问 GitHub，可配置镜像反代地址。"</p>
                                                </div>

                                                <button
                                                    class="px-3.5 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium text-xs transition-colors cursor-pointer"
                                                    on:click=on_save_update_settings
                                                >
                                                    "保存更新检测偏好"
                                                </button>
                                            </div>

                                            // Links & Project Info
                                            <div class="pt-2 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 border-t border-slate-200 dark:border-slate-700">
                                                <div class="flex items-center gap-3">
                                                    <button
                                                        class="hover:text-blue-600 dark:hover:text-blue-400 underline cursor-pointer"
                                                        on:click=move |_| on_open_external("https://github.com/orderye/Easy-writing".to_string())
                                                    >
                                                        "GitHub 开源仓库"
                                                    </button>
                                                    <span>"·"</span>
                                                    <button
                                                        class="hover:text-blue-600 dark:hover:text-blue-400 underline cursor-pointer"
                                                        on:click=move |_| on_open_external("https://github.com/orderye/Easy-writing/releases".to_string())
                                                    >
                                                        "历史版本 Releases"
                                                    </button>
                                                    <span>"·"</span>
                                                    <button
                                                        class="hover:text-blue-600 dark:hover:text-blue-400 underline cursor-pointer"
                                                        on:click=move |_| on_open_external("https://github.com/orderye/Easy-writing/issues".to_string())
                                                    >
                                                        "问题反馈与建议"
                                                    </button>
                                                </div>
                                                <span class="font-mono text-[10px] text-slate-400">"MIT License"</span>
                                            </div>
                                        </div>
                                    }.into_any(),

                                    _ => view! {
                                        // Default: LLM Profiles
                                        <div class="space-y-6">
                                            // Profile list
                                            <div class="space-y-2">
                                                <h4 class="font-bold text-xs text-slate-700">"已配置的模型接口："</h4>
                                                // 钥匙串缺失提示：换设备后本机通常没有那些密钥，
                                                // 作者看到的应该是「缺少密钥」而不是网络错误。
                                                {move || {
                                                    let missing: Vec<String> = app_ctx
                                                        .missing_keys
                                                        .get()
                                                        .iter()
                                                        .filter(|k| k.scope == "llm_profile")
                                                        .map(|k| k.name.clone())
                                                        .collect();
                                                    if missing.is_empty() {
                                                        return view! { <div></div> }.into_any();
                                                    }
                                                    view! {
                                                        <div class="p-2.5 rounded-md border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-950/40 text-red-800 dark:text-red-200">
                                                            <div class="font-bold">"缺少密钥"</div>
                                                            <div>
                                                                "以下配置在本机钥匙串里找不到 API Key（换设备后常见）："
                                                                {missing.join("、")}
                                                            </div>
                                                            <div class="mt-1">
                                                                "请删除对应配置后重新添加并填入密钥，否则 AI 调用会以「缺少密钥」被拦下。"
                                                            </div>
                                                        </div>
                                                    }.into_any()
                                                }}
                                                <For
                                                    each=move || profiles.get()
                                                    key=|p| p.id.clone()
                                                    children=move |p| {
                                                        let p_id = p.id.clone();
                                                        let p_id_for_test = p_id.clone();
                                                        let p_id_for_del = p_id.clone();
                                                        let p_for_edit = p.clone();
                                                        let p_for_save = p.clone();

                                                        // 该配置已保存的生成参数（PRD §5.1 P0）
                                                        let saved_params = GenerationParams::parse_from_str(&p.params);
                                                        let saved_temp_str = saved_params
                                                            .temperature
                                                            .map(|t| t.to_string())
                                                            .unwrap_or_default();
                                                        let saved_max_str = saved_params
                                                            .max_tokens
                                                            .map(|t| t.to_string())
                                                            .unwrap_or_default();
                                                        let saved_ctx_str = saved_params
                                                            .context_window
                                                            .map(|c| c.to_string())
                                                            .unwrap_or_default();
                                                        let saved_stream = saved_params.stream.unwrap_or(true);
                                                        let is_editing = {
                                                            let id = p_id.clone();
                                                            move || editing_id.get().as_deref() == Some(id.as_str())
                                                        };
                                                        let is_editing_label = is_editing.clone();
                                                        let is_editing_panel = is_editing.clone();

                                                        view! {
                                                            <div class="p-3 border border-slate-200 rounded-lg bg-slate-50 text-xs space-y-2">
                                                                <div class="flex items-center justify-between">
                                                                    <div class="space-y-0.5">
                                                                        <div class="flex items-center gap-2">
                                                                            <span class="font-bold text-slate-800">{p.name.clone()}</span>
                                                                            <span class="text-[10px] bg-blue-100 text-blue-800 px-1.5 py-0.2 rounded font-mono">
                                                                                {p.protocol.clone()}
                                                                            </span>
                                                                            {if p.is_default {
                                                                                view! { <span class="text-[10px] bg-emerald-100 text-emerald-800 px-1.5 py-0.2 rounded">"默认"</span> }.into_any()
                                                                            } else {
                                                                                view! { <span></span> }.into_any()
                                                                            }}
                                                                        </div>
                                                                        <div class="text-slate-400 font-mono text-[11px]">
                                                                            "URL: " {p.base_url.clone()} " | Model: " {p.model.clone()}
                                                                        </div>
                                                                        <div class="text-slate-400 text-[10px]">
                                                                            {gen_params_summary(&p.params)}
                                                                        </div>
                                                                    </div>
                                                                    <div class="flex items-center gap-2">
                                                                        <button
                                                                            class="px-2.5 py-1 bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded text-xs transition-colors cursor-pointer"
                                                                            on:click=move |_| on_test_profile(p_id_for_test.clone())
                                                                        >
                                                                            "测试连接"
                                                                        </button>
                                                                        <button
                                                                            class="px-2.5 py-1 bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded text-xs transition-colors cursor-pointer"
                                                                            on:click={
                                                                                let id = p_id.clone();
                                                                                let t = saved_temp_str.clone();
                                                                                let m = saved_max_str.clone();
                                                                                let c = saved_ctx_str.clone();
                                                                                move |_| {
                                                                                    if editing_id.get().as_deref() == Some(id.as_str()) {
                                                                                        editing_id.set(None);
                                                                                    } else {
                                                                                        edit_temperature.set(t.clone());
                                                                                        edit_max_tokens.set(m.clone());
                                                                                        edit_context_window.set(c.clone());
                                                                                        edit_stream.set(saved_stream);
                                                                                        editing_id.set(Some(id.clone()));
                                                                                    }
                                                                                }
                                                                            }
                                                                        >
                                                                            {move || if is_editing_label() { "收起" } else { "生成参数" }}
                                                                        </button>
                                                                        <button
                                                                            class="px-2.5 py-1 text-red-600 hover:bg-red-50 rounded text-xs transition-colors cursor-pointer"
                                                                            on:click=move |_| on_delete_profile(p_id_for_del.clone())
                                                                        >
                                                                            "删除"
                                                                        </button>
                                                                    </div>
                                                                </div>

                                                                // 生成参数编辑区（PRD §5.1 P0）
                                                                {move || if is_editing_panel() {
                                                                    let p_save = p_for_save.clone();
                                                                    let _ = &p_for_edit;
                                                                    view! {
                                                                        <div class="grid grid-cols-4 gap-3 pt-2 border-t border-slate-200 items-end">
                                                                            <div class="space-y-1">
                                                                                <label class="font-medium text-slate-600">"temperature"</label>
                                                                                <input
                                                                                    type="number"
                                                                                    step="0.1"
                                                                                    min="0"
                                                                                    max="2"
                                                                                    class="w-full p-1.5 border border-slate-300 rounded font-mono"
                                                                                    placeholder="留空用默认"
                                                                                    prop:value=move || edit_temperature.get()
                                                                                    on:input=move |ev| edit_temperature.set(event_target_value(&ev))
                                                                                />
                                                                            </div>
                                                                            <div class="space-y-1">
                                                                                <label class="font-medium text-slate-600">"最大输出长度"</label>
                                                                                <input
                                                                                    type="number"
                                                                                    min="1"
                                                                                    class="w-full p-1.5 border border-slate-300 rounded font-mono"
                                                                                    placeholder="留空用默认"
                                                                                    prop:value=move || edit_max_tokens.get()
                                                                                    on:input=move |ev| edit_max_tokens.set(event_target_value(&ev))
                                                                                />
                                                                            </div>
                                                                            <div class="space-y-1">
                                                                                <label class="font-medium text-slate-600">"上下文窗口 (token)"</label>
                                                                                <input
                                                                                    type="number"
                                                                                    min="1"
                                                                                    class="w-full p-1.5 border border-slate-300 rounded font-mono"
                                                                                    placeholder="留空用默认"
                                                                                    prop:value=move || edit_context_window.get()
                                                                                    on:input=move |ev| edit_context_window.set(event_target_value(&ev))
                                                                                />
                                                                            </div>
                                                                            <div class="flex items-center gap-2 pb-1.5">
                                                                                <input
                                                                                    type="checkbox"
                                                                                    class="w-3.5 h-3.5 text-blue-600 rounded cursor-pointer"
                                                                                    prop:checked=move || edit_stream.get()
                                                                                    on:change=move |ev| edit_stream.set(event_target_checked(&ev))
                                                                                />
                                                                                <label class="font-medium text-slate-600">"启用流式输出"</label>
                                                                                <button
                                                                                    class="ml-auto px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium transition-colors cursor-pointer"
                                                                                    on:click=move |_| on_save_profile_params(p_save.clone())
                                                                                >
                                                                                    "保存参数"
                                                                                </button>
                                                                            </div>
                                                                        </div>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <div></div> }.into_any()
                                                                }}
                                                            </div>
                                                        }
                                                    }
                                                />
                                            </div>

                                            {move || {
                                                let msg = test_status.get();
                                                if msg.is_empty() {
                                                    view! { <div></div> }.into_any()
                                                } else if msg.starts_with('✓') {
                                                    view! {
                                                        <div class="p-2.5 bg-emerald-50 border border-emerald-200 rounded-md text-xs font-semibold text-emerald-800">
                                                            {msg}
                                                        </div>
                                                    }.into_any()
                                                } else if msg.starts_with('✕') {
                                                    view! {
                                                        <div class="p-2.5 bg-red-50 border border-red-200 rounded-md text-xs font-semibold text-red-800">
                                                            {msg}
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="p-2.5 bg-blue-50 border border-blue-200 rounded-md text-xs font-semibold text-blue-800">
                                                            {msg}
                                                        </div>
                                                    }.into_any()
                                                }
                                            }}

                                            // Add new profile form
                                            <div class="border-t border-slate-200 pt-4 space-y-3">
                                                <h4 class="font-bold text-xs text-slate-800">"+ 添加新模型接口配置"</h4>
                                                <div class="grid grid-cols-2 gap-3 text-xs">
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"配置名称"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500"
                                                            placeholder="如：DeepSeek / Claude 3.5 / 本地 Ollama"
                                                            prop:value=move || form_name.get()
                                                            on:input=move |ev| form_name.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"协议规范"</label>
                                                        <select
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 bg-white"
                                                            on:change=move |ev| form_protocol.set(event_target_value(&ev))
                                                        >
                                                            <option value="openai">"OpenAI 兼容协议"</option>
                                                            <option value="anthropic">"Anthropic 兼容协议"</option>
                                                        </select>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"Base URL 接口地址"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="https://api.openai.com/v1"
                                                            prop:value=move || form_base_url.get()
                                                            on:input=move |ev| form_base_url.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"模型名称 (Model)"</label>
                                                        <div class="flex items-center gap-2">
                                                            <input
                                                                type="text"
                                                                class="flex-1 p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                                placeholder="gpt-4o / deepseek-chat / claude-3-5-sonnet"
                                                                prop:value=move || form_model.get()
                                                                on:input=move |ev| form_model.set(event_target_value(&ev))
                                                            />
                                                            <button
                                                                class="px-3 py-2 bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded font-medium whitespace-nowrap transition-colors cursor-pointer disabled:opacity-50"
                                                                title="按上方协议与 Base URL 拉取该接口的可用模型"
                                                                disabled=move || is_fetching_models.get()
                                                                on:click=on_fetch_models
                                                            >
                                                                {move || if is_fetching_models.get() { "拉取中..." } else { "拉取模型列表" }}
                                                            </button>
                                                        </div>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"temperature"</label>
                                                        <input
                                                            type="number"
                                                            step="0.1"
                                                            min="0"
                                                            max="2"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="留空使用接口默认值"
                                                            prop:value=move || form_temperature.get()
                                                            on:input=move |ev| form_temperature.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"最大输出长度 (max_tokens)"</label>
                                                        <input
                                                            type="number"
                                                            min="1"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="留空使用接口默认值"
                                                            prop:value=move || form_max_tokens.get()
                                                            on:input=move |ev| form_max_tokens.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"上下文窗口 (context_window, token)"</label>
                                                        <input
                                                            type="number"
                                                            min="1"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="留空使用接口默认值（决定带入 prompt 的设定预算）"
                                                            prop:value=move || form_context_window.get()
                                                            on:input=move |ev| form_context_window.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="col-span-2 space-y-1">
                                                        <label class="font-medium text-slate-600">"API Key 密钥（保存至系统钥匙串，界面掩码，不随整库上传）"</label>
                                                        <input
                                                            type="password"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="请输入以 sk- 开头的 API Key"
                                                            prop:value=move || form_api_key.get()
                                                            on:input=move |ev| form_api_key.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="col-span-2 flex items-center gap-2">
                                                        <input
                                                            type="checkbox"
                                                            class="w-3.5 h-3.5 text-blue-600 rounded cursor-pointer"
                                                            prop:checked=move || form_stream.get()
                                                            on:change=move |ev| form_stream.set(event_target_checked(&ev))
                                                        />
                                                        <label class="font-medium text-slate-600">"启用流式输出（关闭后一次性返回完整结果）"</label>
                                                    </div>
                                                </div>

                                                // 拉取到的模型候选（PRD §5.1 P0）
                                                {move || {
                                                    let models = remote_models.get();
                                                    if models.is_empty() {
                                                        view! { <div></div> }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="border border-slate-200 rounded-lg bg-white max-h-44 overflow-y-auto">
                                                                <div class="px-3 py-1.5 bg-slate-100 text-[10px] text-slate-500 font-semibold sticky top-0">
                                                                    {format!("可选模型（{} 个，点击即填入模型名称）", models.len())}
                                                                </div>
                                                                <For
                                                                    each=move || remote_models.get()
                                                                    key=|m| m.id.clone()
                                                                    children=move |m| {
                                                                        let model_id = m.id.clone();
                                                                        let owned = m.owned_by.clone();
                                                                        view! {
                                                                            <button
                                                                                class="w-full text-left px-3 py-1.5 hover:bg-blue-50 border-b border-slate-100 last:border-b-0 flex items-center justify-between gap-2 cursor-pointer"
                                                                                on:click=move |_| form_model.set(model_id.clone())
                                                                            >
                                                                                <span class="font-mono text-slate-700 truncate">{m.id.clone()}</span>
                                                                                <span class="text-[10px] text-slate-400 shrink-0">{owned.clone()}</span>
                                                                            </button>
                                                                        }
                                                                    }
                                                                />
                                                            </div>
                                                        }.into_any()
                                                    }
                                                }}

                                                <button
                                                    class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium text-xs shadow-xs transition-colors cursor-pointer"
                                                    on:click=on_add_profile
                                                >
                                                    "保存此接口配置"
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any(),
                                }}
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
