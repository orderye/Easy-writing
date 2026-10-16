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
pub struct ChapterRevision {
    pub id: String,
    pub chapter_id: String,
    pub novel_id: String,
    pub title: String,
    pub content: String,
    pub content_plain: String,
    pub word_count: i32,
    pub revision_type: String, // "auto" | "manual" | "ai_before" | "ai_after" | "rollback_before"
    pub note: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterRevisionHeader {
    pub id: String,
    pub chapter_id: String,
    pub novel_id: String,
    pub title: String,
    pub word_count: i32,
    pub revision_type: String,
    pub note: String,
    pub created_at: String,
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

/// 生成参数：temperature、最大输出长度、是否流式（PRD §5.1 P0）。
/// 以 JSON 字符串形式持久化在 llm_profile.params 列中，缺省字段一律使用服务端/网关默认值。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GenerationParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl GenerationParams {
    pub fn parse_from_str(s: &str) -> Self {
        serde_json::from_str::<GenerationParams>(s).unwrap_or_default()
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// 该参数对象是否为空（未设置任何生成参数）。
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none() && self.max_tokens.is_none() && self.stream.is_none()
    }
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
    pub target_title: String,
    pub before_content: String,
    pub after_content: String,
    pub reason: String,
    pub is_reverted: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrashItem {
    pub id: String,
    pub novel_id: String,
    pub item_type: String, // "volume" | "chapter" | "worldview" | "character" | "plot"
    pub title: String,
    pub snippet: String,
    pub deleted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPreview {
    pub novel_title: String,
    pub total_word_count: i32,
    pub volumes: Vec<ImportVolumePreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportVolumePreview {
    pub title: String,
    pub chapters: Vec<ImportChapterPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportChapterPreview {
    pub title: String,
    pub content: String,
    pub content_plain: String,
    pub word_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterRelation {
    pub id: String,
    pub novel_id: String,
    pub from_char_id: String,
    pub to_char_id: String,
    pub relation_name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptTemplate {
    pub key: String,
    pub name: String,
    pub template: String,
    pub default_template: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExplodedProfile {
    pub core: CharacterCore,
    pub appearance: CharacterAppearance,
    pub voice: CharacterVoice,
    pub assets: Vec<CharacterAssetItem>,
    pub secret: CharacterSecret,
    pub faction: String,
    pub importance: i32, // 1(主角) ~ 5(龙套)
    pub raw_notes: String,
}

impl ExplodedProfile {
    pub fn parse_from_str(s: &str) -> Self {
        if let Ok(exploded) = serde_json::from_str::<ExplodedProfile>(s) {
            exploded
        } else {
            Self {
                raw_notes: s.to_string(),
                faction: "默认阵营".to_string(),
                importance: 2,
                ..Default::default()
            }
        }
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| self.raw_notes.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CharacterCore {
    pub desire: String,         // 渴望 (Want)
    pub inner_need: String,     // 真正所需 (Need)
    pub flaw: String,           // 心魔执念 (Lie / Ghost)
    pub principle: String,      // 行事底线
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CharacterAppearance {
    pub features: String,       // 核心辨识点
    pub clothing: String,       // 经典穿搭风格
    pub gestures: String,       // 习惯性小动作
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CharacterVoice {
    pub tone: String,           // 说话语调
    pub catchphrase: String,    // 经典口头禅
    pub triggers: String,       // 情绪失控雷区
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CharacterAssetItem {
    pub name: String,
    pub kind: String,           // 武器 / 信物 / 功法 / 资产
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CharacterSecret {
    pub identity: String,       // 隐藏身份
    pub weakness: String,       // 致命死穴
    pub secret: String,         // 不可告人的秘密
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphNodeDto {
    pub id: String,
    pub name: String,
    pub faction: String,
    pub importance: i32,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphLinkDto {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation_name: String,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LexiconEntry {
    pub id: String,
    pub novel_id: String,
    pub term: String,
    pub category: String, // "worldview" | "character" | "plot" | "corpus" | "custom"
    pub frequency: i32,
    pub source_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 图片资产元信息（编辑器插图 / 图库）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetInfo {
    pub id: String,
    pub novel_id: String,
    pub name: String,
    pub mime: String,
    /// 相对 APPDATA 的路径，形如 images/<sha256>.png
    pub rel_path: String,
    pub size: i64,
    pub hash: String,
    pub created_at: String,
}

/// 对话记录（PRD §4.1 P1：对话记录随作品保存，可按章节筛选）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSession {
    pub id: String,
    pub novel_id: String,
    pub chapter_id: Option<String>,
    pub title: String,
    pub message_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String, // "user" | "assistant"
    pub content: String,
    pub created_at: String,
}

/// 「拉取模型列表」返回的单个模型条目（PRD §5.1 P0）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteModel {
    pub id: String,
    #[serde(default)]
    pub owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AppUpdateAsset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_name: String,
    pub release_notes: String,
    pub published_at: String,
    pub html_url: String,
    pub download_url: String,
    pub assets: Vec<AppUpdateAsset>,
    pub is_prerelease: bool,
}
