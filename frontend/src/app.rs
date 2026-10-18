use crate::bridge::theme::{apply_app_theme, set_editor_style, setup_theme_listener};
use crate::components::layout::app_shell::AppShell;
use crate::state::AppContext;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use leptos::prelude::*;
use shared_types::{Chapter, ChapterHeader, Novel, Volume};

#[component]
pub fn App() -> impl IntoView {
    let app_ctx = AppContext::new();

    // Startup bootstrap logic
    Effect::new(move |_| {
        setup_theme_listener();

        leptos::task::spawn_local(async move {
            // Load application preferences
            if let Ok(Some(theme_val)) = invoke_cmd("get_app_setting")
                .arg("key", "theme".to_string())
                .send::<Option<String>>()
                .await
            {
                app_ctx.theme.set(theme_val.clone());
                apply_app_theme(&theme_val);
            } else {
                apply_app_theme("system");
            }

            let mut fs_val: u32 = 16;
            if let Ok(Some(fs)) = invoke_cmd("get_app_setting")
                .arg("key", "font_size".to_string())
                .send::<Option<String>>()
                .await
            {
                if let Ok(parsed) = fs.parse::<u32>() {
                    fs_val = parsed;
                    app_ctx.font_size.set(parsed);
                }
            }

            let mut lh_val = "1.8".to_string();
            if let Ok(Some(lh)) = invoke_cmd("get_app_setting")
                .arg("key", "line_height".to_string())
                .send::<Option<String>>()
                .await
            {
                if !lh.trim().is_empty() {
                    lh_val = lh.clone();
                    app_ctx.line_height.set(lh);
                }
            }

            if let Ok(dir) = invoke_no_args::<String>("get_app_data_dir").await {
                crate::bridge::tiptap::set_app_data_dir(&dir);
            }
            set_editor_style(fs_val, &lh_val);

            if let Ok(Some(interval)) = invoke_cmd("get_app_setting")
                .arg("key", "auto_save_interval".to_string())
                .send::<Option<String>>()
                .await
            {
                if let Ok(parsed) = interval.parse::<u32>() {
                    app_ctx.auto_save_interval.set(parsed);
                }
            }

            // Load novels and initial data
            match invoke_no_args::<Vec<Novel>>("list_novels").await {
                Ok(novels) => {
                    if novels.is_empty() {
                        // Create initial novel
                        if let Ok(new_novel) = invoke_cmd("create_novel")
                            .arg("title", "未命名长篇小说".to_string())
                            .arg("synopsis", "".to_string())
                            .send::<Novel>()
                            .await
                        {
                            let novel_id = new_novel.id.clone();
                            app_ctx.current_novel.set(Some(new_novel));

                            // Create initial volume
                            if let Ok(new_vol) = invoke_cmd("create_volume")
                                .arg("novel_id", novel_id.clone())
                                .arg("title", "第一卷：起势".to_string())
                                .arg("summary", "".to_string())
                                .send::<Volume>()
                                .await
                            {
                                let vol_id = new_vol.id.clone();
                                app_ctx.volumes_list.set(vec![new_vol.clone()]);
                                app_ctx.current_volume.set(Some(new_vol));

                                // Create initial chapter
                                if let Ok(new_ch) = invoke_cmd("create_chapter")
                                    .arg("novel_id", novel_id)
                                    .arg("volume_id", vol_id.clone())
                                    .arg("title", "第一章：初现端倪".to_string())
                                    .arg("summary", "".to_string())
                                    .send::<Chapter>()
                                    .await
                                {
                                    app_ctx.chapters_map.set(vec![ChapterHeader {
                                        id: new_ch.id.clone(),
                                        novel_id: new_ch.novel_id.clone(),
                                        volume_id: new_ch.volume_id.clone(),
                                        title: new_ch.title.clone(),
                                        summary: new_ch.summary.clone(),
                                        word_count: 0,
                                        status: new_ch.status.clone(),
                                        sort_order: new_ch.sort_order,
                                        updated_at: new_ch.updated_at.clone(),
                                    }]);
                                    app_ctx.current_chapter.set(Some(new_ch));
                                }
                            }
                        }
                    } else {
                        // Load first novel
                        let first_novel = novels[0].clone();
                        let novel_id = first_novel.id.clone();
                        app_ctx.novels_list.set(novels);
                        app_ctx.current_novel.set(Some(first_novel));

                        // Load volumes
                        if let Ok(mut vols) = invoke_cmd("list_volumes")
                            .arg("novel_id", novel_id.clone())
                            .send::<Vec<Volume>>()
                            .await
                        {
                            if vols.is_empty() {
                                // Create initial volume if none exists
                                if let Ok(new_vol) = invoke_cmd("create_volume")
                                    .arg("novel_id", novel_id.clone())
                                    .arg("title", "第一卷：起势".to_string())
                                    .arg("summary", "".to_string())
                                    .send::<Volume>()
                                    .await
                                {
                                    vols.push(new_vol);
                                }
                            }

                            if !vols.is_empty() {
                                let first_vol = vols[0].clone();
                                let vol_id = first_vol.id.clone();
                                app_ctx.volumes_list.set(vols);
                                app_ctx.current_volume.set(Some(first_vol));

                                // Load chapters
                                if let Ok(mut chs) = invoke_cmd("list_chapter_headers")
                                    .arg("volume_id", vol_id.clone())
                                    .send::<Vec<ChapterHeader>>()
                                    .await
                                {
                                    if chs.is_empty() {
                                        // Create initial chapter if none exists
                                        if let Ok(new_ch) = invoke_cmd("create_chapter")
                                            .arg("novel_id", novel_id.clone())
                                            .arg("volume_id", vol_id)
                                            .arg("title", "第一章：初现端倪".to_string())
                                            .arg("summary", "".to_string())
                                            .send::<Chapter>()
                                            .await
                                        {
                                            chs.push(ChapterHeader {
                                                id: new_ch.id.clone(),
                                                novel_id: new_ch.novel_id.clone(),
                                                volume_id: new_ch.volume_id.clone(),
                                                title: new_ch.title.clone(),
                                                summary: new_ch.summary.clone(),
                                                word_count: 0,
                                                status: new_ch.status.clone(),
                                                sort_order: new_ch.sort_order,
                                                updated_at: new_ch.updated_at.clone(),
                                            });
                                            app_ctx.current_chapter.set(Some(new_ch));
                                        }
                                    }

                                    if !chs.is_empty() {
                                        let first_ch_header = chs[0].clone();
                                        app_ctx.chapters_map.set(chs);

                                        // Load full chapter
                                        if let Ok(Some(full_ch)) = invoke_cmd("get_chapter")
                                            .arg("id", first_ch_header.id)
                                            .send::<Option<Chapter>>()
                                            .await
                                        {
                                            crate::bridge::tiptap::set_content(&full_ch.content);
                                            app_ctx.word_count.set(full_ch.word_count as u32);
                                            app_ctx.current_chapter.set(Some(full_ch));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    web_sys::console::warn_1(
                        &format!("Running outside of Tauri or failed to list novels: {}", err)
                            .into(),
                    );
                }
            }
        });
    });

    view! {
        <AppShell app_ctx=app_ctx />
    }
}
