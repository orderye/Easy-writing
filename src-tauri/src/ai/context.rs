//! 写作上下文的装配。
//!
//! 历史问题：这里曾经是一串拍脑袋的 take(N) —— 世界观取前 15 条、
//! 角色取前 12 条、伏笔取前 10 条，当前章正文超过 3000 字只留最后 2500 字。
//! 结果是：写到第 500 章时，进入 prompt 的仍然是列表里最靠前的那十几条设定，
//! 早登场但排序靠后的角色**永远不会**被送进上下文 —— 不是"缺少评测"，
//! 是结构上就没有这个能力。
//!
//! 现在的装配方式：
//!   1. **按预算**而不是按条数：总字符数有上限，单条超长会被截断而不是整条丢弃；
//!   2. **按相关性**而不是按排序取角色：先看本章正文里出现了谁（名字或别名命中），
//!      出场角色给完整档案，未出场角色降级为只留名字，避免挤占预算；
//!   3. 优先级明确：作品梗概 > 当前章 > 出场角色 > 待回收伏笔 > 世界观。
use rusqlite::Connection;
use shared_types::Novel;

pub struct AssembledContext {
    pub system_prompt: String,
    pub context_text: String,
}

/// 默认字符预算。约等于 8k token 量级的中文，留出空间给对话历史和模型输出。
pub const DEFAULT_CONTEXT_BUDGET_CHARS: usize = 12_000;

/// 分段之间的分隔符长度（两个换行）。
const SEP: usize = 2;

/// 单条设定内容的上限：一条超长条目不该吃掉整个预算。
const MAX_SINGLE_ENTRY_CHARS: usize = 900;

/// 这里历史上写死的角色条数上限。保留它只是为了在测试与文档里引用
/// "以前会漏掉多少条"，**不要**再用它做截断。
#[doc(hidden)]
pub const LEGACY_CHARACTER_TAKE: usize = 12;

