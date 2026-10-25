pub mod ai_cmd;
pub mod config_cmd;
pub mod export_cmd;
pub mod search_cmd;
pub mod sync_cmd;
pub mod update_cmd;

use crate::db::DbPool;
use shared_types::{
    Chapter, ChapterHeader, ChapterRevision, ChapterRevisionHeader, Character, ForeshadowRow,
    LexiconEntry, Novel, PlotItem, SettingChange, TrashItem, Volume, WorldviewEntry,
};
use tauri::State;

// Novel commands
#[tauri::command(rename_all = "snake_case")]
pub fn create_novel(
    pool: State<DbPool>,
    title: String,
    synopsis: String,
) -> Result<Novel, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::create_novel(&conn, &title, &synopsis).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_novels(pool: State<DbPool>) -> Result<Vec<Novel>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::list_novels(&conn).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_novels_with_stats(
    pool: State<DbPool>,
) -> Result<Vec<shared_types::NovelWithStats>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::list_novels_with_stats(&conn).map_err(|e| e.to_string())
}

// 注：这里原先有 get_novel 命令，因前端从不需要单查作品（等价数据源见
// `list_novels` 与前端持有的 current_novel）而删除。
// 数据库层的 db::novel::get_novel 保留 —— create_novel、AI 提炼、ai_chat_stream
// 等 4+ 处内部逻辑依赖它。删命令不等于删函数。

#[tauri::command(rename_all = "snake_case")]
pub fn update_novel(
    pool: State<DbPool>,
    id: String,
    title: String,
    synopsis: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::update_novel(&conn, &id, &title, &synopsis).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_novel(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::novel::delete_novel(&conn, &id).map_err(|e| e.to_string())
}

/// 列出已删除的作品（跨作品作用域）。
///
/// 回收站其余条目都以「当前作品」为作用域，而被删除的作品不属于任何作品，
/// 因此需要这个独立入口——否则删除作品就无法找回。
#[tauri::command(rename_all = "snake_case")]
pub fn list_deleted_novels(pool: State<DbPool>) -> Result<Vec<TrashItem>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::list_deleted_novels(&conn).map_err(|e| e.to_string())
}

