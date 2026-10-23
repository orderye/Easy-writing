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
//!   2. **按相关性**而不是按排序取角色：先看本章正文（或用户指令）里出现了谁
//!      （名字或别名命中），出场角色给完整档案，未出场角色降级为只留名字，
//!      避免挤占预算；
//!   3. 优先级明确：作品梗概 > 当前章 > 最近章摘要 > 出场角色 > 待回收伏笔
//!      > 词库术语 > 未出场名单 > 世界观；
//!   4. **硬性设定恒入**（T06）：被标记为 `pinned` 的世界观 / 角色 / 伏笔始终
//!      带入，且在各自类别里排在最前，最不容易被预算挤掉；
//!   5. **透明可审计**（T06）：装配结果同时产出 `ContextManifest`，逐条目记录
//!      入选原因与估算 token，前端「本次上下文」面板与实际发送保持一致。
use rusqlite::Connection;
use shared_types::{ContextManifest, ContextManifestEntry, Novel};

pub struct AssembledContext {
    pub system_prompt: String,
    pub context_text: String,
    /// 本次装配的透明清单（T06）：条目、入选原因、估算 token 与被裁剪项。
    pub manifest: ContextManifest,
}

/// 默认字符预算。约等于 8k token 量级的中文，留出空间给对话历史和模型输出。
pub const DEFAULT_CONTEXT_BUDGET_CHARS: usize = 12_000;

/// 默认 token 预算：`llm_profile.params.context_window` 未设置时使用。
pub const DEFAULT_CONTEXT_BUDGET_TOKENS: usize = 8_000;

/// 分段之间的分隔符长度（两个换行）。
const SEP: usize = 2;

/// 单条设定内容的上限：一条超长条目不该吃掉整个预算。
const MAX_SINGLE_ENTRY_CHARS: usize = 900;

/// 最近 K 章摘要的条数（T06 选取规则）。
const RECENT_SUMMARY_COUNT: usize = 3;

/// 词库术语最多贡献的条数（按词频排序，避免高频词挤占预算）。
const MAX_LEXICON_ENTRIES: usize = 40;

/// 计入命中的最短词库术语长度：单字术语噪声太大，不作为命中依据。
const MIN_LEXICON_TERM_CHARS: usize = 2;

/// 这里历史上写死的角色条数上限。保留它只是为了在测试与文档里引用
/// "以前会漏掉多少条"，**不要**再用它做截断。
#[doc(hidden)]
pub const LEGACY_CHARACTER_TAKE: usize = 12;

// ---------- Token 估算（T06） ----------

/// 把文本估算为 token 数。中文先用字符系数法，可整体替换为真实分词器。
pub trait TokenEstimator {
    fn estimate_tokens(&self, text: &str) -> usize;
}

/// 字符系数估算器：`字符数 / 每 token 字符数`，向上取整。
/// 中文经验系数约 1.5 字符/token；英文偏小，系数可调。
pub struct CharCoefficientEstimator {
    pub chars_per_token: f64,
}

impl Default for CharCoefficientEstimator {
    fn default() -> Self {
        Self {
            chars_per_token: 1.5,
        }
    }
}

impl TokenEstimator for CharCoefficientEstimator {
    fn estimate_tokens(&self, text: &str) -> usize {
        let chars = text.chars().count();
        if chars == 0 {
            return 0;
        }
        (chars as f64 / self.chars_per_token).ceil() as usize
    }
}

// ---------- 装配选项 ----------

pub struct AssembleOptions<'a> {
    /// 字符预算：context_text 的总字符上限。
    pub budget_chars: usize,
    /// token 预算：由调用方按系数与字符预算对齐，仅记入 manifest 展示。
    pub budget_tokens: usize,
    /// 用户在面板上取消勾选的条目 id：不进 context_text，manifest 标记为未入选。
    pub excluded_entry_ids: Vec<String>,
    pub estimator: &'a dyn TokenEstimator,
}

impl<'a> AssembleOptions<'a> {
    pub fn new(
        budget_chars: usize,
        budget_tokens: usize,
        estimator: &'a dyn TokenEstimator,
    ) -> Self {
        Self {
            budget_chars,
            budget_tokens,
            excluded_entry_ids: Vec::new(),
            estimator,
        }
    }
}

