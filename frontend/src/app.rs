use crate::components::layout::app_shell::AppShell;
use crate::state::AppContext;
use crate::tauri_api::{invoke, invoke_no_args};
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{Chapter, ChapterHeader, Novel, Volume};

#[derive(Serialize)]
struct CreateNovelArgs {
    title: String,
    synopsis: String,
}

#[derive(Serialize)]
struct CreateVolumeArgs {
    novel_id: String,
    title: String,
    summary: String,
}

#[derive(Serialize)]
struct CreateChapterArgs {
    novel_id: String,
    volume_id: String,
    title: String,
    summary: String,
}

#[derive(Serialize)]
struct ListVolumesArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct ListChaptersArgs {
    volume_id: String,
}

#[derive(Serialize)]
struct GetChapterArgs {
    id: String,
}

#[component]
pub fn App() -> impl IntoView {
    let app_ctx = AppContext::new();

    // Startup bootstrap logic
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            match invoke_no_args::<Vec<Novel>>("list_novels").await {
                Ok(novels) => {
                    if novels.is_empty() {
                        // Create initial novel
                        if let Ok(new_novel) = invoke::<Novel, _>(
                            "create_novel",
                            &CreateNovelArgs {
                                title: "未命名长篇小说".to_string(),
                                synopsis: "".to_string(),
                            },
                        )
                        .await
                        {
                            let novel_id = new_novel.id.clone();
                            app_ctx.current_novel.set(Some(new_novel));

                            // Create initial volume
                            if let Ok(new_vol) = invoke::<Volume, _>(
                                "create_volume",
                                &CreateVolumeArgs {
                                    novel_id: novel_id.clone(),
                                    title: "第一卷：起势".to_string(),
                                    summary: "".to_string(),
                                },
                            )
                            .await
                            {
                                let vol_id = new_vol.id.clone();
                                app_ctx.volumes_list.set(vec![new_vol.clone()]);
                                app_ctx.current_volume.set(Some(new_vol));

                                // Create initial chapter
                                if let Ok(new_ch) = invoke::<Chapter, _>(
                                    "create_chapter",
                                    &CreateChapterArgs {
                                        novel_id,
                                        volume_id: vol_id.clone(),
                                        title: "第一章：初现端倪".to_string(),
                                        summary: "".to_string(),
                                    },
                                )
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
                        if let Ok(mut vols) =
                            invoke::<Vec<Volume>, _>("list_volumes", &ListVolumesArgs { novel_id: novel_id.clone() })
                                .await
                        {
                            if vols.is_empty() {
                                // Create initial volume if none exists
                                if let Ok(new_vol) = invoke::<Volume, _>(
                                    "create_volume",
                                    &CreateVolumeArgs {
                                        novel_id: novel_id.clone(),
                                        title: "第一卷：起势".to_string(),
                                        summary: "".to_string(),
                                    },
                                )
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
                                if let Ok(mut chs) =
                                    invoke::<Vec<ChapterHeader>, _>("list_chapter_headers", &ListChaptersArgs { volume_id: vol_id.clone() })
                                        .await
                                {
                                    if chs.is_empty() {
                                        // Create initial chapter if none exists
                                        if let Ok(new_ch) = invoke::<Chapter, _>(
                                            "create_chapter",
                                            &CreateChapterArgs {
                                                novel_id: novel_id.clone(),
                                                volume_id: vol_id,
                                                title: "第一章：初现端倪".to_string(),
                                                summary: "".to_string(),
                                            },
                                        )
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
                                        if let Ok(Some(full_ch)) =
                                            invoke::<Option<Chapter>, _>("get_chapter", &GetChapterArgs { id: first_ch_header.id }).await
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
                    web_sys::console::warn_1(&format!("Running outside of Tauri or failed to list novels: {}", err).into());
                }
            }
        });
    });

    view! {
        <AppShell app_ctx=app_ctx />
    }
}
