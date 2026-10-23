use leptos::prelude::*;
use shared_types::{Chapter, ConflictInfo};

/// 设定冲突裁决卡（T08）。
///
/// 与旧版相比多三件事：
///   1. 展示**判定依据**：关联设定（可定位到库内条目）、证据原句、置信度；
///   2. 「本章忽略此类提示」—— 只对当前章节生效，签名来自后端；
///   3. 置信度低于阈值的结果根本不会走到这里（后端返回 `Suppressed`），
///      所以卡片上的数字是作者真正需要裁决的那部分。
#[component]
pub fn ConflictModal<F1, F2, F3, F4>(
    is_open: RwSignal<bool>,
    conflict_info: RwSignal<Option<ConflictInfo>>,
    /// 仅用于判断「本章忽略」按钮是否可用：没有打开的章节就没有「本章」。
    chapter: RwSignal<Option<Chapter>>,
    on_update_setting_and_continue: F1,
    on_rewrite_without_conflict: F2,
    on_force_apply: F3,
    on_ignore_this_kind: F4,
) -> impl IntoView
where
    F1: Fn(String) + Send + Sync + Copy + 'static,
    F2: Fn() + Send + Sync + Copy + 'static,
    F3: Fn() + Send + Sync + Copy + 'static,
    F4: Fn() + Send + Sync + Copy + 'static,
{
    view! {
        {move || {
            if is_open.get() {
                if let Some(info) = conflict_info.get() {
                    let proposed_change = RwSignal::new(info.proposed_change.clone().unwrap_or_default());
                    let related = info.related_settings.clone();
                    let has_related = !related.is_empty();
                    let evidence = info.evidence.trim().to_string();
                    let confidence = info.confidence;
                    let can_ignore = !info.ignore_signature.trim().is_empty();
                    let ignore_label = info.ignore_label.clone();
                    let confidence_label = match confidence {
                        Some(c) => format!("{:.0}%", (c * 100.0).clamp(0.0, 100.0)),
                        None => "模型未给出置信度".to_string(),
                    };
                    let confidence_width = confidence
                        .map(|c| format!("width: {:.0}%", (c * 100.0).clamp(0.0, 100.0)))
                        .unwrap_or_else(|| "width: 0%".to_string());

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

                                    <div>
                                        <span class="font-bold text-slate-900 block mb-1">"【关联设定】"</span>
                                        <div class="flex flex-wrap gap-1.5">
                                            {if has_related {
                                                related
                                                    .into_iter()
                                                    .map(|r| {
                                                        let label = match r.entity_type.as_str() {
                                                            "worldview" => "世界观",
                                                            "character" => "角色",
                                                            "plot" => "剧情",
                                                            _ => "设定",
                                                        };
                                                        view! {
                                                            <span
                                                                class="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-slate-100 border border-slate-200 text-slate-700"
                                                                title=format!("{}#{}", r.entity_type, r.id)
                                                            >
                                                                <span class="text-[10px] text-slate-400">{label}</span>
                                                                <span class="font-semibold">{r.title.clone()}</span>
                                                            </span>
                                                        }
                                                    })
                                                    .collect_view()
                                                    .into_any()
                                            } else {
                                                view! {
                                                    <span class="text-slate-500">
                                                        {format!("{} · {}（未能定位到具体条目）", info.setting_module, info.setting_title)}
                                                    </span>
                                                }
                                                    .into_any()
                                            }}
                                        </div>
                                    </div>

                                    <div>
                                        <span class="font-bold text-slate-900 block mb-1">"【证据】"</span>
                                        <p class="p-2.5 bg-amber-50/60 border border-amber-200 rounded-lg text-amber-900 leading-relaxed whitespace-pre-wrap">
                                            {if evidence.is_empty() {
                                                "（模型未摘录引用原句，请结合下方原因判断）".to_string()
                                            } else {
                                                evidence.clone()
                                            }}
                                        </p>
                                    </div>

                                    <div>
                                        <div class="flex items-center justify-between mb-1">
                                            <span class="font-bold text-slate-900">"【置信度】"</span>
                                            <span class="font-mono text-slate-600">{confidence_label}</span>
                                        </div>
                                        <div class="h-1.5 w-full bg-slate-200 rounded-full overflow-hidden">
                                            <div class="h-full bg-red-500 rounded-full" style=confidence_width></div>
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
                                <div class="px-5 py-3 border-t border-slate-200 bg-slate-50 flex flex-wrap items-center justify-end gap-2.5">
                                    <button
                                        class="px-3 py-1.5 text-xs text-slate-600 hover:bg-slate-200 rounded-md font-medium transition-colors"
                                        on:click=move |_| is_open.set(false)
                                    >
                                        "放弃写入"
                                    </button>
                                    <button
                                        class="px-3 py-1.5 text-xs bg-amber-100 hover:bg-amber-200 text-amber-900 rounded-md font-semibold transition-colors"
                                        on:click=move |_| {
                                            on_rewrite_without_conflict();
                                            is_open.set(false);
                                        }
                                    >
                                        "换一种不冲突写法"
                                    </button>
                                    <button
                                        class="px-3 py-1.5 text-xs bg-slate-200 hover:bg-slate-300 text-slate-800 rounded-md font-semibold transition-colors"
                                        title="忽略冲突警告，保留既有设定不变，直接将生成的文字插入正文"
                                        on:click=move |_| {
                                            on_force_apply();
                                            is_open.set(false);
                                        }
                                    >
                                        "忽略警告并写入正文"
                                    </button>
                                    {move || {
                                        if can_ignore && chapter.get().is_some() {
                                            let title = format!("本章不再提示：{}", ignore_label);
                                            view! {
                                                <button
                                                    class="px-3 py-1.5 text-xs bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded-md font-semibold transition-colors"
                                                    title=title
                                                    on:click=move |_| {
                                                        on_ignore_this_kind();
                                                        is_open.set(false);
                                                    }
                                                >
                                                    "本章忽略此类提示"
                                                </button>
                                            }
                                                .into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }
                                    }}
                                    <button
                                        class="px-3.5 py-1.5 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded-md font-semibold shadow-xs transition-colors"
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