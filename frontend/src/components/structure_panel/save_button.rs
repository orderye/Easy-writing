use crate::bridge::tiptap;
use crate::save_state;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;

/// 同步触发一次保存。按钮、⌘S、"保存并切换"都走这里。
pub fn perform_save(app_ctx: AppContext) {
    // 丢弃返回值：调用方（按钮 / ⌘S / 确认弹窗）不关心成败，
    // 状态已经由 perform_save_async 写进 save_status / save_alert。
    leptos::task::spawn_local(async move {
        let _ = perform_save_async(app_ctx).await;
    });
}

/// 执行一次保存并**等待其真正结束**，返回是否成功。
///
/// 之所以要有 async 版本：后台自动保存必须等到保存结束才知道成败，
/// 不能用 `sleep` 猜（请求慢于猜测阈值就会被误判为失败并触发退避）。
///
/// 状态判定交给 `save_state::resolve_save_status`：若保存期间用户又输入了内容
/// （`edit_seq` 已变），保存成功也不能标记为 "saved" —— 否则切章逻辑看到
/// "saved" 就直接切走，那段输入会被静默丢弃。
pub async fn perform_save_async(app_ctx: AppContext) -> save_state::SaveOutcome {
    let Some(mut chapter) = app_ctx.current_chapter.get_untracked() else {
        // 没有当前章节，视为无事发生（不算失败，避免自动保存误报）
        return save_state::SaveOutcome::Saved;
    };

    let seq_at_save = app_ctx.edit_seq.get_untracked();
    app_ctx.save_status.set("saving".to_string());

    let content_json = tiptap::get_content_json();
    let content_plain = tiptap::get_content_plain();
    let word_count = tiptap::get_word_count() as i32;

    chapter.content = content_json.clone();
    chapter.content_plain = content_plain.clone();
    chapter.word_count = word_count;
    let chapter_id = chapter.id.clone();
    app_ctx.current_chapter.set(Some(chapter));
    app_ctx.word_count.set(word_count as u32);

    let res = invoke_cmd("save_chapter_content")
        .arg("id", chapter_id)
        .arg("content", content_json)
        .arg("content_plain", content_plain)
        .arg("word_count", word_count)
        .send_unit()
        .await;

    let succeeded = res.is_ok();
    if let Err(err) = &res {
        web_sys::console::error_1(&format!("Failed to save chapter: {}", err).into());
    }

    let edited_since = app_ctx.edit_seq.get_untracked() != seq_at_save;
    let outcome = save_state::resolve_save_outcome(succeeded, edited_since);
    app_ctx
        .save_status
        .set(outcome.as_status_str().to_string());

    outcome
}

#[component]
pub fn SaveButton(app_ctx: AppContext) -> impl IntoView {
    let save_status = move || app_ctx.save_status.get();
    let is_dirty = move || save_status() == "dirty";

    view! {
        <button
            class=move || {
                format!(
                    "w-full py-2 px-3 rounded-md text-sm font-semibold flex items-center justify-center gap-2 shadow-sm transition-all {}",
                    if is_dirty() {
                        "bg-blue-600 hover:bg-blue-700 text-white animate-pulse"
                    } else if save_status() == "saving" {
                        "bg-slate-200 text-slate-500 cursor-wait"
                    } else if save_status() == "error" {
                        "bg-red-500 text-white"
                    } else {
                        "bg-slate-100 hover:bg-slate-200 text-slate-700 border border-slate-300"
                    }
                )
            }
            on:click=move |_| perform_save(app_ctx)
        >
            <span>
                {move || match save_status().as_str() {
                    "dirty" => "💾 保存当前章节 *",
                    "saving" => "⏳ 正在保存...",
                    "error" => "⚠️ 保存失败 (点击重试)",
                    _ => "✓ 章节已保存",
                }}
            </span>
            <span class="text-xs opacity-75 font-normal">"(⌘S)"</span>
        </button>
    }
}
