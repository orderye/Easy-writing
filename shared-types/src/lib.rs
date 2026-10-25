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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NovelWithStats {
    pub id: String,
    pub title: String,
    pub synopsis: String,
    pub created_at: String,
    pub updated_at: String,
    pub volume_count: i64,
    pub chapter_count: i64,
    pub word_count: i64,
}

impl NovelWithStats {
    pub fn to_novel(&self) -> Novel {
        Novel {
            id: self.id.clone(),
            title: self.title.clone(),
            synopsis: self.synopsis.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            deleted_at: None,
        }
    }
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
    /// 本章视角角色（T11）。`None` = 未指定；角色被删除时数据库会自动置空。
    #[serde(default)]
    pub pov_character_id: Option<String>,
    /// 故事内时间（T11），自由文本，如「第三年·冬」。
    #[serde(default)]
    pub story_time: Option<String>,
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
    /// 本章视角角色（T11），右栏按其筛选用。
    #[serde(default)]
    pub pov_character_id: Option<String>,
    /// 故事内时间（T11），自由文本。
    #[serde(default)]
    pub story_time: Option<String>,
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
    pub pinned: bool,
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
    pub pinned: bool,
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
    /// 回收章节（V008）。此前「已回收」只是状态位，没有记录在哪一回收的；
    /// 伏笔看板据此算「距今章数」与一键跳回收章节。
    ///
    /// 与埋设章节（chapter_id）分开存储：埋设章由提炼或人工编辑确定，
    /// 回收章由作者在看板上选定。两者语义不同，不能共用一个列。
    pub resolved_chapter_id: Option<String>,
    pub origin: String, // "manual" | "ai"
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub pinned: bool,
    pub deleted_at: Option<String>,
}

/// 伏笔看板的一行（T10）。
///
/// 把 `plot_item` 的原始字段加上「全书章序位置」与「超期判定」后的展示结构：
/// 看板不需要作者算章距，后端按卷章排序算好。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForeshadowRow {
    pub id: String,
    pub novel_id: String,
    pub title: String,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub chapter_id: Option<String>,
    pub resolved_chapter_id: Option<String>,
    /// 埋设章在全书章序中的下标（0 起）。None = 埋设章缺失或已删。
    pub chapter_index: Option<usize>,
    /// 回收章在全书章序中的下标。None = 已回收但未指定章节。
    pub resolved_chapter_index: Option<usize>,
    /// 距今章数 = 全书末章下标 + 1 - plant_index。埋设在末章为 0，
    /// 埋设在倒数第二章为 1。跨卷天然正确，因为章序是「全书」序而非「卷内」序。
    pub chapters_since_plant: Option<usize>,
    /// 未回收且已超阈值（`foreshadow_overdue_chapters`）。
    pub overdue: bool,
    /// 关联角色名（按本名 + 别名在伏笔内容里匹配，T06 同口径但文本来源是伏笔内容）。
    pub related_characters: Vec<String>,
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
/// `context_window`（T06）：模型上下文窗口（token），用于推导写作上下文的字符预算。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GenerationParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
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
        self.temperature.is_none()
            && self.max_tokens.is_none()
            && self.stream.is_none()
            && self.context_window.is_none()
    }
}

