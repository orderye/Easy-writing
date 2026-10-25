use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{Chapter, ChapterHeader};

pub fn do_switch_chapter(app_ctx: AppContext, chapter_id: String) {
    if chapter_id.is_empty() {
        return;
    }
    // 立即锁定旧编辑器，防止在读取目标章节期间用户键入新内容被覆盖
    tiptap::set_editable(false);
    let start_seq = app_ctx.edit_seq.get();

    leptos::task::spawn_local(async move {
        let res = invoke_cmd("get_chapter")
            .arg("id", chapter_id)
            .send::<Option<Chapter>>()
            .await;

        match res {
            Ok(Some(ch)) => {
                let current_seq = app_ctx.edit_seq.get();
                let current_status = app_ctx.save_status.get();
                match crate::save_state::check_switch_safety(start_seq, current_seq, &current_status) {
                    crate::save_state::SwitchSafetyOutcome::SafeToSwitch => {
                        if !tiptap::set_content(&ch.content) {
                            // 编辑器尚未挂载（返回 false 且什么都没做）：记下正文，
                            // 等 EditorWrapper 挂载完成后回放。不这么做的话用户会看到空白编辑区，
                            // 而正文其实好端端躺在库里。
                            app_ctx.pending_editor_content.set(Some(ch.content.clone()));
                        }
                        app_ctx.current_chapter.set(Some(ch));
                        app_ctx.word_count.set(tiptap::get_word_count());
                        app_ctx.save_status.set("saved".to_string());
                        // 目标章节装载完毕后恢复可编辑
                        tiptap::set_editable(true);
                    }
                    crate::save_state::SwitchSafetyOutcome::AbortedDueToNewEdits
                    | crate::save_state::SwitchSafetyOutcome::AbortedDueToUnsaved => {
                        // 竞态保护生效：等待期间产生了新输入或正文未保存，恢复可编辑并提示用户
                        tiptap::set_editable(true);
                        app_ctx.save_alert.set(Some(
                            "切章已拦截：检测到正文在此期间产生新变动，请先保存当前修改。".to_string(),
                        ));
                    }
                }
            }
            Ok(None) => {
                tiptap::set_editable(true);
                app_ctx.save_alert.set(Some("目标章节不存在".to_string()));
            }
            Err(e) => {
                tiptap::set_editable(true);
                web_sys::console::error_1(&format!("读取目标章节失败: {}", e).into());
                app_ctx.save_alert.set(Some(format!("读取目标章节失败：{}", e)));
            }
        }
    });
}

pub fn request_switch_chapter(app_ctx: AppContext, target: ChapterHeader) {
    if let Some(curr) = app_ctx.current_chapter.get() {
        if curr.id == target.id {
            return;
        }
    }
    let status = app_ctx.save_status.get();
    // 任何非 saved 状态（未保存 dirty、保存失败 error、保存中 saving）都必须拦截，严防丢稿！
    if status != "saved" {
        app_ctx.pending_switch_chapter.set(Some(target));
        app_ctx.is_unsaved_modal_open.set(true);
    } else {
        do_switch_chapter(app_ctx, target.id);
    }
}

pub fn create_chapter_in_volume(app_ctx: AppContext, volume_id: String) {
    if let Some(novel) = app_ctx.current_novel.get() {
        let novel_id = novel.id;
        let v_id = volume_id.clone();
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("create_chapter")
                .arg("novel_id", novel_id)
                .arg("volume_id", v_id)
                .arg("title", "新章节".to_string())
                .arg("summary", "".to_string())
                .send::<Chapter>()
                .await;

            if let Ok(new_ch) = res {
                let mut list = app_ctx.chapters_map.get();
                let new_header = ChapterHeader {
                    id: new_ch.id.clone(),
                    novel_id: new_ch.novel_id.clone(),
                    volume_id: new_ch.volume_id.clone(),
                    title: new_ch.title.clone(),
                    summary: new_ch.summary.clone(),
                    word_count: 0,
                    status: new_ch.status.clone(),
                    sort_order: new_ch.sort_order,
                    updated_at: new_ch.updated_at.clone(),
                    pov_character_id: None,
                    story_time: None,
                };
                list.push(new_header.clone());
                app_ctx.chapters_map.set(list);

                // Switch to new chapter (checking if current chapter is dirty)
                request_switch_chapter(app_ctx, new_header);
            }
        });
    }
}

pub fn delete_chapter_by_id(app_ctx: AppContext, ch_id: String) {
    if ch_id.is_empty() {
        return;
    }
    // 若被删除的正是当前章节且正文存在未保存修改，拦截并提醒用户保存或放弃
    if let Some(curr) = app_ctx.current_chapter.get() {
        if curr.id == ch_id && app_ctx.save_status.get() != "saved" {
            app_ctx.save_alert.set(Some(
                "当前章节有未保存的修改，请先保存或放弃修改后再删除。".to_string(),
            ));
            return;
        }
    }

    leptos::task::spawn_local(async move {
        let res = invoke_cmd("delete_chapter")
            .arg("id", ch_id.clone())
            .send_unit()
            .await;

        match res {
            Ok(()) => {
                let mut list = app_ctx.chapters_map.get();
                list.retain(|c| c.id != ch_id);
                app_ctx.chapters_map.set(list.clone());

                if let Some(curr) = app_ctx.current_chapter.get() {
                    if curr.id == ch_id {
                        if let Some(next_ch) = list.first() {
                            do_switch_chapter(app_ctx, next_ch.id.clone());
                        } else {
                            app_ctx.current_chapter.set(None);
                            tiptap::set_content("");
                            tiptap::set_editable(false);
                            app_ctx.word_count.set(0);
                            app_ctx.save_status.set("saved".to_string());
                        }
                    }
                }

            }
            Err(e) => {
                web_sys::console::error_1(&format!("删除章节失败: {}", e).into());
                app_ctx.save_alert.set(Some(format!("删除章节失败：{}", e)));
            }
        }
    });
}
