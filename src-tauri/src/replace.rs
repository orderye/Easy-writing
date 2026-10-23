//! 批量替换引擎（任务书 T09）。
//!
//! ## 为什么不能对 `content_plain` 做字符串替换再写回
//!
//! 库里一章有两份正文：`content`（Tiptap JSON，带样式与图片）和
//! `content_plain`（纯文本，供搜索与字数用）。把纯文本替换完再写回 JSON 是不可能的 ——
//! 纯文本里没有样式信息，反推不回原来的 marks 与图片节点。
//!
//! 所以替换必须发生在 **JSON 的文本节点上**。
//!
//! ## 为什么不能逐个文本节点各替换一遍
//!
//! ProseMirror 会把一段文字切成多个文本节点（加粗、斜体、颜色变化处都是切口）。
//! 如果「断水剑」三个字正好横跨「断水」+「剑」两个节点，
//! 逐节点替换会**一个都命中不了** —— 而 `content_plain` 里看得见这三个字，
//! 作者在预览里也会看到 1 处命中。两边对不上就是 bug。
//!
//! 因此这里的做法是：
//!
//! 1. 按文档序收集所有文本节点，拼接成 `joined`，同时记下每个节点在 `joined` 里的区间；
//! 2. 在 `joined` 上匹配（跨节点命中自然能命中）；
//! 3. 把每处命中映射回节点区间，做**区间减法**式改写。
//!
//! ## 三条硬约束
//!
//! 1. **不破坏样式与图片**：只改 `{"type":"text"}` 节点的 `text` 字段，
//!    其它节点（image / hardBreak / horizontalRule）原样不动；
//!    命中落在哪个节点上，就用哪个节点原有的 marks。
//! 2. **不能留下空文本节点**：ProseMirror 不接受 `{"type":"text","text":""}`，
//!    前端 `setContent` 会抛错。删空的节点必须整条移除。
//! 3. **字数口径不变**：替换后重新算 `content_plain` 与 `word_count`，
//!    口径与编辑器 / 导出一致（见 `crate::export::doc`）。

use serde_json::Value;

/// 一次替换的执行选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceOptions {
    /// 是否区分大小写。默认不区分（中文无影响，英文更符合直觉）。
    pub case_sensitive: bool,
    /// 是否替换**第一次以上**的全部命中。目前恒为 true（全量替换），
    /// 保留字段是为了将来支持「逐个替换」时不必改签名。
    pub all: bool,
}

impl Default for ReplaceOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            all: true,
        }
    }
}

/// 替换结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceOutcome {
    /// 替换后的 Tiptap JSON。
    pub content: String,
    /// 替换后的纯文本（口径与 `editor.getText()` 一致）。
    pub content_plain: String,
    /// 替换后的字数（口径与 `CharacterCount.characters()` 一致）。
    pub word_count: i32,
    /// 实际替换处数。
    pub replacements: usize,
}

