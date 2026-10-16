//! 作品工作区的装载逻辑。
//!
//! 原先这段逻辑内联在 `app.rs` 的启动 Effect 里，只能跑一次。
//! 「切换作品」需要把同样的装载流程再跑一遍（换 novel → 载卷 → 载章 → 设置正文），
//! 因此提取为可复用函数，避免在 app.rs 里再抄一份。
//!
//! 涉及的命令与参数键名均与后端一致。

use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{Chapter, ChapterHeader, Novel, Volume};

/// 把一个作品装载为当前工作区：载入其卷、章节并选中第一卷第一章。
///
/// `ensure_content` 为真时，若该作品没有任何卷/章则自动创建初始卷章
/// （沿用启动时的行为，避免用户面对空白工作台）；切换作品时同样适用。
pub async fn load_novel_workspace(app_ctx: AppContext, novel: Novel, ensure_content: bool) {
    let novel_id = novel.id.clone();
    app_ctx.current_novel.set(Some(novel));

    // 载入卷
    let mut vols = invoke_cmd("list_volumes")
        .arg("novel_id", novel_id.clone())
        .send::<Vec<Volume>>()
        .await
        .unwrap_or_default();

    if vols.is_empty() && ensure_content {
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

    if vols.is_empty() {
        // 无卷：清空下游状态，避免残留上一个作品的章节
        app_ctx.volumes_list.set(Vec::new());
        app_ctx.current_volume.set(None);
        app_ctx.chapters_map.set(Vec::new());
        app_ctx.current_chapter.set(None);
        app_ctx.word_count.set(0);
        return;
    }

    let first_vol = vols[0].clone();
    let vol_id = first_vol.id.clone();
    app_ctx.volumes_list.set(vols);
    app_ctx.current_volume.set(Some(first_vol));

    // 载入该卷的章节
    let mut chs = invoke_cmd("list_chapter_headers")
        .arg("volume_id", vol_id.clone())
        .send::<Vec<ChapterHeader>>()
        .await
        .unwrap_or_default();

    if chs.is_empty() && ensure_content {
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
            app_ctx.current_chapter.set(Some(new_ch.clone()));
            app_ctx.chapters_map.set(chs);
            // 新建章节内容为空，直接装载空文档
            crate::bridge::tiptap::set_content(&new_ch.content);
            app_ctx.word_count.set(0);
            app_ctx.save_status.set("saved".to_string());
            return;
        }
    }

    if chs.is_empty() {
        app_ctx.chapters_map.set(Vec::new());
        app_ctx.current_chapter.set(None);
        app_ctx.word_count.set(0);
        return;
    }

    let first_header = chs[0].clone();
    app_ctx.chapters_map.set(chs);

    // 载入第一章正文
    if let Ok(Some(full_ch)) = invoke_cmd("get_chapter")
        .arg("id", first_header.id)
        .send::<Option<Chapter>>()
        .await
    {
        crate::bridge::tiptap::set_content(&full_ch.content);
        app_ctx.word_count.set(full_ch.word_count as u32);
        app_ctx.save_status.set("saved".to_string());
        app_ctx.current_chapter.set(Some(full_ch));
    }
}

/// 重新拉取作品列表并写入 `novels_list`。
pub async fn refresh_novels_list(app_ctx: AppContext) -> Vec<Novel> {
    let novels = crate::tauri_api::invoke_no_args::<Vec<Novel>>("list_novels")
        .await
        .unwrap_or_default();
    app_ctx.novels_list.set(novels.clone());
    novels
}