// ---------- 内部装配器 ----------

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

/// 尝试把一段文本整组装进预算；放不下则整条放弃（返回 None），
/// 而不是截成残段 —— 半截的段落标题对模型没有意义，
/// 还会让清单把「只装进一半」的条目标成「已入选」误导作者。
/// 被放弃的条目由调用方记入 trimmed，前端清单里可见。
fn push_capped(
    parts: &mut Vec<String>,
    used: &mut usize,
    budget: usize,
    text: &str,
) -> Option<String> {
    if text.is_empty() || *used >= budget {
        return None;
    }
    let avail = budget - *used;
    if text.chars().count() > avail {
        return None;
    }
    *used += text.chars().count() + SEP;
    parts.push(text.to_string());
    Some(text.to_string())
}

/// 逐条目的装配状态：context_text 推进与 manifest 记录同步进行。
struct Assembler<'a> {
    parts: Vec<String>,
    used: usize,
    budget: usize,
    excluded: &'a [String],
    estimator: &'a dyn TokenEstimator,
    entries: Vec<ContextManifestEntry>,
    trimmed: Vec<ContextManifestEntry>,
}

impl<'a> Assembler<'a> {
    fn new(opts: &'a AssembleOptions<'a>) -> Self {
        Self {
            parts: Vec::new(),
            used: 0,
            budget: opts.budget_chars,
            excluded: &opts.excluded_entry_ids,
            estimator: opts.estimator,
            entries: Vec::new(),
            trimmed: Vec::new(),
        }
    }

    /// 装入一个内容条目。文本为空视为「无内容可带」，不产生 manifest 条目。
    ///
    /// `section_label` 是所属段落的前导标题（如「【本章出场角色卡…】：\n」），
    /// 只挂在该段第一个**成功装入**的条目上 —— 调用方在首次返回 true 后
    /// 应把后续条目的 label 传 None，避免标题重复。
    ///
    /// 返回该条目是否成功入选（装入 context_text）。
    fn push_entry(
        &mut self,
        entity_type: &str,
        entry_id: &str,
        title: &str,
        reason: &str,
        text: &str,
        section_label: Option<&str>,
    ) -> bool {
        if text.is_empty() {
            return false;
        }
        let body = match section_label {
            Some(label) => format!("{}{}", label, text),
            None => text.to_string(),
        };
        let full_chars = body.chars().count();
        let full_tokens = self.estimator.estimate_tokens(&body);

        let excluded = self.excluded.iter().any(|id| id.as_str() == entry_id);
        let included = !excluded
            && push_capped(&mut self.parts, &mut self.used, self.budget, &body)
                .is_some();

        let entry = ContextManifestEntry {
            entity_type: entity_type.to_string(),
            entry_id: entry_id.to_string(),
            title: title.to_string(),
            reason: reason.to_string(),
            token_count: full_tokens,
            included,
            content_chars: full_chars,
        };
        self.entries.push(entry.clone());
        if !included {
            self.trimmed.push(entry);
        }
        included
    }
}

// ---------- 角色命中 ----------

/// 解析 character.aliases（JSON 字符串数组）。解析失败按"无别名"处理，
/// 不能让一条脏数据把整个上下文装配打断。
fn parse_aliases(aliases_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(aliases_json).unwrap_or_default()
}

/// 角色在本章正文（或用户指令）中的命中方式。
enum PresenceMatch {
    None,
    /// 按本名命中。
    Name,
    /// 仅按别名命中。
    Alias,
}

/// 角色是否在本章正文中被提及（按本名或任一别名）。
#[cfg(test)]
fn character_appears(name: &str, aliases_json: &str, text: &str) -> bool {
    !matches!(match_presence(name, aliases_json, text), PresenceMatch::None)
}