/// 在一章的 Tiptap JSON 上执行替换。
///
/// `content` 不是合法 Tiptap JSON 时返回 `None` —— 调用方应跳过该章并如实上报，
/// 而不是拿 `content_plain` 硬替换后写坏 JSON。
pub fn replace_in_chapter(
    content: &str,
    query: &str,
    replacement: &str,
    options: ReplaceOptions,
) -> Option<ReplaceOutcome> {
    if query.is_empty() {
        return None;
    }
    let mut doc: Value = serde_json::from_str(content).ok()?;
    if doc.get("type").and_then(|t| t.as_str()) != Some("doc") {
        return None;
    }

    // 1. 收集文本节点（按文档序）。
    let mut slots: Vec<TextSlot> = Vec::new();
    collect_text_slots(&mut doc, &mut slots);
    if slots.is_empty() {
        return None;
    }

    // 2. 拼成一个连续字符串，并记录每个节点在其中的区间。
    let mut joined = String::new();
    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(slots.len());
    for slot in &slots {
        let start = joined.len();
        joined.push_str(&slot.text);
        ranges.push((start, joined.len()));
    }

    // 3. 在 joined 上找全部命中。
    let hits = find_all(&joined, query, options.case_sensitive);
    if hits.is_empty() {
        return Some(ReplaceOutcome {
            content: content.to_string(),
            content_plain: parse_plain(content),
            word_count: tiptap_count(content) as i32,
            replacements: 0,
        });
    }

    // 4. 逐节点做区间减法。
    //    每个节点只关心「落在自己区间内的命中」，并把命中裁到自己的边界上。
    let mut replacements = 0usize;
    for (idx, slot) in slots.iter_mut().enumerate() {
        let (node_start, node_end) = ranges[idx];
        let mut result = String::new();
        let mut cursor = 0usize; // 相对本节点文本的偏移

        for (hit_start, hit_end) in &hits {
            // 命中与本节点的交集
            let overlap_start = (*hit_start).max(node_start);
            let overlap_end = (*hit_end).min(node_end);
            if overlap_start >= overlap_end {
                continue;
            }
            let local_start = overlap_start - node_start;
            let local_end = overlap_end - node_start;

            result.push_str(&slot.text[cursor..local_start]);
            // 替换文本只落在**命中的第一个节点**上，其余节点把被吃掉的片段删掉。
            // 否则一处跨节点命中会被写成两份替换文本。
            if overlap_start == *hit_start {
                result.push_str(replacement);
                replacements += 1;
            }
            cursor = local_end;
        }
        result.push_str(&slot.text[cursor..]);

        if result != slot.text {
            slot.new_text = Some(result);
        }
    }

    if replacements == 0 {
        return Some(ReplaceOutcome {
            content: content.to_string(),
            content_plain: parse_plain(content),
            word_count: tiptap_count(content) as i32,
            replacements: 0,
        });
    }

    // 5. 写回 JSON，并移除被删空的文本节点。
    apply_slots(&mut doc, &slots);

    let new_content = serde_json::to_string(&doc).ok()?;
    let new_plain = parse_plain(&new_content);
    let word_count = tiptap_count(&new_content) as i32;

    Some(ReplaceOutcome {
        content: new_content,
        content_plain: new_plain,
        word_count,
        replacements,
    })
}

/// 一个待改写的文本节点。
struct TextSlot {
    text: String,
    /// `None` 表示不改；`Some("")` 表示删空、整条移除。
    new_text: Option<String>,
}

/// 递归收集所有 `{"type":"text"}` 节点。
///
/// 只走 `content` 数组：Tiptap 的块、行内元素都挂在这里，
/// marks 是节点属性而不是子节点，所以不需要单独处理。
fn collect_text_slots(node: &Value, slots: &mut Vec<TextSlot>) {
    let Some(obj) = node.as_object() else { return };
    let Some(children) = obj.get("content").and_then(|c| c.as_array()) else {
        return;
    };
    for child in children.iter() {
        let is_text = child.get("type").and_then(|t| t.as_str()) == Some("text");
        if is_text {
            if let Some(text) = child.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    slots.push(TextSlot {
                        text: text.to_string(),
                        new_text: None,
                    });
                }
            }
        }
        collect_text_slots(child, slots);
    }
}

/// 把 `new_text` 写回 JSON，并移除空文本节点。
///
/// 直接递归遍历、在遇到 text 节点时按顺序取下一个 slot：
/// 收集与写回用的是同一次深度优先遍历，顺序必然一致，
/// 因此不需要真把路径存下来（`path` 字段仅用于调试与断言）。
fn apply_slots(node: &mut Value, slots: &[TextSlot]) {
    let mut cursor = 0usize;
    apply_slots_inner(node, slots, &mut cursor);
}

fn apply_slots_inner(node: &mut Value, slots: &[TextSlot], cursor: &mut usize) {
    let Some(obj) = node.as_object_mut() else { return };
    let Some(children) = obj.get_mut("content").and_then(|c| c.as_array_mut()) else {
        return;
    };

    let mut i = 0usize;
    while i < children.len() {
        let is_text = children[i].get("type").and_then(|t| t.as_str()) == Some("text");
        if is_text {
            if let Some(slot) = slots.get(*cursor) {
                *cursor += 1;
                if let Some(new_text) = &slot.new_text {
                    if new_text.is_empty() {
                        // 删空：整条移除，ProseMirror 不接受空 text 节点。
                        children.remove(i);
                        continue;
                    }
                    if let Some(map) = children[i].as_object_mut() {
                        map.insert("text".to_string(), Value::String(new_text.clone()));
                    }
                }
            }
        } else {
            apply_slots_inner(&mut children[i], slots, cursor);
        }
        i += 1;
    }
}