/// 上下文装配清单（T06）：面板展示与实际发送保持一致的入选依据。
/// 每条目记录实体类型、id、标题、入选原因、估算 token 与是否入选；
/// `trimmed` 收录被裁剪（预算不足或用户取消勾选）的条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextManifestEntry {
    /// "novel" | "chapter" | "character" | "plot" | "worldview" | "lexicon"
    pub entity_type: String,
    pub entry_id: String,
    pub title: String,
    /// "core" | "recent_summary" | "pinned" | "presence" | "alias"
    /// | "active_plot" | "lexicon" | "worldview" | "listed"
    pub reason: String,
    pub token_count: usize,
    pub included: bool,
    pub content_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextManifest {
    pub budget_tokens: usize,
    pub used_tokens: usize,
    pub entries: Vec<ContextManifestEntry>,
    pub trimmed: Vec<ContextManifestEntry>,
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
    #[serde(default)]
    pub title: String,
    pub payload: String,
    pub evidence: String,
    pub dedup_key: String,
    pub status: String, // "pending" | "accepted" | "ignored"
    /// 疑似重复的锚点（T07）：既有角色 id 或另一条候选 id。
    /// `None` 表示未发现疑似同人。
    #[serde(default)]
    pub possible_duplicate_of: Option<String>,
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

/// 冲突涉及的某条具体设定（T08：关联设定要能定位到库内条目）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictSettingRef {
    /// `worldview` / `character` / `plot` —— 与上下文清单的 entity_type 同一口径。
    pub entity_type: String,
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub conflict_point: String,
    pub setting_module: String,
    pub setting_title: String,
    pub reason: String,
    pub proposed_change: Option<String>,
    /// 模型指出、并经库内校验的关联设定（最多 3 条；伪造的 id 会被丢弃）。
    #[serde(default)]
    pub related_settings: Vec<ConflictSettingRef>,
    /// 判定依据：设定或正文中的原句。
    #[serde(default)]
    pub evidence: String,
    /// 模型自评置信度 0–1。模型没给就是 `None` —— 不编一个数字来假装确定。
    #[serde(default)]
    pub confidence: Option<f32>,
    /// 「本章忽略此类提示」的稳定签名：`<entity_type>:<id>` 或 `module:<类型>`。
    #[serde(default)]
    pub ignore_signature: String,
    /// 忽略按钮的展示文案（如「角色·沈砚」）。
    #[serde(default)]
    pub ignore_label: String,
}

/// 可能受某条设定改动影响的章节（确定性匹配的结果，不涉及模型判断）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AffectedChapter {
    pub id: String,
    pub title: String,
    /// 该词在本章正文里出现的次数。
    pub hits: i64,
    /// 命中处的上下文片段，便于作者快速判断是否真的相关。
    pub snippet: String,
}

/// 全书搜索的一条命中（T09）。
///
/// 与 `AffectedChapter` 的区别：后者是「改某条设定后哪些章节受影响」的影响面扫描
/// （限定作品、按命中数排序）；这里是面向作者的通用全文搜索，带卷归属与多片段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterSearchHit {
    pub chapter_id: String,
    pub volume_id: String,
    pub title: String,
    /// 标题 + 正文的命中总次数。
    pub hits: i64,
    /// 最多 3 条命中片段，供结果列表预览。
    pub snippets: Vec<String>,
    /// 该章是否有正文（空章只可能命中标题，前端据此提示）。
    pub has_body: bool,
}

/// 批量替换的逐章预览（T09）：不写库，只报告「会改哪章、改几处」。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplacePreviewItem {
    pub chapter_id: String,
    pub title: String,
    pub hits: i64,
    pub snippets: Vec<String>,
    /// 正文不是合法 Tiptap doc（无法在文本节点上安全替换），前端应置灰该章。
    pub unsupported: bool,
}

/// 批量替换的执行结果（T09）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplaceReport {
    /// 本批次标识：整批撤销的钥匙，同时写入每章快照的 `batch_id`。
    pub batch_id: String,
    pub chapters_changed: u32,
    pub total_replacements: u32,
    /// 正文格式不受支持、被跳过的章节标题。
    pub skipped_chapters: Vec<String>,
}

/// 整批撤销的结果（T09）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UndoReplaceReport {
    pub batch_id: String,
    pub chapters_restored: u32,
    /// 快照缺失或还原失败的章节 id —— 如实上报，不假装成功。
    pub missing_chapters: Vec<String>,
}

