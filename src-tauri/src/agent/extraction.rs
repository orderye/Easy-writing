use crate::ai::gateway::AiGateway;
use crate::ai::openai::ChatMessage;
use crate::db::DbPool;
use md5::Md5;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use shared_types::{Chapter, ExtractionCandidate, LlmProfile, Novel};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Deserialize, Debug)]
pub struct RawCandidate {
    pub kind: String,   // "worldview" | "character" | "plot" | "chapter_summary"
    pub action: String, // "create" | "update"
    pub title: String,
    pub payload: String,
    pub evidence: String,
    /// 模型声称要更新的既有条目 id（旧版没有这个字段，故 serde 默认 None）。
    #[serde(default)]
    pub target_id: Option<String>,
}

/// 既有条目索引：kind -> [(id, title, aliases_json)]。
pub type ExistingIndex = Vec<(String, Vec<(String, String, String)>)>;

/// 归一化标题，用于「同一条目的不同写法」匹配。
///
/// 注意这**不是**去重哈希：哈希只能判等，无法把「李四（师兄）」这种
/// 带后缀的写法归到「李四」上。这里至少处理空白与大小写差异。
pub fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase().replace(' ', "")
}

/// 一个候选最终应当执行的动作。
#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedAction {
    pub action: String,
    pub target_id: Option<String>,
}

/// 把模型的 create/update 主张落到具体的既有条目上。
///
/// 规则优先级：
///   1. 模型给的 target_id 真的存在 → 更新它（模型最了解上下文）；
///   2. 标题（或既有别名）归一化后与某条目相等 → 更新它；
///   3. 否则 → 新建。
///
/// 这样做的意义：旧实现直接采信模型的 action，而模型**看不到**已有设定库，
/// 于是「李四」和「李四（师兄）」会各自建一条独立角色，长期积累成脏数据。
pub fn resolve_candidate_action(
    claimed_action: &str,
    claimed_target_id: Option<&str>,
    title: &str,
    kind: &str,
    existing: &ExistingIndex,
) -> ResolvedAction {
    let entries = existing
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v.as_slice())
        .unwrap_or(&[]);

    // 1) 模型点了名，且该 id 确实存在于这个 kind 下
    if let Some(tid) = claimed_target_id {
        if entries.iter().any(|(id, _, _)| id == tid) {
            return ResolvedAction {
                action: "update".to_string(),
                target_id: Some(tid.to_string()),
            };
        }
    }

    // 2) 标题 / 别名规则匹配
    let key = normalize_title(title);
    if !key.is_empty() {
        for (id, existing_title, aliases_json) in entries {
            if normalize_title(existing_title) == key {
                return ResolvedAction {
                    action: "update".to_string(),
                    target_id: Some(id.clone()),
                };
            }
            let aliases: Vec<String> =
                serde_json::from_str(aliases_json).unwrap_or_default();
            if aliases.iter().any(|a| normalize_title(a) == key) {
                return ResolvedAction {
                    action: "update".to_string(),
                    target_id: Some(id.clone()),
                };
            }
        }
    }

    // 3) 兜底：新建，且不带上任何 target_id，避免悬挂引用
    let _ = claimed_action;
    ResolvedAction {
        action: "create".to_string(),
        target_id: None,
    }
}

/// 把既有设定库读成索引，供 prompt 注入与动作判定使用。
pub fn build_existing_index(
    conn: &rusqlite::Connection,
    novel_id: &str,
) -> Result<ExistingIndex, String> {
    let mut index: ExistingIndex = Vec::new();

    let chars = crate::db::settings::list_characters(conn, novel_id).map_err(|e| e.to_string())?;
    index.push((
        "character".to_string(),
        chars
            .into_iter()
            .map(|c| (c.id, c.name, c.aliases))
            .collect(),
    ));

    let wv = crate::db::settings::list_worldview(conn, novel_id).map_err(|e| e.to_string())?;
    index.push((
        "worldview".to_string(),
        wv.into_iter()
            .map(|w| (w.id, w.title, "[]".to_string()))
            .collect(),
    ));

    let plots = crate::db::settings::list_plots(conn, novel_id).map_err(|e| e.to_string())?;
    index.push((
        "plot".to_string(),
        plots
            .into_iter()
            .map(|p| (p.id, p.title, "[]".to_string()))
            .collect(),
    ));

    Ok(index)
}

