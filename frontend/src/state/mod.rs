use leptos::prelude::*;
use shared_types::{Chapter, ChapterHeader, ConflictInfo, ExtractionCandidate, Novel, Volume};

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
    pub settings_search_query: RwSignal<String>,
    pub chapter_search_query: RwSignal<String>,
    pub chapter_status_filter: RwSignal<String>,
    pub is_import_modal_open: RwSignal<bool>,
    pub is_update_modal_open: RwSignal<bool>,
    pub available_update: RwSignal<Option<shared_types::AppUpdateInfo>>,
    pub has_update_badge: RwSignal<bool>,
    pub is_checking_update: RwSignal<bool>,
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
            settings_search_query: RwSignal::new(String::new()),
            chapter_search_query: RwSignal::new(String::new()),
            chapter_status_filter: RwSignal::new("all".to_string()),
            is_import_modal_open: RwSignal::new(false),
            is_update_modal_open: RwSignal::new(false),
            available_update: RwSignal::new(None),
            has_update_badge: RwSignal::new(false),
            is_checking_update: RwSignal::new(false),
        }
    }
}