/// 冲突检测的结果。
///
/// 存在的理由：以前只有 Option<ConflictInfo>，"没检测出来"和"没检测成功"
/// 都表现为 None —— 模型返回了无法解析的 JSON 时，上层收到的是"无冲突"，
/// 于是正文被当作干净的直接插入。这是**静默放行**，必须区分开。
///
/// T08 又拆出两种「不该弹窗」的情况，它们与「干净」语义不同，不能合并：
///   * `Suppressed` —— 确实判出冲突，但置信度低于作者设定的阈值；
///   * `Disabled`   —— 作者主动关掉了前置检查，这一轮根本没检测。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ConsistencyOutcome {
    /// 检测完成，未发现矛盾。
    Clean,
    /// 检测完成，发现矛盾。
    Conflict { info: ConflictInfo },
    /// 检测完成且发现矛盾，但置信度低于阈值：不打断写作，仅留痕。
    Suppressed { info: ConflictInfo, threshold: f32 },
    /// 作者关闭了前置检查：本次没有发起检测，也没有产生额外调用。
    Disabled,
    /// 检测**没有完成**（模型返回不可解析 / 调用失败 / 内容过短被跳过）。
    /// 调用方必须把它显示成"未完成检测"，而不是"未发现矛盾"。
    NotChecked { reason: String },
}

impl ConsistencyOutcome {
    /// 需要弹窗裁决的冲突。`Suppressed` 不算 —— 它按设计就不打扰作者。
    pub fn as_conflict(&self) -> Option<&ConflictInfo> {
        match self {
            ConsistencyOutcome::Conflict { info } => Some(info),
            _ => None,
        }
    }

    pub fn is_not_checked(&self) -> bool {
        matches!(self, ConsistencyOutcome::NotChecked { .. })
    }

    pub fn is_suppressed(&self) -> bool {
        matches!(self, ConsistencyOutcome::Suppressed { .. })
    }

    pub fn is_disabled(&self) -> bool {
        matches!(self, ConsistencyOutcome::Disabled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupFileInfo {
    pub filename: String,
    pub size_bytes: u64,
    pub modified_at: String,
}

/// 一次整库备份的结果。
///
/// 存在的理由：后端本来就有这些字段，但 command 层只把 `filename`
/// 透传给前端，于是「清单没传上去」「三张图没同步」「两个素材本地就缺」
/// 全部在边界上被丢掉，作者只看到一个「✓ 备份成功」。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupReport {
    pub filename: String,
    /// 备份包是否已通过大小校验与结构自检。未通过时不会轮转旧备份。
    pub verified: bool,
    /// 清单 sidecar 是否上传成功。为 false 时这份备份只能按历史无清单路径恢复。
    pub manifest_uploaded: bool,
    /// 文件存在但上传失败的图片文件名。
    pub image_upload_failures: Vec<String>,
    /// 数据库有记录、但备份时本地文件已缺失的素材 `rel_path`。
    pub missing_assets: Vec<String>,
    /// 本次轮转实际删除的旧备份数量。
    ///
    /// 保留这个数字是为了让作者看见「远端目录确实在被回收」——
    /// 清理是静默的，缺了这个数字就只剩一个看不出变化的备份列表。
    pub rotated_deleted: u32,
    /// 本次使用的远端保留策略。
    pub retention: RetentionPolicy,
    /// 备份来源身份 sidecar（`.meta.json`）是否上传成功。
    ///
    /// 为 false 时这份备份**没有**跨设备保护所需的来源信息：
    /// 备份本体依然可用，但别的设备无法据此判断「这份备份是谁写的」。
    pub meta_uploaded: bool,
}

impl BackupReport {
    /// 是否有任何需要告知作者的完整性缺口。
    pub fn has_gaps(&self) -> bool {
        !self.verified
            || !self.manifest_uploaded
            || !self.image_upload_failures.is_empty()
            || !self.missing_assets.is_empty()
    }
}

/// 远端备份保留策略。
///
/// 三个维度是**并集**关系：一份备份只要命中任意一维就会保留。
///
/// - `recent`：最近 N 份（按备份内嵌的 UTC 时间戳）
/// - `daily`：最近 N 个「有备份的自然日」中，每天的最新一份
/// - `weekly`：最近 N 个「有备份的 ISO 周」中，每周的最新一份
///
/// 三个维度同时为 0 是非法配置（等于「把所有备份都删掉」），
/// 后端在读取设置时会把它回落成默认值，前端也不允许提交全 0。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub recent: usize,
    pub daily: usize,
    pub weekly: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            recent: 20,
            daily: 7,
            weekly: 4,
        }
    }
}

