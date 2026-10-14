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
}

pub fn compute_dedup_key(kind: &str, title: &str) -> String {
    let normalized = title.trim().to_lowercase().replace(' ', "");
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

        let prompt = format!(
            "你是一个小说【{}】。\n\
             任务指令：{}\n\
             当前章节标题：《{}》\n\
             当前正文内容如下：\n{}\n\n\
             请仔细阅读正文，提炼出结构化的候选条目。\n\
             请务必输出严格的 JSON 数组格式，不得输出其他字符：\n\
             [\n\
               {{\n\
                 \"kind\": \"{}\",\n\
                 \"action\": \"create 或 update\",\n\
                 \"title\": \"条目标题（如角色名、规则名或伏笔名）\",\n\
                 \"payload\": \"条目的完整具体描述文本\",\n\
                 \"evidence\": \"来自正文的原句证据片段\"\n\
               }}\n\
             ]\n\
             （若本章节中未出现值得提炼的内容，返回空数组 [] 即可）",
            name, instructions, ch_title, text, kind
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
                            &item.action,
                            None,
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
