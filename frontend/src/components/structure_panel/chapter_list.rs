use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{Chapter, ChapterHeader};

pub fn do_switch_chapter(app_ctx: AppContext, chapter_id: String) {
    if chapter_id.is_empty() {
        return;
    }
    leptos::task::spawn_local(async move {
        if let Ok(Some(ch)) = invoke_cmd("get_chapter")
            .arg("id", chapter_id)
            .send::<Option<Chapter>>()
            .await
        {
            tiptap::set_content(&ch.content);
            app_ctx.current_chapter.set(Some(ch));
            app_ctx.word_count.set(tiptap::get_word_count());
            app_ctx.save_status.set("saved".to_string());
        }
    });
}

pub fn request_switch_chapter(app_ctx: AppContext, target: ChapterHeader) {
    if let Some(curr) = app_ctx.current_chapter.get() {
        if curr.id == target.id {
            return;
        }
    }
    if app_ctx.save_status.get() == "dirty" {
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
    leptos::task::spawn_local(async move {
        let _ = invoke_cmd("delete_chapter")
            .arg("id", ch_id.clone())
            .send_unit()
            .await;
        let mut list = app_ctx.chapters_map.get();
        list.retain(|c| c.id != ch_id);
        app_ctx.chapters_map.set(list.clone());

        if let Some(curr) = app_ctx.current_chapter.get() {
            if curr.id == ch_id {
                if let Some(next_ch) = list.first() {
                    do_switch_chapter(app_ctx, next_ch.id.clone());
                } else {
                    app_ctx.current_chapter.set(None);
                }
            }
        }
    });
}