/// 保留策略的 dry-run 预览结果。
///
/// 预览**不删除任何东西**，只回答「如果现在执行清理，会删掉哪些」。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionPreview {
    /// 本次预览使用的策略。
    pub policy: RetentionPolicy,
    /// 受本应用保留策略管理的备份总数。
    pub managed_count: usize,
    /// 策略执行后保留的备份数。
    pub keep_count: usize,
    /// 将被删除的备份文件名。
    pub to_delete: Vec<String>,
    /// 将被保留的备份文件名。
    pub kept: Vec<String>,
    /// 远端目录中不属于本应用备份命名范围的文件数。
    ///
    /// 它们的数量只用于让作者知道「策略管不到这些文件」，
    /// 本策略永远不会删除它们。
    pub protected_foreign: usize,
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
    /// 软删除时间；`None` 表示在库（图库可见），`Some` 表示已移除、可恢复。
    pub deleted_at: Option<String>,
}

/// 一张图片资产的引用规模，用于删除前的风险提示。
///
/// - `referencing_chapters`：正文里引用了该图的章节数（含已进回收站的章节，
///   因为恢复后仍会引用）；
/// - `referencing_revisions`：历史版本里引用了该图的条数（版本回退后会引用）；
/// - `shared_records`：同文件的**其他**资产记录数（内容哈希去重，一行不等于一文件）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageAssetUsage {
    pub referencing_chapters: i64,
    pub referencing_revisions: i64,
    pub shared_records: i64,
}

/// 磁盘上存在、但 asset 表没有任何记录（含软删除记录）引用的图片文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrphanImageFile {
    /// 相对 APPDATA 的路径，形如 images/<sha256>.png
    pub rel_path: String,
    pub size: i64,
    /// 是否仍被正文或历史版本引用：为真时**不可清理**（清了会当场裂图）。
    pub referenced_by_content: bool,
}

/// 图片存储对账结果。注意作用域是**全库**，不是单个作品：
/// 孤儿文件不属于任何作品（记录已被删除），无法按作品归属。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageScanReport {
    /// 磁盘有、表里无记录的文件（可回收的空间就在这里）
    pub orphan_files: Vec<OrphanImageFile>,
    /// 表里有（在库）记录、磁盘上却没有文件的记录（WebDAV 恢复未补齐的痕迹）
    pub missing_records: Vec<AssetInfo>,
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

/// 恢复过程中发现的单个不一致项。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreIssue {
    pub filename: String,
    pub reason: String,
}

/// 跨设备恢复后钥匙串中缺失的 API Key 引用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DanglingKeyInfo {
    /// 对应配置项 id（模型接口 id 或 `default_webdav`）。
    pub profile_id: String,
    /// 面向作者的名称。
    pub profile_name: String,
    /// 钥匙串引用键名（**不是**密钥内容）。
    pub key_ref: String,
    /// 密钥作用域：`llm_profile` 或 `webdav`。
    ///
    /// 带默认值是为了兼容旧结构：历史数据里没有这个字段时按模型接口处理。
    #[serde(default = "default_key_scope")]
    pub scope: String,
}

fn default_key_scope() -> String {
    "llm_profile".to_string()
}

