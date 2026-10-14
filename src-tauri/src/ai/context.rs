use rusqlite::Connection;
use shared_types::Novel;

pub struct AssembledContext {
    pub system_prompt: String,
    pub context_text: String,
}

pub fn assemble_writing_context(
    conn: &Connection,
    novel: &Novel,
    current_chapter_id: Option<&str>,
    agent_system_prompt: &str,
) -> Result<AssembledContext, rusqlite::Error> {
    let mut context_parts = Vec::new();

    // 1. Novel basic info
    context_parts.push(format!("【作品名称】：《{}》", novel.title));
    if !novel.synopsis.is_empty() {
        context_parts.push(format!("【全篇大纲梗概】：\n{}", novel.synopsis));
    }

    // 2. Worldview
    let worldview_list = crate::db::settings::list_worldview(conn, &novel.id)?;
    if !worldview_list.is_empty() {
        let mut wv_str = String::from("【核心世界观与设定法则】：\n");
        for wv in worldview_list.iter().take(15) {
            wv_str.push_str(&format!("- [{}] {}: {}\n", wv.group_name, wv.title, wv.content));
        }
        context_parts.push(wv_str);
    }

    // 3. Characters
    let char_list = crate::db::settings::list_characters(conn, &novel.id)?;
    if !char_list.is_empty() {
        let mut char_str = String::from("【主要角色卡】：\n");
        for ch in char_list.iter().take(12) {
            char_str.push_str(&format!("- 【{}】(别名: {}): {}\n", ch.name, ch.aliases, ch.profile));
        }
        context_parts.push(char_str);
    }

    // 4. Plots & Foreshadowing
    let plots = crate::db::settings::list_plots(conn, &novel.id)?;
    let active_plots: Vec<_> = plots.into_iter().filter(|p| p.status == "planted").collect();
    if !active_plots.is_empty() {
        let mut plot_str = String::from("【待回收伏笔与关键事件】：\n");
        for p in active_plots.iter().take(10) {
            plot_str.push_str(&format!("- [{}] {}: {}\n", p.kind, p.title, p.content));
        }
        context_parts.push(plot_str);
    }

    // 5. Current chapter context
    if let Some(ch_id) = current_chapter_id {
        if let Ok(Some(ch)) = crate::db::chapter::get_chapter(conn, ch_id) {
            context_parts.push(format!(
                "【当前所在章节】：第《{}》\n【本章纲要摘要】：{}\n【本章当前正文】：\n{}",
                ch.title,
                ch.summary,
                if ch.content_plain.len() > 3000 {
                    let end_snippet = &ch.content_plain[ch.content_plain.len() - 2500..];
                    format!("...(前文略)...\n{}", end_snippet)
                } else {
                    ch.content_plain
                }
            ));
        }
    }

    let full_context = context_parts.join("\n\n");
    let combined_system = format!(
        "{}\n\n【必须遵循的硬性小说设定依据如下（不得编造与设定矛盾的事实）】：\n{}",
        agent_system_prompt, full_context
    );

    Ok(AssembledContext {
        system_prompt: combined_system,
        context_text: full_context,
    })
}