/// 把某个 kind 的既有条目渲染成 prompt 片段。
fn render_existing_for_prompt(existing: &ExistingIndex, kind: &str) -> String {
    let entries = existing
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v.as_slice())
        .unwrap_or(&[]);

    if entries.is_empty() {
        return "（当前设定库中还没有这一类的条目）".to_string();
    }

    let mut s = String::new();
    for (id, title, aliases) in entries.iter().take(200) {
        s.push_str(&format!("- id={id} 标题={title} 别名={aliases}\n"));
    }
    s
}

pub fn compute_dedup_key(kind: &str, title: &str) -> String {
    let normalized = normalize_title(title);
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn compute_chapter_fingerprint(text: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn run_parallel_extraction(
    gateway: Arc<AiGateway>,
    pool: DbPool,
    novel: Novel,
    chapter: Chapter,
    profile: LlmProfile,
    api_key: String,
) -> Result<Vec<ExtractionCandidate>, String> {
    // 1. Check if chapter text has changed
    let current_hash = compute_chapter_fingerprint(&chapter.content_plain);
    if let Some(ref last_hash) = chapter.extracted_hash {
        if last_hash == &current_hash && !chapter.content_plain.trim().is_empty() {
            // Already extracted with same text, return pending
            let conn = pool.get().map_err(|e| e.to_string())?;
            return crate::db::extraction::list_pending_candidates(&conn, &novel.id)
                .map_err(|e| e.to_string());
        }
    }

    // 1.5 读既有设定库：既用于 prompt 注入，也用于动作判定
    let existing: ExistingIndex = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        build_existing_index(&conn, &novel.id)?
    };

    // 2. Concurrency limiter (Max 2 simultaneous agent requests)
    let semaphore = Arc::new(Semaphore::new(2));

    // Agent kinds: Worldview, Character, Plot, Chapter summary
    let kinds = vec![
        ("worldview", "世界观 Agent", "提炼正文中出现的全新时代背景、地理地点、世界运行规则或独特专有名词。"),
        ("character", "角色 Agent", "提炼正文中新登场的人物角色、称号外号、外貌体态、性格行为以及与其他角色的关系演变。"),
        ("plot", "剧情 Agent", "提炼正文中发生的核心戏剧冲突事件、新埋设的伏笔暗示，或某已知伏笔是否在本章获得回收。"),
        ("chapter_summary", "大纲 Agent", "为本章节生成一至两句高度精炼的单章核心梗概摘要（用作章纲）。"),
    ];

    let mut tasks = Vec::new();

    for (kind, name, instructions) in kinds {
        let sem = semaphore.clone();
        let gw = gateway.clone();
        let prof = profile.clone();
        let key = api_key.clone();
        let text = chapter.content_plain.clone();
        let ch_title = chapter.title.clone();

        // 让模型看得见已有条目，它才可能给出正确的 target_id
        let existing_block = render_existing_for_prompt(&existing, kind);

        let prompt = format!(
            "你是一个小说【{}】。\n\
             任务指令：{}\n\
             当前章节标题：《{}》\n\
             当前正文内容如下：\n{}\n\n\
             【设定库中已有的同类条目】（更新已有条目时，必须在 target_id 里填它的 id；\n\
             若正文讲的是全新的人/事/规则，则 action 填 create 且 target_id 留空）：\n\
             {}\n\
             请仔细阅读正文，提炼出结构化的候选条目。\n\
             请务必输出严格的 JSON 数组格式，不得输出其他字符：\n\
             [\n\
               {{\n\
                 \"kind\": \"{}\",\n\
                 \"action\": \"create 或 update\",\n\
                 \"target_id\": \"要更新的既有条目 id；新建则填 null\",\n\
                 \"title\": \"条目标题（如角色名、规则名或伏笔名）\",\n\
                 \"payload\": \"条目的完整具体描述文本\",\n\
                 \"evidence\": \"来自正文的原句证据片段\"\n\
               }}\n\
             ]\n\
             （若本章节中未出现值得提炼的内容，返回空数组 [] 即可）",
            name, instructions, ch_title, text, existing_block, kind
        );

        let task = tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
            let messages = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }];
            gw.call_sync(&prof, &key, None, messages).await
        });

        tasks.push((kind, task));
    }

    let mut extracted_candidates = Vec::new();

    for (_kind, task) in tasks {
        if let Ok(Ok(raw_json)) = task.await {
            let json_str = extract_json_array(&raw_json);
            if let Ok(items) = serde_json::from_str::<Vec<RawCandidate>>(&json_str) {
                let conn = pool.get().map_err(|e| e.to_string())?;
                for item in items {
                    // 模型看不到设定库时给的 action 不可信，这里按既有条目重新裁决
                    let resolved = resolve_candidate_action(
                        &item.action,
                        item.target_id.as_deref(),
                        &item.title,
                        &item.kind,
                        &existing,
                    );

                    let dedup_key = compute_dedup_key(&item.kind, &item.title);
                    // Check if already accepted or ignored
                    if let Ok(false) = crate::db::extraction::is_candidate_ignored_or_accepted(
                        &conn,
                        &novel.id,
                        &dedup_key,
                    ) {
                        if let Ok(cand) = crate::db::extraction::insert_candidate(
                            &conn,
                            &novel.id,
                            &chapter.id,
                            "agent-extractor",
                            &item.kind,
                            &resolved.action,
                            resolved.target_id.as_deref(),
                            &item.payload,
                            &item.evidence,
                            &dedup_key,
                        ) {
                            extracted_candidates.push(cand);
                        }
                    }
                }
            }
        }
    }

    // Update chapter extracted_hash
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "UPDATE chapter SET extracted_hash = ?1 WHERE id = ?2",
            rusqlite::params![current_hash, chapter.id],
        );
    }

    Ok(extracted_candidates)
}