/// 密钥体检结果中的一条缺失密钥。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissingKeyInfo {
    /// 密钥作用域：`llm_profile` 或 `webdav`。
    pub scope: String,
    /// 对应配置项 id。
    pub id: String,
    /// 面向作者的名称。
    pub name: String,
    /// 钥匙串引用键名（**不是**密钥内容）。
    pub key_ref: String,
}

/// 跨设备冲突信息。
///
/// 远端存在「来自其他设备、且比本机上次同步更新」的备份时返回，
/// 由作者决定「继续上传 / 先恢复 / 取消」。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupConflictInfo {
    pub local_device_id: String,
    pub local_device_name: String,
    pub remote_device_id: String,
    pub remote_device_name: String,
    pub remote_filename: String,
    pub remote_created_at: String,
    pub local_last_sync_at: Option<String>,
}

/// 退出自动同步时因冲突而跳过的提示。
///
/// 退出时**不能**弹阻塞窗口（那会把「关掉应用」变成「必须先处理冲突」），
/// 因此冲突被记在这里，等下次启动时再提示作者。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingNotice {
    /// 提示类型。目前只有 `backup_conflict`。
    pub kind: String,
    pub remote_filename: String,
    pub remote_device_id: String,
    pub remote_device_name: String,
    pub remote_created_at: String,
    pub detected_at: String,
}

/// 本机同步状态（供前端展示）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceStateInfo {
    pub device_id: String,
    pub device_name: String,
    /// 本机内容与远端对齐的时间点（RFC3339 UTC）；从未同步过时为空。
    pub last_sync_at: Option<String>,
    /// 上次退出时因冲突而跳过的提示。
    pub pending_notice: Option<PendingNotice>,
}

/// 备份清单的校验状态。
///
/// 三态而不是 bool，是因为「没有清单」与「清单校验没过」必须能被区分：
/// 前者是历史备份的正常兼容路径，后者必须拒绝覆盖数据库。
/// 旧实现把两者压成同一个 `Vec<String>` 提示，损坏的清单会被
/// 静默当成「历史无清单备份」放行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManifestCheck {
    /// 服务端明确返回「文件不存在」（HTTP 404），按历史备份兼容路径处理。
    Absent,
    /// 清单存在且与备份内容一致。
    Verified { asset_count: u64 },
}

impl ManifestCheck {
    pub fn is_absent(&self) -> bool {
        matches!(self, ManifestCheck::Absent)
    }
}

/// 恢复预检结果。
///
/// 这是「覆盖数据库之前」交给作者的全部事实，数据来源必须是已下载、
/// 已解压、已通过完整性检查的候选备份本身 —— 不是远端文件名，
/// 也不是未经校验的清单声明。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestorePreflight {
    pub filename: String,
    /// 候选备份压缩包的 SHA-256。执行阶段据此重新定位并重新校验候选文件，
    /// 使「确认 A 却恢复 B」在结构上不可能发生。
    ///
    /// 刻意用哈希而不是文件路径：前端传回路径等于交出「恢复任意本地文件」的能力。
    pub preflight_token: String,
    pub schema_version: i64,
    pub novel_count: i64,
    pub volume_count: i64,
    pub chapter_count: i64,
    pub word_count: i64,
    /// 候选备份结构完整但一部作品都没有。
    pub is_empty_library: bool,
    /// 本机当前库的作品数，用于判断是否会用空库覆盖非空库。
    pub local_novel_count: i64,
    pub local_is_empty: bool,
    /// 本机有作品、而候选备份是空库 —— 必须二次确认才允许覆盖。
    pub requires_empty_overwrite_confirm: bool,
    pub manifest: ManifestCheck,
    /// 候选备份是否通过 `PRAGMA integrity_check`。
    /// 预检能返回就意味着已通过；保留该字段是为了让前端能明确展示这一项，
    /// 而不是让作者去推断「没报错就是通过了」。
    pub integrity_ok: bool,
    /// 候选包压缩后字节数。
    pub compressed_size_bytes: u64,
    /// 备份数据库 `asset` 表记录的资产总数。
    pub asset_total: u64,
    /// 本机已存在且哈希与清单一致的资产数。
    pub asset_present_locally: u64,
    /// 本机缺失、需要在执行阶段从远端回填的资产数。
    pub asset_missing_locally: u64,
    /// 资产层面的可见问题（路径非法、重复、哈希格式错误、清单与库不一致等）。
    pub asset_problems: Vec<String>,
    /// 这份备份的来源设备标识（来自 meta sidecar；历史备份没有 meta 时为空）。
    pub source_device_id: Option<String>,
    /// 这份备份的来源设备名。
    pub source_device_name: Option<String>,
    /// 这份备份是否来自**其他设备**。
    ///
    /// 恢复一份别的机器写的备份，是跨设备切换的正常动作；
    /// 但作者必须看见这件事 —— 否则「我这台机器上的东西怎么变了」无从解释。
    pub is_from_other_device: bool,
    /// 预检过程中观察到的、不阻断恢复的提示。
    pub notes: Vec<String>,
}

