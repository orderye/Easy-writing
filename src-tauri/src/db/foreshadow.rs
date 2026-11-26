//! 伏笔看板（T10）。
//!
//! 把 `plot_item` 的原始字段加上「全书章序位置」与「超期判定」后的展示结构：
//! 看板不需要作者算章距，后端按卷章排序算好。
//!
//! 「距今章数」的口径：`list_all_chapter_headers_for_novel` 已按
//! `volume.sort_order → chapter.sort_order → created_at` 排好，下标即全书章序。
//! `chapters_since_plant = 全书末章下标 + 1 - plant_index` —— 埋设在末章就是 0，
//! 埋设在倒数第二章就是 1。跨卷天然正确，因为章序是「全书」序而非「卷内」序。
//!
//! 关联角色的匹配：复制 T06 `ai/context.rs` 的 `match_presence` 逻辑
//! （本名命中 → 别名命中 → 不命中），但文本来源是**伏笔内容**而非章节正文。
//! 不抽公共函数：T06 的函数签名绑定上下文装配语义，改它会动一条已钉死测试的热路径。

use crate::db::chapter::list_all_chapter_headers_for_novel;
use crate::db::setting_view::{load_setting_index, match_character_presence, PresenceMatch};
use rusqlite::{Connection, Result};
use shared_types::{Character, ForeshadowRow, PlotItem};

/// 超期阈值默认值。
pub const DEFAULT_OVERDUE_CHAPTERS: usize = 30;

/// 从 `app_setting` 读超期阈值；缺键或非法值一律回落默认。
///
/// 读取失败不阻断看板：默认值是「30 章」，与产品默认一致，
// 而不是悄悄变成「永不高亮」。
pub fn overdue_threshold_chars(conn: &Connection) -> usize {
    crate::db::app_setting::get_setting(conn, "foreshadow_overdue_chapters")
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_OVERDUE_CHAPTERS)
}

/// 把「本名 + 别名」与给定文本做包含匹配，返回命中的角色名。
///
/// 与 `ai/context.rs` 的 `match_presence` 同口径（本名命中 → 别名命中 → 不命中），
/// 但文本来源是伏笔内容而非章节正文。
pub fn characters_mentioned_in(text: &str, characters: &[Character]) -> Vec<String> {
    if text.is_empty() || characters.is_empty() {
        return Vec::new();
    }
    let mut matched = Vec::new();
    for c in characters {
        if !matches!(
            match_character_presence(&c.name, &c.aliases, text),
            PresenceMatch::None
        ) {
            matched.push(c.name.clone());
        }
    }
    matched
}

/// 伏笔看板：按全书章序算出埋设/回收位置与距今章数，并匹配关联角色。
pub fn list_foreshadow_board(conn: &Connection, novel_id: &str) -> Result<Vec<ForeshadowRow>> {
    let view = load_setting_index(conn, novel_id)?;
    let plots = view.plots;
    if plots.is_empty() {
        return Ok(Vec::new());
    }

    // 建 id → 章序下标 的映射（list_all_chapter_headers_for_novel 已按卷序→章序排好）
    let headers = list_all_chapter_headers_for_novel(conn, novel_id)?;
    let mut chapter_index: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, h) in headers.iter().enumerate() {
        chapter_index.insert(h.id.as_str(), i);
    }
    let last_index = headers.len().saturating_sub(1);

    let threshold = overdue_threshold_chars(conn);

    // 关联角色：一次取出全部角色，逐条伏笔匹配
    let characters = view.characters;

    let mut rows: Vec<ForeshadowRow> = plots
        .into_iter()
        .map(|p| {
            let plant_idx = p.chapter_id.as_deref().and_then(|cid| chapter_index.get(cid).copied());
            let resolved_idx = p
                .resolved_chapter_id
                .as_deref()
                .and_then(|cid| chapter_index.get(cid).copied());
            // 距今章数 = 末章下标 - 埋设章下标。
            // 埋设在末章（下标 14）→ 0；埋设在第 3 章（下标 2）→ 12。
            let chapters_since_plant = plant_idx.map(|idx| last_index.saturating_sub(idx));
            let overdue = p.status == "planted"
                && chapters_since_plant
                    .map(|n| n > threshold)
                    .unwrap_or(false);
            let related_characters = characters_mentioned_in(&p.content, &characters);

            ForeshadowRow {
                id: p.id,
                novel_id: p.novel_id,
                title: p.title,
                content: p.content,
                kind: p.kind,
                status: p.status,
                chapter_id: p.chapter_id,
                resolved_chapter_id: p.resolved_chapter_id,
                chapter_index: plant_idx,
                resolved_chapter_index: resolved_idx,
                chapters_since_plant,
                overdue,
                related_characters,
            }
        })
        .collect();

    // 排序：未回收在前 → 距今章数降序（埋得越早越靠前）→ id 兜底
    rows.sort_by(|a, b| {
        let a_unresolved = a.status != "resolved";
        let b_unresolved = b.status != "resolved";
        if a_unresolved != b_unresolved {
            // 未回收（true）排前，已回收（false）排后
            return b_unresolved.cmp(&a_unresolved);
        }
        // 未回收：距今章数降序
        match (a.chapters_since_plant, b.chapters_since_plant) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.id.cmp(&b.id),
        }
    });

    Ok(rows)
}

/// 标记伏笔回收（T10 / V008）：设置 status=resolved 并记录回收章节。
///
/// 校验：
///   - 伏笔存在且未删；
///   - 若给了 `resolved_chapter_id`，该章节必须属于该作品（不静默接受跨作品章节）；
///   - 写 `status = 'resolved'` + `resolved_chapter_id` + `updated_at`；
///   - 通过 `record_setting_change` 留档（target_type = "plot"），与编辑路径一致。
pub fn resolve_plot(
    conn: &Connection,
    id: &str,
    resolved_chapter_id: Option<String>,
) -> Result<PlotItem> {
    crate::db::setting_tx::resolve_plot_with_reason(
        conn,
        id,
        resolved_chapter_id,
        "伏笔看板标记回收",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::Character;

    fn char(name: &str, aliases: &str) -> Character {
        Character {
            id: format!("ch_{}", name),
            novel_id: "n1".to_string(),
            name: name.to_string(),
            aliases: aliases.to_string(),
            profile: "".to_string(),
            origin: "manual".to_string(),
            source_chapter_id: None,
            sort_order: 0,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            pinned: false,
            deleted_at: None,
        }
    }

    #[test]
    fn matches_by_name_and_alias() {
        let chars = vec![
            char("沈砚", "[]"),
            char("阿岚", "[\"岚子\"]"),
        ];
        // 本名命中
        let r = characters_mentioned_in("沈砚在第 3 章战死", &chars);
        assert_eq!(r, vec!["沈砚".to_string()]);
        // 别名命中
        let r = characters_mentioned_in("岚子 등장", &chars);
        assert_eq!(r, vec!["阿岚".to_string()]);
        // 都不命中
        let r = characters_mentioned_in("无关内容", &chars);
        assert!(r.is_empty());
        // 空文本
        assert!(characters_mentioned_in("", &chars).is_empty());
        // 空角色列表
        assert!(characters_mentioned_in("沈砚", &[]).is_empty());
    }

    #[test]
    fn empty_aliases_parses_as_no_alias() {
        let chars = vec![char("沈砚", "not-json")];
        // aliases 解析失败按「无别名」处理，不报错
        assert!(characters_mentioned_in("沈砚", &chars) == vec!["沈砚".to_string()]);
    }
}