/// 判定角色的命中方式：本名优先，别名次之。
fn match_presence(name: &str, aliases_json: &str, text: &str) -> PresenceMatch {
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

// ---------- 装配入口 ----------

/// 用默认预算装配上下文。
pub fn assemble_writing_context(
    conn: &Connection,
    novel: &Novel,
    current_chapter_id: Option<&str>,
    agent_system_prompt: &str,
) -> Result<AssembledContext, rusqlite::Error> {
    let estimator = CharCoefficientEstimator::default();
    let opts = AssembleOptions::new(
        DEFAULT_CONTEXT_BUDGET_CHARS,
        DEFAULT_CONTEXT_BUDGET_TOKENS,
        &estimator,
    );
    assemble_writing_context_with_options(
        conn,
        novel,
        current_chapter_id,
        agent_system_prompt,
        "",
        &opts,
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
    let estimator = CharCoefficientEstimator::default();
    let opts = AssembleOptions::new(
        budget_chars,
        tokens_for_chars(budget_chars),
        &estimator,
    );
    assemble_writing_context_with_options(
        conn,
        novel,
        current_chapter_id,
        agent_system_prompt,
        "",
        &opts,
    )
}

/// 字符预算按缺省系数（1.5 字符/token）反推 token 预算，仅用于 manifest 展示。
fn tokens_for_chars(chars: usize) -> usize {
    (chars as f64 / 1.5).ceil() as usize
}

/// 完整装配入口（T06）：按优先级逐条目装入预算，同步产出透明清单。
///
/// `prompt` 是用户本轮输入的指令，与当前章正文合并作为角色命中与
/// 词库命中的依据（任务书：角色按「本章正文或用户指令」命中）。
pub fn assemble_writing_context_with_options(
    conn: &Connection,
    novel: &Novel,
    current_chapter_id: Option<&str>,
    agent_system_prompt: &str,
    prompt: &str,
    opts: &AssembleOptions<'_>,
) -> Result<AssembledContext, rusqlite::Error> {
    let mut asm = Assembler::new(opts);

    // 命中文本 = 当前章正文 + 用户指令
    let mut current_plain = String::new();
    if let Some(ch_id) = current_chapter_id {
        if let Ok(Some(ch)) = crate::db::chapter::get_chapter(conn, ch_id) {
            current_plain = ch.content_plain.clone();
        }
    }
    let hit_text = if prompt.is_empty() {
        current_plain.clone()
    } else if current_plain.is_empty() {
        prompt.to_string()
    } else {
        format!("{}\n{}", current_plain, prompt)
    };

    // 1. 作品基本信息（最高优先级：没有它模型不知道自己在写什么）
    let mut basic = format!("【作品名称】：《{}》", novel.title);
    if !novel.synopsis.is_empty() {
        basic.push_str(&format!("\n【全篇大纲梗概】：\n{}", novel.synopsis));
    }
    asm.push_entry("novel", &novel.id, &novel.title, "core", &basic, None);

    // 2. 当前章正文（写作用的核心依据）
    if let Some(ch_id) = current_chapter_id {
        if let Ok(Some(ch)) = crate::db::chapter::get_chapter(conn, ch_id) {
            let head = format!(
                "【当前所在章节】：第《{}》\n【本章纲要摘要】：{}\n【本章当前正文】：\n{}",
                ch.title, ch.summary, ch.content_plain
            );
            asm.push_entry("chapter", &ch.id, &ch.title, "core", &head, None);
        }
    }

    // 3. 最近 K 章摘要（T06：始终带入，帮助模型衔接前文）
    if let Some(ch_id) = current_chapter_id {
        if let Ok(headers) =
            crate::db::chapter::list_all_chapter_headers_for_novel(conn, &novel.id)
        {
            if let Some(pos) = headers.iter().position(|h| h.id == ch_id) {
                // 取当前章之前、摘要非空的最近 K 章，恢复成时间顺序（远→近）
                let mut recent: Vec<_> = headers[..pos]
                    .iter()
                    .filter(|h| !h.summary.is_empty())
                    .rev()
                    .take(RECENT_SUMMARY_COUNT)
                    .collect();
                recent.reverse();
                let mut label: Option<&str> = Some("【最近章节摘要】：\n");
                for h in recent {
                    let text = format!(
                        "- 第《{}》：{}\n",
                        h.title,
                        truncate_chars(&h.summary, MAX_SINGLE_ENTRY_CHARS)
                    );
                    if asm.push_entry(
                        "chapter",
                        &h.id,
                        &h.title,
                        "recent_summary",
                        &text,
                        label,
                    ) {
                        label = None;
                    }
                }
            }
        }
    }

    // 4. 角色：pinned 恒入；其余按本名/别名命中分出场/未出场
    let characters = crate::db::settings::list_characters(conn, &novel.id)?;
    if !characters.is_empty() {
        let mut pinned_chars = Vec::new();
        let mut on_stage: Vec<(&shared_types::Character, &str)> = Vec::new();
        let mut off_stage = Vec::new();
        for c in &characters {
            if c.pinned {
                pinned_chars.push(c);
                continue;
            }
            match match_presence(&c.name, &c.aliases, &hit_text) {
                PresenceMatch::Name => on_stage.push((c, "presence")),
                PresenceMatch::Alias => on_stage.push((c, "alias")),
                PresenceMatch::None => off_stage.push(c),
            }
        }

        // 出场角色卡（完整档案，pinned 排在最前）
        let mut stage_label: Option<&str> = Some("【本章出场角色卡（完整档案）】：\n");
        for c in &pinned_chars {
            let text = format!(
                "- 【{}】(别名: {}): {}\n",
                c.name,
                c.aliases,
                truncate_chars(&c.profile, MAX_SINGLE_ENTRY_CHARS)
            );
            if asm.push_entry("character", &c.id, &c.name, "pinned", &text, stage_label) {
                stage_label = None;
            }
        }
        for (c, reason) in on_stage {
            let text = format!(
                "- 【{}】(别名: {}): {}\n",
                c.name,
                c.aliases,
                truncate_chars(&c.profile, MAX_SINGLE_ENTRY_CHARS)
            );
            if asm.push_entry("character", &c.id, &c.name, reason, &text, stage_label) {
                stage_label = None;
            }
        }

        // 未出场角色降级为名单：仍提供"这个人存在"的线索，但不挤占档案预算。
        let mut list_label: Option<&str> = Some("【其他已登场角色（仅名单，本章未出现）】：\n");
        for c in &off_stage {
            let text = format!("- 【{}】(别名: {})\n", c.name, c.aliases);
            if asm.push_entry("character", &c.id, &c.name, "listed", &text, list_label) {
                list_label = None;
            }
        }
    }

    // 5. 待回收伏笔（pinned 恒入；其余按 planted 状态）
    let plots = crate::db::settings::list_plots(conn, &novel.id)?;
    let mut pinned_plots = Vec::new();
    let mut active_plots = Vec::new();
    for p in &plots {
        if p.pinned {
            pinned_plots.push(p);
        } else if p.status == "planted" {
            active_plots.push(p);
        }
    }
    let mut plot_label: Option<&str> = Some("【待回收伏笔与关键事件】：\n");
    for p in &pinned_plots {
        let text = format!(
            "- [{}] {}: {}\n",
            p.kind,
            p.title,
            truncate_chars(&p.content, MAX_SINGLE_ENTRY_CHARS)
        );
        if asm.push_entry("plot", &p.id, &p.title, "pinned", &text, plot_label) {
            plot_label = None;
        }
    }
    for p in &active_plots {
        let text = format!(
            "- [{}] {}: {}\n",
            p.kind,
            p.title,
            truncate_chars(&p.content, MAX_SINGLE_ENTRY_CHARS)
        );
        if asm.push_entry("plot", &p.id, &p.title, "active_plot", &text, plot_label) {
            plot_label = None;
        }
    }

    // 6. 词库术语：正文或指令中命中的术语（按词频排序，限条数）
    let lexicon = crate::db::lexicon::list_lexicon(conn, &novel.id)?;
    let mut lexicon_label: Option<&str> = Some("【相关词库术语】：\n");
    let mut lexicon_taken = 0usize;
    for term in &lexicon {
        if lexicon_taken >= MAX_LEXICON_ENTRIES {
            break;
        }
        if term.term.chars().count() < MIN_LEXICON_TERM_CHARS {
            continue;
        }
        if !hit_text.contains(term.term.as_str()) {
            continue;
        }
        lexicon_taken += 1;
        let text = format!("- [{}] {}\n", term.category, term.term);
        if asm.push_entry("lexicon", &term.id, &term.term, "lexicon", &text, lexicon_label) {
            lexicon_label = None;
        }
    }

    // 7. 世界观（优先级最低，预算不够时最先被挤压；pinned 排在最前）
    let worldview_list = crate::db::settings::list_worldview(conn, &novel.id)?;
    if !worldview_list.is_empty() {
        let mut pinned_wv = Vec::new();
        let mut normal_wv = Vec::new();
        for wv in &worldview_list {
            if wv.pinned {
                pinned_wv.push(wv);
            } else {
                normal_wv.push(wv);
            }
        }
        let mut wv_label: Option<&str> = Some("【核心世界观与设定法则】：\n");
        for wv in &pinned_wv {
            let text = format!(
                "- [{}] {}: {}\n",
                wv.group_name,
                wv.title,
                truncate_chars(&wv.content, MAX_SINGLE_ENTRY_CHARS)
            );
            if asm.push_entry("worldview", &wv.id, &wv.title, "pinned", &text, wv_label) {
                wv_label = None;
            }
        }
        for wv in &normal_wv {
            let text = format!(
                "- [{}] {}: {}\n",
                wv.group_name,
                wv.title,
                truncate_chars(&wv.content, MAX_SINGLE_ENTRY_CHARS)
            );
            if asm.push_entry("worldview", &wv.id, &wv.title, "worldview", &text, wv_label) {
                wv_label = None;
            }
        }
    }

    let context_text = asm.parts.join("\n\n");
    let used_tokens = asm.estimator.estimate_tokens(&context_text);

    let manifest = ContextManifest {
        budget_tokens: opts.budget_tokens,
        used_tokens,
        entries: asm.entries,
        trimmed: asm.trimmed,
    };

    let combined_system = format!(
        "{}\n\n【必须遵循的硬性小说设定依据如下（不得编造与设定矛盾的事实）】：\n{}",
        agent_system_prompt, context_text
    );

    Ok(AssembledContext {
        system_prompt: combined_system,
        context_text,
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 纯函数 ----------

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

    #[test]
    fn token_estimator_scales_by_coefficient() {
        let integer = CharCoefficientEstimator {
            chars_per_token: 1.0,
        };
        assert_eq!(integer.estimate_tokens("一百"), 2);
        let default = CharCoefficientEstimator::default();
        // 2 字符 / 1.5 → 向上取整为 2
        assert_eq!(default.estimate_tokens("一百"), 2);
        // 3 字符 / 1.5 → 恰好 2
        assert_eq!(default.estimate_tokens("一百字"), 2);
        assert_eq!(default.estimate_tokens(""), 0);
    }

    // ---------- 装配集成（内存库 fixture） ----------

    /// 建一个带 V001 + V004 形态的内存库（含 pinned 列与词库表）。
    fn fixture_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../migrations/V001__init.sql"))
            .unwrap();
        // V004 的 pinned 列（测试库不跑迁移阶梯，直接补列）
        conn.execute_batch(include_str!("../../migrations/V004__entry_pinned.sql"))
            .unwrap();
        // novel_lexicon 建表语句与 V002 一致
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS novel_lexicon (
                id          TEXT PRIMARY KEY,
                novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
                term        TEXT NOT NULL,
                category    TEXT NOT NULL DEFAULT 'custom',
                frequency   INTEGER NOT NULL DEFAULT 1,
                source_id   TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO novel (id, title, synopsis) VALUES ('n1', '测试之书', '一部关于沈砚的长篇测试作品')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO volume (id, novel_id, title, sort_order) VALUES ('v1', 'n1', '第一卷', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chapter (id, novel_id, volume_id, title, summary, content_plain, sort_order)
             VALUES ('c0', 'n1', 'v1', '序章', '序章摘要', '序章正文', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chapter (id, novel_id, volume_id, title, summary, content_plain, sort_order)
             VALUES ('c1', 'n1', 'v1', '第一章', '第一章摘要', '沈砚走进大殿。', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chapter (id, novel_id, volume_id, title, summary, content_plain, sort_order)
             VALUES ('c2', 'n1', 'v1', '第二章', '第二章摘要', '第二章正文', 2)",
            [],
        )
        .unwrap();
        // 沈砚（带别名），李四（未出场路人）
        conn.execute(
            "INSERT INTO character (id, novel_id, name, aliases, profile) VALUES
             ('char_shen', 'n1', '沈砚', '[\"小沈\",\"沈师兄\"]', '主角档案：沉稳的剑修'),
             ('char_li', 'n1', '李四', '[]', '路人档案')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO worldview_entry (id, novel_id, group_name, title, content) VALUES
             ('wv1', 'n1', '世界观', '法术体系', '灵气是修炼的根本'),
             ('wv2', 'n1', '世界观', '门派分布', '三大门派鼎立')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO plot_item (id, novel_id, title, content, kind, status) VALUES
             ('plot1', 'n1', '伏笔一', '沈砚的身世', 'foreshadow', 'planted'),
             ('plot2', 'n1', '伏笔二', '已回收的往事', 'foreshadow', 'resolved')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO novel_lexicon (id, novel_id, term, category, frequency) VALUES
             ('lex1', 'n1', '灵气', '术语', 10),
             ('lex2', 'n1', '剑修', '术语', 5),
             ('lex3', 'n1', '大', '术语', 1)",
            [],
        )
        .unwrap();
        conn
    }

    fn test_novel() -> Novel {
        Novel {
            id: "n1".to_string(),
            title: "测试之书".to_string(),
            synopsis: "一部关于沈砚的长篇测试作品".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            deleted_at: None,
        }
    }

    fn assemble(
        conn: &Connection,
        chapter_id: Option<&str>,
        prompt: &str,
        budget_chars: usize,
    ) -> AssembledContext {
        let estimator = CharCoefficientEstimator::default();
        let opts = AssembleOptions::new(
            budget_chars,
            tokens_for_chars(budget_chars),
            &estimator,
        );
        assemble_writing_context_with_options(&conn, &test_novel(), chapter_id, "", prompt, &opts)
            .unwrap()
    }

    #[test]
    fn pinned_entries_are_always_included() {
        let conn = fixture_conn();
        // 世界观与角色标记为 pinned：即使正文未命中也恒入
        conn.execute("UPDATE worldview_entry SET pinned = 1 WHERE id = 'wv2'", [])
            .unwrap();
        conn.execute("UPDATE character SET pinned = 1 WHERE id = 'char_li'", [])
            .unwrap();

        let ctx = assemble(&conn, Some("c1"), "", DEFAULT_CONTEXT_BUDGET_CHARS);
        // 李四未出场，但 pinned 恒入且给完整档案
        assert!(ctx.context_text.contains("路人档案"));
        // wv2 pinned 恒入
        assert!(ctx.context_text.contains("三大门派鼎立"));

        let pinned_char = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "char_li")
            .unwrap();
        assert_eq!(pinned_char.reason, "pinned");
        assert!(pinned_char.included);
        let pinned_wv = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "wv2")
            .unwrap();
        assert_eq!(pinned_wv.reason, "pinned");
        assert!(pinned_wv.included);
    }

    #[test]
    fn presence_and_listed_reasons_are_recorded() {
        let conn = fixture_conn();
        let ctx = assemble(&conn, Some("c1"), "", DEFAULT_CONTEXT_BUDGET_CHARS);
        // 沈砚本名命中第一章正文 → 完整档案
        let shen = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "char_shen")
            .unwrap();
        assert_eq!(shen.reason, "presence");
        assert!(shen.included);
        assert!(ctx.context_text.contains("主角档案：沉稳的剑修"));
        // 李四未命中 → 仅名单
        let li = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "char_li")
            .unwrap();
        assert_eq!(li.reason, "listed");
        assert!(li.included);
        assert!(ctx.context_text.contains("其他已登场角色"));
    }

    #[test]
    fn alias_match_via_prompt_records_alias_reason() {
        let conn = fixture_conn();
        // c2 正文（「第二章正文」）不含本名也不含别名，
        // 用户指令含别名「小沈」→ 别名命中。
        // （不能用 c1：其正文含本名「沈砚」，本名优先级高于别名，
        //  测的就不是别名路径了。）
        let ctx = assemble(&conn, Some("c2"), "请让小沈出场", DEFAULT_CONTEXT_BUDGET_CHARS);
        let shen = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "char_shen")
            .unwrap();
        assert_eq!(shen.reason, "alias");
        assert!(shen.included);
        assert!(ctx.context_text.contains("主角档案：沉稳的剑修"));
    }

    #[test]
    fn recent_summaries_and_lexicon_are_included() {
        let conn = fixture_conn();
        let ctx = assemble(&conn, Some("c2"), "", DEFAULT_CONTEXT_BUDGET_CHARS);
        // c2 之前有 c1、c0：最近章摘要带上
        assert!(ctx.context_text.contains("最近章节摘要"));
        assert!(ctx.context_text.contains("第一章摘要"));
        assert!(ctx.context_text.contains("序章摘要"));
        let c0 = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "c0")
            .unwrap();
        assert_eq!(c0.reason, "recent_summary");
        assert!(c0.included);

        // 词库：用户指令命中「灵气」「剑修」；单字术语「大」不计入
        let ctx2 = assemble(&conn, Some("c2"), "灵气灌注剑修", DEFAULT_CONTEXT_BUDGET_CHARS);
        assert!(ctx2.context_text.contains("相关词库术语"));
        assert!(ctx2.context_text.contains("灵气"));
        assert!(ctx2.context_text.contains("剑修"));
        let lex1 = ctx2
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "lex1")
            .unwrap();
        assert_eq!(lex1.reason, "lexicon");
        assert!(lex1.included);
        assert!(ctx2.manifest.entries.iter().all(|e| e.entry_id != "lex3"));
    }

    #[test]
    fn budget_trims_low_priority_segments_first() {
        let conn = fixture_conn();
        // 极小预算：只够作品信息 + 当前章 + 最近章摘要
        let ctx = assemble(&conn, Some("c2"), "", 200);
        assert!(ctx.context_text.contains("测试之书"));
        assert!(ctx.context_text.contains("第二章"));
        assert!(ctx.context_text.contains("最近章节摘要"));
        // 世界观优先级最低，必然被裁掉并记入 trimmed
        let wv = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "wv1")
            .unwrap();
        assert!(!wv.included);
        assert!(ctx.manifest.trimmed.iter().any(|e| e.entry_id == "wv1"));
    }

    #[test]
    fn excluded_entries_are_left_out_of_context() {
        let conn = fixture_conn();
        let estimator = CharCoefficientEstimator::default();
        let mut opts = AssembleOptions::new(
            DEFAULT_CONTEXT_BUDGET_CHARS,
            DEFAULT_CONTEXT_BUDGET_TOKENS,
            &estimator,
        );
        opts.excluded_entry_ids = vec!["wv1".to_string()];
        let ctx = assemble_writing_context_with_options(
            &conn,
            &test_novel(),
            Some("c2"),
            "",
            "",
            &opts,
        )
        .unwrap();
        let wv = ctx
            .manifest
            .entries
            .iter()
            .find(|e| e.entry_id == "wv1")
            .unwrap();
        assert!(!wv.included);
        assert!(!ctx.context_text.contains("法术体系"));
        assert!(ctx.manifest.trimmed.iter().any(|e| e.entry_id == "wv1"));
    }

    #[test]
    fn manifest_titles_match_context_text() {
        let conn = fixture_conn();
        let ctx = assemble(&conn, Some("c2"), "灵气", DEFAULT_CONTEXT_BUDGET_CHARS);
        // 面板清单与实际发送一致：入选条目的标题都能在 context_text 中找到
        for e in &ctx.manifest.entries {
            if e.included {
                assert!(
                    ctx.context_text.contains(e.title.as_str()),
                    "included entry title should appear in context: {} ({})",
                    e.title,
                    e.entry_id
                );
            }
        }
        assert!(ctx.manifest.used_tokens > 0);
        assert_eq!(
            ctx.manifest.budget_tokens,
            tokens_for_chars(DEFAULT_CONTEXT_BUDGET_CHARS)
        );
    }
}