/// 恢复的最终结局。四态互斥，不允许把「数据库覆盖成功」当成「整体恢复成功」。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestoreOutcome {
    /// 数据库、本地配置、全部图片资产均恢复成功。安全快照已清理。
    Success,
    /// 数据库与本地配置恢复成功，但部分素材缺失或哈希不符。安全快照保留。
    PartialAssets,
    /// 关键阶段失败，已从安全快照回滚，原数据库未被改变。安全快照保留。
    RolledBack,
    /// 关键阶段失败**且回滚也失败**。安全快照保留，必须人工介入。
    RollbackFailed,
}

impl RestoreOutcome {
    /// 数据库内容是否已经被替换成备份内容且未回滚。
    pub fn database_is_restored(&self) -> bool {
        matches!(self, RestoreOutcome::Success | RestoreOutcome::PartialAssets)
    }

    /// 是否达到允许清理安全快照的状态。
    ///
    /// 只有完全成功才清理：部分成功意味着素材仍不完整，
    /// 作者可能还要再恢复一次，此时快照是唯一的退路。
    pub fn allows_snapshot_cleanup(&self) -> bool {
        matches!(self, RestoreOutcome::Success)
    }
}

/// 一次恢复的结构化摘要。
///
/// 取代旧实现塞进 `issues[0]` 的「（恢复摘要）」伪条目 —— 把结论混进
/// 问题列表里，前端只能折叠成「另有 N 项异常」，作者看不到关键状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreSummary {
    pub schema_version: i64,
    pub novel_count: i64,
    pub volume_count: i64,
    pub chapter_count: i64,
    pub word_count: i64,
    pub is_empty_library: bool,
    pub manifest: ManifestCheck,
    /// 本地 WebDAV 连接配置与界面设置是否成功写回。
    pub config_restored: bool,
    /// 配置恢复失败的具体阶段与原因；成功时为空。
    /// 刻意不含凭据内容或钥匙串引用键名。
    pub config_error: String,
    /// 数据库覆盖是否成功。
    pub database_restored: bool,
    pub asset_total: u64,
    pub asset_restored: u64,
    pub asset_failed: u64,
    /// 安全快照路径。仅在 `allows_snapshot_cleanup` 时为空。
    pub snapshot_path: String,
    pub snapshot_retained: bool,
    /// 回滚失败时的原因；未发生回滚失败时为空。
    pub rollback_error: String,
}

/// WebDAV 恢复结果报告。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreReport {
    pub outcome: RestoreOutcome,
    pub summary: RestoreSummary,
    pub issues: Vec<RestoreIssue>,
    pub dangling_keys: Vec<DanglingKeyInfo>,
}

