use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Novel {
    pub id: String,
    pub title: String,
    pub synopsis: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Volume {
    pub id: String,
    pub novel_id: String,
    pub title: String,
    pub summary: String,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chapter {
    pub id: String,
    pub novel_id: String,
    pub volume_id: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub content_plain: String,
    pub word_count: i32,
    pub status: String, // "draft" | "editing" | "finalized"
    pub extracted_hash: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterHeader {
    pub id: String,
    pub novel_id: String,
    pub volume_id: String,
    pub title: String,
    pub summary: String,
    pub word_count: i32,
    pub status: String,
    pub sort_order: i32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldviewEntry {
    pub id: String,
    pub novel_id: String,
    pub group_name: String,
    pub title: String,
    pub content: String,
    pub origin: String, // "manual" | "ai"
    pub source_chapter_id: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Character {
    pub id: String,
    pub novel_id: String,
    pub name: String,
    pub aliases: String, // JSON array string
    pub profile: String, // JSON object string
    pub origin: String, // "manual" | "ai"
    pub source_chapter_id: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlotItem {
    pub id: String,
    pub novel_id: String,
    pub title: String,
    pub content: String,
    pub kind: String,   // "mainline" | "conflict" | "foreshadow"
    pub status: String, // "planted" | "resolved"
    pub chapter_id: Option<String>,
    pub origin: String, // "manual" | "ai"
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub key: String, // "worldview" | "character" | "plot" | "outline" | "volume_chapter" | "writing_assistant"
    pub name: String,
    pub system_prompt: String,
    pub llm_profile_id: Option<String>,
    pub params: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmProfile {
    pub id: String,
    pub name: String,
    pub protocol: String, // "openai" | "anthropic"
    pub base_url: String,
    pub key_ref: String,
    pub model: String,
    pub params: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractionCandidate {
    pub id: String,
    pub novel_id: String,
    pub chapter_id: String,
    pub agent_id: String,
    pub kind: String,   // "worldview" | "character" | "plot" | "chapter_summary"
    pub action: String, // "create" | "update"
    pub target_id: Option<String>,
    pub payload: String,
    pub evidence: String,
    pub dedup_key: String,
    pub status: String, // "pending" | "accepted" | "ignored"
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SettingChange {
    pub id: String,
    pub novel_id: String,
    pub chapter_id: Option<String>,
    pub target_type: String,
    pub target_id: String,
    pub before_content: String,
    pub after_content: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebdavConfig {
    pub id: String,
    pub url: String,
    pub username: String,
    pub pwd_ref: String,
    pub remote_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSetting {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub conflict_point: String,
    pub setting_module: String,
    pub setting_title: String,
    pub reason: String,
    pub proposed_change: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupFileInfo {
    pub filename: String,
    pub size_bytes: u64,
    pub modified_at: String,
}
