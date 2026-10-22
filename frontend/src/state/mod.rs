use leptos::prelude::*;
use shared_types::{
    Chapter, ChapterHeader, ConflictInfo, DeviceStateInfo, ExtractionCandidate, Novel, PendingNotice,
    Volume,
};

#[derive(Clone, Copy, Debug)]
pub struct AppContext {
    pub current_novel: RwSignal<Option<Novel>>,
    pub novels_list: RwSignal<Vec<Novel>>,
    pub volumes_list: RwSignal<Vec<Volume>>,
    pub current_volume: RwSignal<Option<Volume>>,
    pub current_chapter: RwSignal<Option<Chapter>>,
    pub chapters_map: RwSignal<Vec<ChapterHeader>>,
    
    // UI Panel controls
    pub left_panel_open: RwSignal<bool>,
    pub right_panel_open: RwSignal<bool>,
    pub active_settings_tab: RwSignal<String>,
    pub ai_chat_open: RwSignal<bool>,
    
    // Editor status
    pub save_status: RwSignal<String>,
    pub word_count: RwSignal<u32>,
    pub is_editor_mounted: RwSignal<bool>,
    pub is_ghost_completion_enabled: RwSignal<bool>,
    pub lexicon_version: RwSignal<u32>,
    /// 每次编辑器内容变更自增。用于判定"保存期间是否又有新编辑"：
    /// 保存开始时记下序号，完成时若不相等，说明磁盘内容已经落后，
    /// 必须保持 dirty 而不是标记为已保存（见 `crate::save_state`）。
    pub edit_seq: RwSignal<u64>,
    /// 自动保存连续失败后的常驻告警文案。为 `None` 表示无告警。
    pub save_alert: RwSignal<Option<String>>,
    /// 编辑器尚未挂载时被 `set_content` 拒绝的正文，挂载后回放。
    ///
    /// 注意语义是**最后写入胜出**（不是队列）：挂载前连续切两次章时，
    /// 后一次会覆盖前一次 —— 这是刻意的，用户最终停留在哪一章就该显示哪一章。
    pub pending_editor_content: RwSignal<Option<String>>,

    // App Preferences & Theme
    pub theme: RwSignal<String>,
    pub font_size: RwSignal<u32>,
    pub line_height: RwSignal<String>,
    pub auto_save_interval: RwSignal<u32>,

    // Unsaved changes confirm modal
    pub pending_switch_chapter: RwSignal<Option<ChapterHeader>>,
    pub is_unsaved_modal_open: RwSignal<bool>,

    // AI & Modals state
    pub is_config_open: RwSignal<bool>,
    pub is_extract_modal_open: RwSignal<bool>,
    pub is_extracting: RwSignal<bool>,
    pub extraction_candidates: RwSignal<Vec<ExtractionCandidate>>,
    pub is_conflict_modal_open: RwSignal<bool>,
    pub current_conflict: RwSignal<Option<ConflictInfo>>,
    pub pending_conflict_text: RwSignal<Option<String>>,
    pub is_revisions_modal_open: RwSignal<bool>,
    pub is_setting_changes_modal_open: RwSignal<bool>,
    pub is_trash_modal_open: RwSignal<bool>,
    /// 作品管理器（新建 / 切换 / 删除作品）
    pub is_novel_manager_open: RwSignal<bool>,
    pub settings_search_query: RwSignal<String>,
    pub chapter_search_query: RwSignal<String>,
    pub chapter_status_filter: RwSignal<String>,
    pub is_import_modal_open: RwSignal<bool>,
    /// 图库（浏览本作品图片资产、把已入库图片插回正文）
    pub is_asset_gallery_open: RwSignal<bool>,
    pub is_update_modal_open: RwSignal<bool>,
    pub available_update: RwSignal<Option<shared_types::AppUpdateInfo>>,
    pub has_update_badge: RwSignal<bool>,
    pub is_checking_update: RwSignal<bool>,

    // 跨设备同步保护（任务书 T04）
    /// 本机设备身份与上次同步时间。
    pub device_state: RwSignal<Option<DeviceStateInfo>>,
    /// 上次退出时因跨设备冲突而跳过的提示，等作者处理。
    ///
    /// 退出应用时**不能**弹阻塞窗口（那会让「关掉应用」变成
    /// 「必须先处理冲突」），所以冲突被记在这里，下次启动再提示。
    pub pending_sync_notice: RwSignal<Option<PendingNotice>>,
    /// 设置弹窗当前标签页。提到全局是为了让「启动提示条」能跳到
    /// 「WebDAV 整库同步」页处理冲突。
    pub config_active_tab: RwSignal<String>,
}

impl AppContext {
    pub fn new() -> Self {
        Self {
            current_novel: RwSignal::new(None),
            novels_list: RwSignal::new(Vec::new()),
            volumes_list: RwSignal::new(Vec::new()),
            current_volume: RwSignal::new(None),
            current_chapter: RwSignal::new(None),
            chapters_map: RwSignal::new(Vec::new()),
            left_panel_open: RwSignal::new(true),
            right_panel_open: RwSignal::new(true),
            active_settings_tab: RwSignal::new("worldview".to_string()),
            ai_chat_open: RwSignal::new(false),
            save_status: RwSignal::new("saved".to_string()),
            word_count: RwSignal::new(0),
            is_editor_mounted: RwSignal::new(false),
            is_ghost_completion_enabled: RwSignal::new(true),
            lexicon_version: RwSignal::new(0),
            edit_seq: RwSignal::new(0),
            save_alert: RwSignal::new(None),
            pending_editor_content: RwSignal::new(None),

            theme: RwSignal::new("system".to_string()),
            font_size: RwSignal::new(16),
            line_height: RwSignal::new("1.8".to_string()),
            auto_save_interval: RwSignal::new(30),

            pending_switch_chapter: RwSignal::new(None),
            is_unsaved_modal_open: RwSignal::new(false),

            is_config_open: RwSignal::new(false),
            is_extract_modal_open: RwSignal::new(false),
            is_extracting: RwSignal::new(false),
            extraction_candidates: RwSignal::new(Vec::new()),
            is_conflict_modal_open: RwSignal::new(false),
            current_conflict: RwSignal::new(None),
            pending_conflict_text: RwSignal::new(None),
            is_revisions_modal_open: RwSignal::new(false),
            is_setting_changes_modal_open: RwSignal::new(false),
            is_trash_modal_open: RwSignal::new(false),
            is_novel_manager_open: RwSignal::new(false),
            settings_search_query: RwSignal::new(String::new()),
            chapter_search_query: RwSignal::new(String::new()),
            chapter_status_filter: RwSignal::new("all".to_string()),
            is_import_modal_open: RwSignal::new(false),
            is_asset_gallery_open: RwSignal::new(false),
            is_update_modal_open: RwSignal::new(false),
            available_update: RwSignal::new(None),
            has_update_badge: RwSignal::new(false),
            is_checking_update: RwSignal::new(false),
            device_state: RwSignal::new(None),
            pending_sync_notice: RwSignal::new(None),
            config_active_tab: RwSignal::new("llm".to_string()),
        }
    }
}