/// 按字符数截断，超出时以省略号结尾。
fn truncate_chars(s: &str, max: usize) -> String {
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

/// 尝试把一段文本装进预算；装不下就截断；连一个字符都放不下则返回 false。
fn push_capped(parts: &mut Vec<String>, used: &mut usize, budget: usize, text: &str) -> bool {
    if text.is_empty() || *used >= budget {
        return false;
    }
    let avail = budget - *used;
    let avail = if avail > SEP { avail - SEP } else { return false };
    let capped = truncate_chars(text, avail);
    if capped.is_empty() {
        return false;
    }
    *used += capped.chars().count() + SEP;
    parts.push(capped);
    true
}

/// 解析 character.aliases（JSON 字符串数组）。解析失败按"无别名"处理，
/// 不能让一条脏数据把整个上下文装配打断。
fn parse_aliases(aliases_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(aliases_json).unwrap_or_default()
}

/// 角色是否在本章正文中被提及（按本名或任一别名）。
fn character_appears(name: &str, aliases_json: &str, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if !name.is_empty() && text.contains(name) {
        return true;
    }
    parse_aliases(aliases_json)
        .iter()
        .any(|a| !a.is_empty() && text.contains(a.as_str()))
}

/// 把角色分成"本章出场"与"未出场"两组，保持原有顺序。
fn split_by_presence(
    chars: Vec<shared_types::Character>,
    current_text: &str,
) -> (Vec<shared_types::Character>, Vec<shared_types::Character>) {
    let mut on_stage = Vec::new();
    let mut off_stage = Vec::new();
    for c in chars {
        if character_appears(&c.name, &c.aliases, current_text) {
            on_stage.push(c);
        } else {
            off_stage.push(c);
        }
    }
    (on_stage, off_stage)
}

/// 用默认预算装配上下文。
pub fn assemble_writing_context(
    conn: &Connection,
    novel: &Novel,
    current_chapter_id: Option<&str>,
    agent_system_prompt: &str,
) -> Result<AssembledContext, rusqlite::Error> {
    assemble_writing_context_with_budget(
        conn,
        novel,
        current_chapter_id,
        agent_system_prompt,
        DEFAULT_CONTEXT_BUDGET_CHARS,
    )
}

/// 按给定字符预算装配上下文。预算参数暴露出来是为了能写"预算不超标"的测试。
pub fn assemble_writing_context_with_budget(
    conn: &Connection,
    novel: &Novel,
    current_chapter_id: Option<&str>,
    agent_system_prompt: &str,
    budget_chars: usize,
) -> Result<AssembledContext, rusqlite::Error> {
    let mut parts: Vec<String> = Vec::new();
    let mut used = 0usize;

    // 1. 作品基本信息（最高优先级：没有它模型不知道自己在写什么）
    let mut basic = format!("【作品名称】：《{}》", novel.title);
    if !novel.synopsis.is_empty() {
        basic.push_str(&format!("\n【全篇大纲梗概】：\n{}", novel.synopsis));
    }
    push_capped(&mut parts, &mut used, budget_chars, &basic);

    // 2. 当前章正文（写作用的核心依据）
    let mut current_plain = String::new();
    if let Some(ch_id) = current_chapter_id {
        if let Ok(Some(ch)) = crate::db::chapter::get_chapter(conn, ch_id) {
            current_plain = ch.content_plain.clone();
            let head = format!(
                "【当前所在章节】：第《{}》\n【本章纲要摘要】：{}\n【本章当前正文】：\n{}",
                ch.title, ch.summary, ch.content_plain
            );
            push_capped(&mut parts, &mut used, budget_chars, &head);
        }
    }

    // 3. 角色：按是否在本章出场选择，而不是按排序取前 N 条
    let characters = crate::db::settings::list_characters(conn, &novel.id)?;
    if !characters.is_empty() {
        let (on_stage, off_stage) = split_by_presence(characters, &current_plain);

        if !on_stage.is_empty() {
            let mut s = String::from("【本章出场角色卡（完整档案）】：\n");
            for c in &on_stage {
                s.push_str(&format!(
                    "- 【{}】(别名: {}): {}\n",
                    c.name,
                    c.aliases,
                    truncate_chars(&c.profile, MAX_SINGLE_ENTRY_CHARS)
                ));
            }
            push_capped(&mut parts, &mut used, budget_chars, &s);
        }

        // 未出场角色降级为名单：仍提供"这个人存在"的线索，但不挤占档案预算。
        if !off_stage.is_empty() {
            let mut s = String::from("【其他已登场角色（仅名单，本章未出现）】：\n");
            for c in &off_stage {
                s.push_str(&format!("- 【{}】(别名: {})\n", c.name, c.aliases));
            }
            push_capped(&mut parts, &mut used, budget_chars, &s);
        }
    }

    // 4. 待回收伏笔
    let plots = crate::db::settings::list_plots(conn, &novel.id)?;
    let active_plots: Vec<_> = plots.into_iter().filter(|p| p.status == "planted").collect();
    if !active_plots.is_empty() {
        let mut s = String::from("【待回收伏笔与关键事件】：\n");
        for p in &active_plots {
            s.push_str(&format!(
                "- [{}] {}: {}\n",
                p.kind,
                p.title,
                truncate_chars(&p.content, MAX_SINGLE_ENTRY_CHARS)
            ));
        }
        push_capped(&mut parts, &mut used, budget_chars, &s);
    }

    // 5. 世界观（优先级最低，预算不够时最先被挤压）
    let worldview_list = crate::db::settings::list_worldview(conn, &novel.id)?;
    if !worldview_list.is_empty() {
        let mut s = String::from("【核心世界观与设定法则】：\n");
        for wv in &worldview_list {
            s.push_str(&format!(
                "- [{}] {}: {}\n",
                wv.group_name,
                wv.title,
                truncate_chars(&wv.content, MAX_SINGLE_ENTRY_CHARS)
            ));
        }
        push_capped(&mut parts, &mut used, budget_chars, &s);
    }

    let full_context = parts.join("\n\n");
    let combined_system = format!(
        "{}\n\n【必须遵循的硬性小说设定依据如下（不得编造与设定矛盾的事实）】：\n{}",
        agent_system_prompt, full_context
    );

    Ok(AssembledContext {
        system_prompt: combined_system,
        context_text: full_context,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        // 按字符而非字节截断：中文不会被切成半个字符
        assert_eq!(truncate_chars("一二三四五", 3), "一二…");
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abc", 0), "");
    }

    #[test]
    fn character_presence_matches_name_and_alias() {
        let aliases = r#"["小沈","沈师兄"]"#;
        assert!(character_appears("沈砚", aliases, "沈砚走进大殿。"));
        assert!(character_appears("沈砚", aliases, "小沈没有说话。"));
        assert!(character_appears("沈砚", aliases, "沈师兄来了。"));
        assert!(!character_appears("沈砚", aliases, "李四走进大殿。"));
        // 正文为空时不应命中（避免"空串 contains 空串"这类退化）
        assert!(!character_appears("沈砚", aliases, ""));
    }

    #[test]
    fn alias_json_is_tolerated_when_dirty() {
        assert!(parse_aliases("not json").is_empty());
        assert!(parse_aliases("").is_empty());
        assert_eq!(parse_aliases(r#"["a"]"#), vec!["a".to_string()]);
    }
}