/// 找出 `haystack` 里 `needle` 的全部命中（字节区间）。
///
/// 公开给 `search_cmd::preview_replace` 复用：预览的命中数必须与执行时
/// 实际改动的处数用同一套匹配规则，否则「预览 3 处、执行改了 2 处」
/// 会让作者不再信任预览。
pub fn find_all(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    // 不区分大小写时统一转小写再找。这里用 `to_lowercase` 的长度可能变化
    // （如 'İ'），因此只用于「找到了没」的判断，实际区间仍按原串重算。
    if case_sensitive || !needle.chars().any(|c| c.is_ascii_alphabetic()) {
        // 纯中文或区分大小写：直接在原串上找，偏移天然正确。
        let mut from = 0usize;
        while let Some(rel) = haystack[from..].find(needle) {
            let start = from + rel;
            out.push((start, start + needle.len()));
            from = start + needle.len();
        }
        return out;
    }

    let hay_lower = haystack.to_lowercase();
    let needle_lower = needle.to_lowercase();
    // 只有当大小写折叠不改变长度时，偏移才能直接复用；否则退回逐字符比较。
    if hay_lower.len() == haystack.len() && needle_lower.len() == needle.len() {
        let mut from = 0usize;
        while let Some(rel) = hay_lower[from..].find(&needle_lower) {
            let start = from + rel;
            out.push((start, start + needle_lower.len()));
            from = start + needle_lower.len();
        }
    } else {
        find_all_unicode_fold(haystack, needle, &mut out);
    }
    out
}

/// 大小写折叠改变了长度时的兜底：逐字符窗口比较。
fn find_all_unicode_fold(haystack: &str, needle: &str, out: &mut Vec<(usize, usize)>) {
    let hay: Vec<(usize, char)> = haystack.char_indices().collect();
    let needle_chars: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    let n = needle_chars.len();
    if n == 0 {
        return;
    }
    for start_idx in 0..hay.len() {
        let mut folded = Vec::with_capacity(n);
        let mut end_idx = start_idx;
        while folded.len() < n && end_idx < hay.len() {
            folded.extend(hay[end_idx].1.to_lowercase());
            end_idx += 1;
        }
        if folded == needle_chars {
            let start_byte = hay[start_idx].0;
            let end_byte = if end_idx < hay.len() {
                hay[end_idx].0
            } else {
                haystack.len()
            };
            out.push((start_byte, end_byte));
        }
    }
}

/// 从 Tiptap JSON 取纯文本，口径与 `editor.getText()` 一致（块间 `\n\n`）。
///
/// 复用导出侧的 `parse_chapter` + `blocks_to_plain_text`：
/// 导出、统计、替换三处必须用同一套口径，否则「替换后字数变了」会成为长期疑案。
fn parse_plain(content: &str) -> String {
    let parsed = crate::export::doc::parse_chapter(content, "");
    crate::export::doc::blocks_to_plain_text(&parsed.blocks)
}

/// 字数口径与编辑器一致（文本字符数 + 叶节点数）。
fn tiptap_count(content: &str) -> i64 {
    match serde_json::from_str::<Value>(content) {
        Ok(doc) => crate::export::doc::tiptap_char_count(&doc),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(paragraphs: &[&str]) -> String {
        let content: Vec<Value> = paragraphs
            .iter()
            .map(|p| {
                serde_json::json!({
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": p }]
                })
            })
            .collect();
        serde_json::json!({ "type": "doc", "content": content }).to_string()
    }

    #[test]
    fn replaces_within_a_single_text_node() {
        let content = doc_with(&["他握紧断水剑。"]);
        let out = replace_in_chapter(&content, "断水剑", "青霜剑", Default::default()).unwrap();
        assert_eq!(out.replacements, 1);
        assert!(out.content.contains("他握紧青霜剑。"));
        assert!(out.content_plain.contains("青霜剑"));
    }

    #[test]
    fn removes_nodes_emptied_by_replacement() {
        let content = doc_with(&["删掉"]);
        let out = replace_in_chapter(&content, "删掉", "", Default::default()).unwrap();
        assert_eq!(out.replacements, 1);
        // 空 text 节点必须被移除，否则前端 setContent 会抛错。
        assert!(!out.content.contains("\"text\":\"\""));
    }

    #[test]
    fn no_hit_is_reported_as_zero_and_content_unchanged() {
        let content = doc_with(&["风平浪静。"]);
        let out = replace_in_chapter(&content, "断水剑", "青霜剑", Default::default()).unwrap();
        assert_eq!(out.replacements, 0);
        assert_eq!(out.content, content);
    }

    #[test]
    fn invalid_json_is_rejected_rather_than_written_back() {
        assert!(replace_in_chapter("not json", "a", "b", Default::default()).is_none());
        assert!(replace_in_chapter("<p>x</p>", "a", "b", Default::default()).is_none());
    }
}
