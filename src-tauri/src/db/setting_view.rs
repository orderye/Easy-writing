//! 设定索引只读导出视图（MIT 公理设计 DP5 → DP8/DP12 单向依赖边界）。
//!
//! 供上下文装配（`ai/context.rs`）与冲突检测（`ai/consistency.rs`）单向消费，
//! 集中维护 `pinned` 优先排序、文本安全截断与角色别名命中规则。

use rusqlite::{Connection, Result};
use shared_types::{Character, PlotItem, WorldviewEntry};

/// 设定库只读导出视图：三类核心设定均已按 `pinned` 优先、原有 `sort_order` 次之排列。
pub struct SettingIndexView {
    pub worldviews: Vec<WorldviewEntry>,
    pub characters: Vec<Character>,
    pub plots: Vec<PlotItem>,
}

/// 加载指定作品的只读设定索引视图（仅包含未软删条目，`pinned` 置前）。
pub fn load_setting_index(conn: &Connection, novel_id: &str) -> Result<SettingIndexView> {
    let mut worldviews = crate::db::settings::list_worldview(conn, novel_id)?;
    let mut characters = crate::db::settings::list_characters(conn, novel_id)?;
    let mut plots = crate::db::settings::list_plots(conn, novel_id)?;

    // 稳定排序：pinned=true 排在前面，组内保持 SQL 的 sort_order + created_at 顺序。
    worldviews.sort_by_key(|w| !w.pinned);
    characters.sort_by_key(|c| !c.pinned);
    plots.sort_by_key(|p| !p.pinned);

    Ok(SettingIndexView {
        worldviews,
        characters,
        plots,
    })
}

/// 按字符数截断（含省略号不超过 `max` 个字符），超出时以省略号结尾。
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// 解析 `character.aliases`（JSON 字符串数组）。解析失败按无别名处理，不阻断上层流程。
pub fn parse_aliases(aliases_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(aliases_json).unwrap_or_default()
}

/// 角色在文本（正文或指令）中的命中方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceMatch {
    None,
    /// 按本名命中。
    Name,
    /// 仅按别名命中。
    Alias,
}

/// 判定角色的命中方式：本名优先，别名次之。
pub fn match_character_presence(name: &str, aliases_json: &str, text: &str) -> PresenceMatch {
    if text.is_empty() {
        return PresenceMatch::None;
    }
    if !name.is_empty() && text.contains(name) {
        return PresenceMatch::Name;
    }
    if parse_aliases(aliases_json)
        .iter()
        .any(|a| !a.is_empty() && text.contains(a.as_str()))
    {
        return PresenceMatch::Alias;
    }
    PresenceMatch::None
}