/// 每日写作统计（任务书 T11）：某作品在某自然日的净增字数。
///
/// 净增按保存时新旧字数差累加（允许负值）；导入与批量替换不计入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DailyStat {
    pub stat_date: String,
    pub net_words: i64,
}

/// 导出格式（任务书 T02）。///
/// 序列化用小写字符串（`"txt"` / `"docx"` / `"epub"`），
/// 与前端下拉框取值、文件扩展名一致。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Txt,
    Docx,
    Epub,
}

impl Default for ExportFormat {
    fn default() -> Self {
        ExportFormat::Txt
    }
}

impl ExportFormat {
    /// 文件扩展名（不含点）。
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Txt => "txt",
            ExportFormat::Docx => "docx",
            ExportFormat::Epub => "epub",
        }
    }
}

/// 段落分隔方式。
///
/// - `indent`：段首缩进两字（全角空格），段间不空行；
/// - `blank_line`：段首不缩进，段间空一行。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphSeparator {
    Indent,
    BlankLine,
}

impl Default for ParagraphSeparator {
    fn default() -> Self {
        ParagraphSeparator::Indent
    }
}

/// 一次导出的全部选项（任务书 T02）。
///
/// 范围语义：`volume_ids` 与 `chapter_ids` **都为空**表示整书；
/// 只给 `volume_ids` 表示导出这些卷下的全部章节；
/// 给了 `chapter_ids` 则只导出这些章（`volume_ids` 仅用于分组显示）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportOptions {
    pub novel_id: String,
    pub format: ExportFormat,
    pub volume_ids: Vec<String>,
    pub chapter_ids: Vec<String>,
    /// 卷标题模板。占位符：`{novel}` `{volume_index}` `{volume_title}`。
    pub volume_title_template: String,
    /// 章标题模板。占位符：`{novel}` `{volume_index}` `{volume_title}`
    /// `{chapter_index}` `{chapter_title}`。
    pub chapter_title_template: String,
    pub paragraph_separator: ParagraphSeparator,
    /// 是否把正文里的图片一并导出（txt 不支持，恒为跳过）。
    pub include_images: bool,
    /// 建议文件名（不含扩展名）；最终路径由系统保存对话框决定。
    pub file_name: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            novel_id: String::new(),
            format: ExportFormat::Txt,
            volume_ids: Vec::new(),
            chapter_ids: Vec::new(),
            volume_title_template: "{volume_index} {volume_title}".to_string(),
            chapter_title_template: "{chapter_index} {chapter_title}".to_string(),
            paragraph_separator: ParagraphSeparator::Indent,
            include_images: false,
            file_name: String::new(),
        }
    }
}

/// 导出进度事件载荷（事件名 `novel-export://progress`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportProgress {
    /// 已完成的章节数。
    pub done: u32,
    /// 参与导出的章节总数。
    pub total: u32,
    /// 正在处理的章节标题。
    pub current_title: String,
    /// 阶段：`preparing` / `writing` / `finalizing` / `done` / `cancelled`。
    pub phase: String,
}

/// 一次导出的结果摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportSummary {
    /// 最终落盘路径；被取消时为空。
    pub path: String,
    pub format: ExportFormat,
    pub volume_count: u32,
    pub chapter_count: u32,
    /// 导出内容的字数，**口径与编辑器状态栏一致**
    /// （文本字符数 + 叶节点数，图片计 1 字）。
    pub word_count: i64,
    /// 导出正文的实际字符数（含段落分隔），仅用于排查口径差异。
    pub exported_text_chars: i64,
    /// 成功写入的图片数。
    pub included_images: u32,
    /// 因格式不支持或素材缺失而跳过的图片数。
    pub skipped_images: u32,
    /// 正文不是 Tiptap JSON、只能按纯文本导出的章节标题。
    pub degraded_chapters: Vec<String>,
    /// 是否被用户取消。
    pub cancelled: bool,
    pub elapsed_ms: u64,
}
