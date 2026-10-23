use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{ContextManifest, ContextManifestEntry};

/// 条目分组（entity_type → 中文名与展示顺序）。
const GROUP_ORDER: [(&str, &str); 6] = [
    ("novel", "作品信息"),
    ("chapter", "章节"),
    ("character", "角色"),
    ("plot", "剧情伏笔"),
    ("worldview", "世界观"),
    ("lexicon", "词库术语"),
];

/// 入选原因的展示文案（与后端 manifest 的 reason 枚举一一对应）。
fn reason_label(reason: &str) -> &'static str {
    match reason {
        "core" => "核心",
        "recent_summary" => "近期摘要",
        "pinned" => "恒入",
        "presence" => "本名命中",
        "alias" => "别名命中",
        "active_plot" => "待回收伏笔",
        "lexicon" => "词库命中",
        "worldview" => "世界观",
        "listed" => "仅名单",
        _ => "其他",
    }
}

/// 「本次上下文」透明面板（任务书 T06）：
/// 逐条目展示本次对话会带入哪些设定、为什么带入、估算多少 token；
/// 取消勾选可把条目移出本次装配（请求级，发送后清空），
/// 📌 可把条目设为恒入（pinned），刷新后即生效。
#[component]
pub fn ContextPanel(
    app_ctx: AppContext,
    /// 用户取消勾选的条目 id（请求级：发送成功后由调用方清空）。
    excluded: RwSignal<Vec<String>>,
    /// 发送完成后自增，触发清单重载。
    refresh_tick: RwSignal<u64>,
) -> impl IntoView {
    let manifest = RwSignal::new(Option::<ContextManifest>::None);
    let is_expanded = RwSignal::new(false);
    let is_loading = RwSignal::new(false);

    let load_manifest = move || {
        let novel = app_ctx.current_novel.get();
        let novel_id = match novel {
            Some(n) => n.id,
            None => return,
        };
        let chapter_id = app_ctx.current_chapter.get().map(|c| c.id);
        is_loading.set(true);
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("get_context_manifest")
                .arg("novel_id", novel_id)
                .opt("chapter_id", chapter_id)
                .opt("prompt", None::<String>)
                .send::<ContextManifest>()
                .await;
            match res {
                Ok(m) => manifest.set(Some(m)),
                Err(_) => manifest.set(None),
            }
            is_loading.set(false);
        });
    };

    // 首次展开 / 发送完成 / pin 切换后重载清单
    Effect::new(move |_| {
        if is_expanded.get() {
            let _ = refresh_tick.get();
            load_manifest();
        }
    });

    let on_toggle_exclude = move |entry: ContextManifestEntry| {
        let id = entry.entry_id.clone();
        excluded.update(|list| {
            if list.iter().any(|x| *x == id) {
                list.retain(|x| *x != id);
            } else {
                list.push(id);
            }
        });
    };

    let on_toggle_pin = move |entry: ContextManifestEntry| {
        let entity_type = entry.entity_type.clone();
        let entry_id = entry.entry_id.clone();
        let now_pinned = entry.reason == "pinned";
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("set_entry_pinned")
                .arg("entity_type", entity_type)
                .arg("entry_id", entry_id)
                .arg("pinned", !now_pinned)
                .send_unit()
                .await;
            load_manifest();
        });
    };

    view! {
        <div class="border-b border-slate-200 bg-slate-50/80">
            // 折叠条：已用 / 预算 token 与入选条数一目了然
            <div
                class="h-7 px-4 flex items-center justify-between cursor-pointer select-none text-[11px] text-slate-600 hover:bg-slate-100/70"
                on:click=move |_| is_expanded.update(|v| *v = !*v)
            >
                <div class="flex items-center gap-2 font-medium">
                    <span>"🧩"</span>
                    <span>"本次上下文"</span>
                    {move || match manifest.get() {
                        Some(m) => {
                            let over = m.used_tokens > m.budget_tokens;
                            view! {
                                <span class=format!(
                                    "font-mono text-[10px] {}",
                                    if over { "text-amber-600 font-bold" } else { "text-slate-400" }
                                )>
                                    {format!("已用 {}/{} token", m.used_tokens, m.budget_tokens)}
                                </span>
                                <span class="text-[10px] text-slate-400">
                                    {format!("{} 条入选 · {} 条被裁掉", m.entries.len(), m.trimmed.len())}
                                </span>
                            }.into_any()
                        }
                        None => view! {
                            <span class="text-[10px] text-slate-400">"展开后查看本次装配明细"</span>
                        }.into_any(),
                    }}
                </div>
                <span class="text-slate-400">
                    {move || if is_expanded.get() { "▾ 收起" } else { "▸ 展开" }}
                </span>
            </div>

            // 明细区
            {move || {
                if !is_expanded.get() {
                    return view! { <div></div> }.into_any();
                }
                let m = match manifest.get() {
                    Some(m) => m,
                    None => {
                        return view! {
                            <div class="px-4 py-2 text-[11px] text-slate-400">
                                {if is_loading.get() { "正在装配上下文..." } else { "暂无可用上下文（未选择作品）" }}
                            </div>
                        }.into_any();
                    }
                };

                let groups: Vec<(&str, String, Vec<ContextManifestEntry>)> = GROUP_ORDER
                    .iter()
                    .map(|(et, label)| {
                        let entries: Vec<ContextManifestEntry> = m
                            .entries
                            .iter()
                            .filter(|e| e.entity_type == *et)
                            .cloned()
                            .collect();
                        (*et, (*label).to_string(), entries)
                    })
                    .filter(|(_, _, v)| !v.is_empty())
                    .collect();

                view! {
                    <div class="max-h-56 overflow-y-auto px-3 py-2 space-y-2">
                        <For
                            each=move || groups.clone()
                            key=|(et, _, _)| et.to_string()
                            children=move |(_, label, entries)| {
                                let group_label = label.clone();
                                view! {
                                    <div>
                                        <div class="text-[10px] font-bold text-slate-500 mb-0.5">{group_label}</div>
                                        <For
                                            each=move || entries.clone()
                                            key=|e| e.entry_id.clone()
                                            children=move |e| {
                                                // 同一个条目要喂给多个闭包（勾选态、排除、pin），
                                                // 直接 move 整体会让后用的那处拿到已移走的值；
                                                // 因此拆成可独立 move 的局部量。
                                                let entry_id = e.entry_id.clone();
                                                let included = e.included;
                                                let entry_for_exclude = e.clone();
                                                let entry_for_pin = e.clone();
                                                let is_excluded = move || {
                                                    let id = entry_id.clone();
                                                    excluded.get().iter().any(|x| *x == id)
                                                };
                                                let row_cls = if included { "" } else { "opacity-40" };
                                                view! {
                                                    <div class=format!("flex items-center gap-1.5 py-0.5 {}", row_cls)>
                                                        <input
                                                            type="checkbox"
                                                            class="w-3 h-3 text-blue-600 rounded cursor-pointer"
                                                            prop:checked=move || included && !is_excluded()
                                                            disabled=move || !included
                                                            on:change=move |_| on_toggle_exclude(entry_for_exclude.clone())
                                                        />
                                                        <span class="text-[11px] text-slate-700 truncate max-w-[40%]">{e.title.clone()}</span>
                                                        <span class="text-[9px] px-1 py-px rounded bg-slate-200 text-slate-600 font-mono">
                                                            {reason_label(&e.reason)}
                                                        </span>
                                                        <span class="text-[9px] text-slate-400 font-mono">
                                                            {format!("~{} tok", e.token_count)}
                                                        </span>
                                                        {if !included {
                                                            view! {
                                                                <span class="text-[9px] text-amber-600">"预算不足已裁掉"</span>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span></span> }.into_any()
                                                        }}
                                                        <button
                                                            class=format!(
                                                                "ml-auto text-[11px] px-1 rounded cursor-pointer shrink-0 {}",
                                                                if e.reason == "pinned" { "text-amber-500" } else { "text-slate-300 hover:text-slate-500" }
                                                            )
                                                            title="设为恒入（始终带入上下文）"
                                                            on:click=move |_| on_toggle_pin(entry_for_pin.clone())
                                                        >
                                                            "📌"
                                                        </button>
                                                    </div>
                                                }
                                            }
                                        />
                                    </div>
                                }.into_any()
                            }
                        />
                        // 被预算裁掉的条目（只读，不参与勾选）
                        {move || {
                            let trimmed = m.trimmed.clone();
                            if trimmed.is_empty() {
                                view! { <div></div> }.into_any()
                            } else {
                                view! {
                                    <div class="pt-1 border-t border-slate-200">
                                        <div class="text-[10px] font-bold text-amber-600 mb-0.5">"被预算裁掉（未进入本次发送）"</div>
                                        <For
                                            each=move || trimmed.clone()
                                            key=|e| e.entry_id.clone()
                                            children=move |e| {
                                                view! {
                                                    <div class="flex items-center gap-1.5 py-0.5 opacity-50">
                                                        <span class="text-[11px] text-slate-600 line-through truncate">{e.title.clone()}</span>
                                                        <span class="text-[9px] px-1 py-px rounded bg-slate-200 text-slate-600 font-mono">{reason_label(&e.reason)}</span>
                                                    </div>
                                                }
                                            }
                                        />
                                    </div>
                                }.into_any()
                            }
                        }}
                    </div>
                }.into_any()
            }}
        </div>
    }
}