fn extract_json_array(text: &str) -> String {
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if start <= end {
                return text[start..=end].to_string();
            }
        }
    }
    "[]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with_character() -> ExistingIndex {
        vec![(
            "character".to_string(),
            vec![(
                "char-1".to_string(),
                "李四".to_string(),
                r#"["四哥","李四公子"]"#.to_string(),
            )],
        )]
    }

    #[test]
    fn valid_target_id_wins() {
        let r = resolve_candidate_action(
            "update",
            Some("char-1"),
            "随便什么标题",
            "character",
            &index_with_character(),
        );
        assert_eq!(r.action, "update");
        assert_eq!(r.target_id.as_deref(), Some("char-1"));
    }

    #[test]
    fn dangling_target_id_falls_back_to_title_match() {
        // 模型给了个不存在的 id，但标题能对上既有条目 —— 仍应判为 update
        let r = resolve_candidate_action(
            "update",
            Some("does-not-exist"),
            "李四",
            "character",
            &index_with_character(),
        );
        assert_eq!(r.action, "update");
        assert_eq!(r.target_id.as_deref(), Some("char-1"));
    }

    #[test]
    fn title_and_alias_matching_is_whitespace_and_case_insensitive() {
        for title in ["李四", " 李四 ", "四哥", "李四公子"] {
            let r = resolve_candidate_action("create", None, title, "character", &index_with_character());
            assert_eq!(r.action, "update", "标题 {title} 应命中既有角色");
            assert_eq!(r.target_id.as_deref(), Some("char-1"));
        }
    }

    #[test]
    fn genuinely_new_entry_stays_create_without_dangling_reference() {
        let r = resolve_candidate_action(
            "update",
            Some("nope"),
            "王五",
            "character",
            &index_with_character(),
        );
        assert_eq!(r.action, "create");
        assert_eq!(r.target_id, None, "新建候选绝不能带上不存在的 target_id");
    }

    #[test]
    fn target_id_is_scoped_to_its_own_kind() {
        // 拿 role 的 id 去更新 worldview，必须无效
        let r = resolve_candidate_action(
            "update",
            Some("char-1"),
            "某条世界观",
            "worldview",
            &index_with_character(),
        );
        assert_eq!(r.action, "create");
        assert_eq!(r.target_id, None);
    }

    #[test]
    fn dedup_key_ignores_whitespace_and_case() {
        assert_eq!(
            compute_dedup_key("character", "李 四"),
            compute_dedup_key("character", "李四")
        );
    }
}