// Volume commands
#[tauri::command(rename_all = "snake_case")]
pub fn create_volume(
    pool: State<DbPool>,
    novel_id: String,
    title: String,
    summary: String,
) -> Result<Volume, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::create_volume(&conn, &novel_id, &title, &summary).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_volumes(pool: State<DbPool>, novel_id: String) -> Result<Vec<Volume>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::list_volumes(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_volume(
    pool: State<DbPool>,
    id: String,
    title: String,
    summary: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::update_volume(&conn, &id, &title, &summary).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reorder_volumes(pool: State<DbPool>, volume_ids: Vec<String>) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::reorder_volumes(&conn, &volume_ids).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_volume(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::volume::delete_volume(&conn, &id).map_err(|e| e.to_string())
}

// Chapter commands
#[tauri::command(rename_all = "snake_case")]
pub fn create_chapter(
    pool: State<DbPool>,
    novel_id: String,
    volume_id: String,
    title: String,
    summary: String,
) -> Result<Chapter, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::create_chapter(&conn, &novel_id, &volume_id, &title, &summary)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_chapter_headers(
    pool: State<DbPool>,
    volume_id: String,
) -> Result<Vec<ChapterHeader>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::list_chapter_headers(&conn, &volume_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_all_chapter_headers_for_novel(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<ChapterHeader>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::list_all_chapter_headers_for_novel(&conn, &novel_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_chapter(pool: State<DbPool>, id: String) -> Result<Option<Chapter>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::get_chapter(&conn, &id).map_err(|e| e.to_string())
}

/// 保存后是否应触发后台提炼。
///
/// 抽成纯函数的原因：save_chapter_content 的首参是 tauri::AppHandle，
/// 在 MockRuntime 下无法注册（见 tests/ipc_argcase.rs 里的详细说明），
/// 所以这条决策逻辑必须能脱离 IPC 层被单测覆盖。
///
/// 默认关闭：设置缺失、读取失败，或值不是 "true"/"1" 时一律不触发 ——
/// 避免把要花钱的模型调用变成用户没同意的默认行为。
pub fn should_trigger_auto_extract(
    auto_extract_setting: Option<&str>,
    content_plain: &str,
) -> bool {
    let enabled = matches!(auto_extract_setting, Some("true") | Some("1"));
    // 按字符数而非字节数计：中文正文的字节数会让门槛形同虚设。
    enabled && content_plain.chars().count() >= 100
}

#[cfg(test)]
mod auto_extract_tests {
    use super::should_trigger_auto_extract;

    #[test]
    fn defaults_to_off_when_setting_missing() {
        let long = "字".repeat(200);
        assert!(!should_trigger_auto_extract(None, &long));
    }

    #[test]
    fn only_explicit_truthy_values_enable_it() {
        let long = "字".repeat(200);
        assert!(should_trigger_auto_extract(Some("true"), &long));
        assert!(should_trigger_auto_extract(Some("1"), &long));
        assert!(!should_trigger_auto_extract(Some("false"), &long));
        assert!(!should_trigger_auto_extract(Some("0"), &long));
        assert!(!should_trigger_auto_extract(Some(""), &long));
        assert!(!should_trigger_auto_extract(Some("TRUE"), &long));
    }

    #[test]
    fn short_text_never_triggers() {
        assert!(!should_trigger_auto_extract(Some("true"), ""));
        assert!(!should_trigger_auto_extract(Some("true"), "短"));
        assert!(!should_trigger_auto_extract(Some("true"), &"字".repeat(99)));
        assert!(should_trigger_auto_extract(Some("true"), &"字".repeat(100)));
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_chapter_content(
    app_handle: tauri::AppHandle,
    pool: State<DbPool>,
    id: String,
    content: String,
    content_plain: String,
    word_count: i32,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::save_chapter_content(&conn, &id, &content, &content_plain, word_count)
        .map_err(|e| e.to_string())?;

    // Auto snapshot trigger if conditions are met
    if let Ok(Some(chap)) = crate::db::chapter::get_chapter(&conn, &id) {
        let _ = crate::db::chapter_revision::maybe_auto_snapshot(
            &conn,
            &id,
            &chap.novel_id,
            &chap.title,
            &content,
            &content_plain,
            word_count,
        );

        // 保存后自动提炼：受 app_setting.auto_extract 开关控制（PRD §4.2 P1，默认关闭）。
        // 决策抽到 should_trigger_auto_extract，理由见其文档注释。
        let auto_extract_setting = crate::db::app_setting::get_setting(&conn, "auto_extract")
            .ok()
            .flatten();

        if !should_trigger_auto_extract(auto_extract_setting.as_deref(), &content_plain) {
            return Ok(());
        }

        // Silent background extraction trigger if text has meaningful length
        let pool_clone = pool.inner().clone();
        let novel_id = chap.novel_id.clone();
        let chapter_id = id.clone();
        let handle_clone = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let res = (|| -> Result<_, String> {
                let conn = pool_clone.get().map_err(|e| e.to_string())?;
                let novel = match crate::db::novel::get_novel(&conn, &novel_id) {
                    Ok(Some(n)) => n,
                    _ => return Err("Novel not found".to_string()),
                };
                let chapter = match crate::db::chapter::get_chapter(&conn, &chapter_id) {
                    Ok(Some(c)) => c,
                    _ => return Err("Chapter not found".to_string()),
                };
                let (_agent, profile, api_key) =
                    crate::agent::dispatcher::resolve_agent_and_profile(&conn, "writing_assistant")?;
                Ok((novel, chapter, profile, api_key))
            })();

            if let Ok((novel, chapter, profile, api_key)) = res {
                // 长度门槛已在 should_trigger_auto_extract 里判过，此处不再重复。
                {
                    let gateway_arc = std::sync::Arc::new(crate::ai::gateway::AiGateway::new());
                    if let Ok(outcome) = crate::agent::extraction::run_parallel_extraction(
                        gateway_arc,
                        pool_clone,
                        novel,
                        chapter,
                        profile,
                        api_key,
                    ).await {
                        use tauri::Emitter;
                        let _ = handle_clone.emit("extraction_completed", outcome.candidates.len());
                        // 自动提炼是后台触发的，作者不在弹窗前。
                        // 失败必须留下痕迹，否则配置错误会永久静默
                        // （旧实现只发候选数，全员 401 与「本章无新内容」无法区分）。
                        if let Some(msg) = outcome.summarize() {
                            eprintln!("[auto-extract] 提炼未完整完成：{msg}");
                        }
                    }
                }
            }
        });
    }
    Ok(())
}

/// 找出正文里提到过某个词的全部章节（改动设定后排查影响面用）。
///
/// 确定性实现：只做字面匹配，不调用模型，所以结果可复现、可断言。
#[tauri::command(rename_all = "snake_case")]
pub fn find_affected_chapters(
    pool: State<DbPool>,
    novel_id: String,
    term: String,
) -> Result<Vec<shared_types::AffectedChapter>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::find_chapters_mentioning(&conn, &novel_id, &term)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_chapter_revisions(
    pool: State<DbPool>,
    chapter_id: String,
) -> Result<Vec<ChapterRevisionHeader>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter_revision::list_revisions_for_chapter(&conn, &chapter_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_chapter_revision(
    pool: State<DbPool>,
    id: String,
) -> Result<Option<ChapterRevision>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter_revision::get_revision(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_manual_chapter_revision(
    pool: State<DbPool>,
    chapter_id: String,
    note: String,
) -> Result<ChapterRevision, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let chap = crate::db::chapter::get_chapter(&conn, &chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Chapter not found".to_string())?;

    crate::db::chapter_revision::create_revision(
        &conn,
        &chap.id,
        &chap.novel_id,
        &chap.title,
        &chap.content,
        &chap.content_plain,
        chap.word_count,
        "manual",
        &note,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn revert_chapter_to_revision(
    pool: State<DbPool>,
    chapter_id: String,
    revision_id: String,
) -> Result<Chapter, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter_revision::revert_chapter_to_revision(&conn, &chapter_id, &revision_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_chapter_meta(
    pool: State<DbPool>,
    id: String,
    title: String,
    summary: String,
    status: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::update_chapter_meta(&conn, &id, &title, &summary, &status)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn move_chapter(
    pool: State<DbPool>,
    chapter_id: String,
    new_volume_id: String,
    new_sort_order: i32,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::move_chapter_to_volume(&conn, &chapter_id, &new_volume_id, new_sort_order)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reorder_chapters(pool: State<DbPool>, chapter_ids: Vec<String>) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::reorder_chapters(&conn, &chapter_ids).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_chapter(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::chapter::delete_chapter(&conn, &id).map_err(|e| e.to_string())
}

// Settings commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_worldview(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<WorldviewEntry>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_worldview(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_worldview(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    group_name: String,
    title: String,
    content: String,
) -> Result<WorldviewEntry, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let entry = crate::db::settings::save_worldview(&conn, id, &novel_id, &group_name, &title, &content)
        .map_err(|e| e.to_string())?;
    
    let clean = entry.title.trim();
    if clean.chars().count() >= 2 && clean.chars().count() <= 30 {
        let _ = crate::db::lexicon::upsert_lexicon_term(&conn, &novel_id, clean, "worldview", Some(&entry.id));
    }
    Ok(entry)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_worldview(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::delete_worldview(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_characters(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<Character>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_characters(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_character(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    name: String,
    aliases: String,
    profile: String,
) -> Result<Character, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let character = crate::db::settings::save_character(&conn, id, &novel_id, &name, &aliases, &profile)
        .map_err(|e| e.to_string())?;

    let clean_name = character.name.trim();
    if clean_name.chars().count() >= 2 && clean_name.chars().count() <= 20 {
        let _ = crate::db::lexicon::upsert_lexicon_term(&conn, &novel_id, clean_name, "character", Some(&character.id));
    }
    if let Ok(alias_list) = serde_json::from_str::<Vec<String>>(&character.aliases) {
        for a in alias_list {
            let a_clean = a.trim();
            if a_clean.chars().count() >= 2 && a_clean.chars().count() <= 20 {
                let _ = crate::db::lexicon::upsert_lexicon_term(&conn, &novel_id, a_clean, "character", Some(&character.id));
            }
        }
    }
    Ok(character)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_character(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::delete_character(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_plots(pool: State<DbPool>, novel_id: String) -> Result<Vec<PlotItem>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_plots(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_plot(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    title: String,
    content: String,
    kind: String,
    status: String,
    chapter_id: Option<String>,
) -> Result<PlotItem, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::save_plot(
        &conn, id, &novel_id, &title, &content, &kind, &status, chapter_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_plot(pool: State<DbPool>, id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::delete_plot(&conn, &id).map_err(|e| e.to_string())
}

/// 伏笔看板（T10）：含埋设/回收位置、距今章数、超期高亮与关联角色。
#[tauri::command(rename_all = "snake_case")]
pub fn list_foreshadow_board(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<ForeshadowRow>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::foreshadow::list_foreshadow_board(&conn, &novel_id).map_err(|e| e.to_string())
}

/// 标记伏笔回收（T10 / V008）：设置 status=resolved 并记录回收章节。
#[tauri::command(rename_all = "snake_case")]
pub fn resolve_plot(
    pool: State<DbPool>,
    id: String,
    resolved_chapter_id: Option<String>,
) -> Result<PlotItem, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::foreshadow::resolve_plot(&conn, &id, resolved_chapter_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_setting_changes(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<SettingChange>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::list_setting_changes(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn revert_setting_change(
    pool: State<DbPool>,
    change_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::revert_setting_change(&conn, &change_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn apply_conflict_setting_update(
    pool: State<DbPool>,
    novel_id: String,
    chapter_id: Option<String>,
    setting_module: String,
    setting_title: String,
    new_content: String,
    reason: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::settings::apply_conflict_setting_update(
        &conn,
        &novel_id,
        chapter_id.as_deref(),
        &setting_module,
        &setting_title,
        &new_content,
        &reason,
    )
    .map_err(|e| e.to_string())
}

// App Settings commands
#[tauri::command(rename_all = "snake_case")]
pub fn get_app_setting(pool: State<DbPool>, key: String) -> Result<Option<String>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::app_setting::get_setting(&conn, &key).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_app_setting(pool: State<DbPool>, key: String, value: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::app_setting::set_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_image_asset(
    app: tauri::AppHandle,
    pool: State<DbPool>,
    novel_id: String,
    name: String,
    mime: String,
    base64_data: String,
) -> Result<String, String> {
    use tauri::Manager;
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::save_image(&conn, &app_data_dir, &novel_id, &name, &mime, &base64_data)
}

/// 返回本应用的 APPDATA 绝对路径。
///
/// 供前端把 asset 的相对路径拼成可显示的 asset:// URL：
/// Tauri 的路径 API 只在 JS 侧可用，而 assetProtocol 的 scope 是
/// $APPDATA/images/**，必须先拿到这个前缀才能交给 convertFileSrc。
#[tauri::command(rename_all = "snake_case")]
pub fn get_app_data_dir(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// 列出某作品的图片资产（相对路径 + 元信息）。
#[tauri::command(rename_all = "snake_case")]
pub fn list_image_assets(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<shared_types::AssetInfo>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::list_assets(&conn, &novel_id).map_err(|e| e.to_string())
}

/// 重命名图片资产：只改显示名。磁盘文件名是内容哈希，不受影响，
/// 因此不会影响正文引用，也不会影响 WebDAV 增量上传。
#[tauri::command(rename_all = "snake_case")]
pub fn rename_image_asset(
    pool: State<DbPool>,
    asset_id: String,
    name: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::rename_asset(&conn, &asset_id, &name).map_err(|e| e.to_string())
}

/// 从图库移除一张图片：软删除，可恢复；**不动磁盘文件**。
///
/// 文件回收交由图库「维护」里的孤儿清理完成——那里会先确认该文件没有任何
/// 资产记录引用、也没有任何正文/历史版本引用，才真正删盘。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_image_asset(pool: State<DbPool>, asset_id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::soft_delete_asset(&conn, &asset_id).map_err(|e| e.to_string())
}

/// 列出某作品已移除（软删除）的图片，供恢复。
#[tauri::command(rename_all = "snake_case")]
pub fn list_deleted_image_assets(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<shared_types::AssetInfo>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::list_deleted_assets(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn restore_image_asset(pool: State<DbPool>, asset_id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::restore_asset(&conn, &asset_id).map_err(|e| e.to_string())
}

/// 彻底删除资产记录（不可恢复）。仅删记录，磁盘文件留给孤儿清理判断。
#[tauri::command(rename_all = "snake_case")]
pub fn purge_image_asset(pool: State<DbPool>, asset_id: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::purge_asset(&conn, &asset_id).map_err(|e| e.to_string())
}

/// 查询一张图的引用规模（正文 / 历史版本 / 同文件的其余记录），用于删除前提示。
#[tauri::command(rename_all = "snake_case")]
pub fn get_image_asset_usage(
    pool: State<DbPool>,
    asset_id: String,
) -> Result<shared_types::ImageAssetUsage, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::asset_usage(&conn, &asset_id).map_err(|e| e.to_string())
}

/// 对账图片存储。作用域是**全库**，不是当前作品——孤儿文件不属于任何作品
/// （其记录已被删除），无法按作品归属，因此界面上必须写明这一点。
///
/// 返回：磁盘上无任何记录引用的孤儿文件，以及记录在库但文件缺失的条目。
#[tauri::command(rename_all = "snake_case")]
pub fn scan_image_storage(
    app: tauri::AppHandle,
    pool: State<DbPool>,
) -> Result<shared_types::ImageScanReport, String> {
    use tauri::Manager;
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::scan_storage(&conn, &app_data_dir).map_err(|e| e.to_string())
}

/// 清理孤儿图片文件，返回实际删除数。
///
/// 只删「无记录引用 + 无正文/历史版本引用」的文件；被正文引用着的孤儿只报告
/// 不删除——否则正文会当场裂图。
#[tauri::command(rename_all = "snake_case")]
pub fn prune_orphan_images(app: tauri::AppHandle, pool: State<DbPool>) -> Result<i64, String> {
    use tauri::Manager;
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::asset::prune_orphan_images(&conn, &app_data_dir).map_err(|e| e.to_string())
}

// Trash / Recycle Bin commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_trash_items(pool: State<DbPool>, novel_id: String) -> Result<Vec<TrashItem>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::list_trash_items(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn restore_trash_item(
    pool: State<DbPool>,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::restore_trash_item(&conn, &item_type, &item_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn purge_trash_item(
    pool: State<DbPool>,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    let mut conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::purge_trash_item(&mut conn, &item_type, &item_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn empty_trash(pool: State<DbPool>, novel_id: String) -> Result<(), String> {
    let mut conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::trash::empty_trash(&mut conn, &novel_id).map_err(|e| e.to_string())
}

// Import commands
#[tauri::command(rename_all = "snake_case")]
pub fn parse_import_document(
    file_name: String,
    raw_text: String,
) -> Result<shared_types::ImportPreview, String> {
    Ok(crate::import::parse_import_document(&file_name, &raw_text))
}

#[tauri::command(rename_all = "snake_case")]
pub fn parse_import_epub(
    file_name: String,
    epub_base64: String,
) -> Result<shared_types::ImportPreview, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&epub_base64)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;
    crate::import::epub::parse_import_epub(&file_name, &bytes)
}

#[tauri::command(rename_all = "snake_case")]
pub fn commit_import_novel(
    pool: State<DbPool>,
    target_novel_id: Option<String>,
    preview: shared_types::ImportPreview,
) -> Result<Novel, String> {
    let mut conn = pool.get().map_err(|e| e.to_string())?;
    crate::import::commit_import_novel(&mut conn, target_novel_id, preview)
}

// Character Relation commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_character_relations(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<shared_types::CharacterRelation>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::character_relation::list_character_relations(&conn, &novel_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_character_relation(
    pool: State<DbPool>,
    id: Option<String>,
    novel_id: String,
    from_char_id: String,
    to_char_id: String,
    relation_name: String,
    description: String,
) -> Result<shared_types::CharacterRelation, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::character_relation::save_character_relation(
        &conn,
        id,
        &novel_id,
        &from_char_id,
        &to_char_id,
        &relation_name,
        &description,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_character_relation(
    pool: State<DbPool>,
    id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::character_relation::delete_character_relation(&conn, &id)
        .map_err(|e| e.to_string())
}

// Prompt Template commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_prompt_templates(
    pool: State<DbPool>,
) -> Result<Vec<shared_types::PromptTemplate>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::prompt_template::list_prompt_templates(&conn)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_prompt_template(
    pool: State<DbPool>,
    key: String,
    template: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::prompt_template::save_prompt_template(&conn, &key, &template)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reset_prompt_template(
    pool: State<DbPool>,
    key: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::prompt_template::reset_prompt_template(&conn, &key)
        .map_err(|e| e.to_string())
}

// Lexicon / Autocomplete Dictionary commands
#[tauri::command(rename_all = "snake_case")]
pub fn list_lexicon(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<Vec<LexiconEntry>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::lexicon::list_lexicon(&conn, &novel_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_lexicon_term(
    pool: State<DbPool>,
    novel_id: String,
    term: String,
    category: String,
    source_id: Option<String>,
) -> Result<LexiconEntry, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::lexicon::upsert_lexicon_term(&conn, &novel_id, &term, &category, source_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_lexicon_term(
    pool: State<DbPool>,
    id: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::lexicon::delete_lexicon_term(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn record_lexicon_usage(
    pool: State<DbPool>,
    novel_id: String,
    term: String,
) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::lexicon::record_term_usage(&conn, &novel_id, &term).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn sync_lexicon_from_novel(
    pool: State<DbPool>,
    novel_id: String,
) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let settings_count = crate::db::lexicon::sync_lexicon_from_settings(&conn, &novel_id)
        .map_err(|e| e.to_string())?;
    let corpus_count = crate::db::lexicon::scan_and_extract_corpus_terms(&conn, &novel_id)
        .map_err(|e| e.to_string())?;
    Ok(settings_count + corpus_count)
}

#[tauri::command(rename_all = "snake_case")]
pub fn import_lexicon_terms(
    pool: State<DbPool>,
    novel_id: String,
    terms: Vec<String>,
    category: String,
) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let mut imported = 0;
    for t in terms {
        let trimmed = t.trim();
        if trimmed.chars().count() >= 2 && trimmed.chars().count() <= 40 {
            if crate::db::lexicon::upsert_lexicon_term(&conn, &novel_id, trimmed, &category, None).is_ok() {
                imported += 1;
            }
        }
    }
    Ok(imported)
}





