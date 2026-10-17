use wasm_bindgen::prelude::*;

/// 记录一次桥接调用的结果。
///
/// 桥接是字符串约定的弱类型接口（返回 bool / String），失败时不会 panic、
/// 也不会被编译期拦住。包装一层日志，至少让「静默失败」在控制台里可定位 ——
/// 之前 set_content 返回 false 被三处调用点直接忽略，编辑器未挂载时切章
/// 会让用户看到空白编辑区，而且没有任何痕迹。
pub fn bridge_call(name: &str, ok: bool) -> bool {
    if !ok {
        web_sys::console::warn_1(&format!("[TiptapBridge] {name} 返回 false").into());
    }
    ok
}


#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn mount_editor(element_id: &str, initial_json: &str, on_update_cb: &js_sys::Function) -> bool;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn destroy_editor();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn get_content_json() -> String;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn get_content_plain() -> String;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn get_word_count() -> u32;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    fn set_content_raw(json_str: &str) -> bool;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    fn insert_at_cursor_raw(text: &str) -> bool;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    fn replace_selection_raw(text: &str) -> bool;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn on_selection_update(cb: &js_sys::Function);

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn get_selection_text() -> Option<String>;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn get_selection_coords() -> Option<String>;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn focus();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_bold();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_italic();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_underline();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_heading(level: u32);

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn set_text_align(alignment: &str);

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_bullet_list();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_ordered_list();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn toggle_blockquote();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn insert_horizontal_rule();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn insert_image(src: &str, alt: &str);

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn undo();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn redo();

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn set_lexicon(terms_json: &str);

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn set_ghost_completion_enabled(enabled: bool);

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn on_lexicon_term_accepted(cb: &js_sys::Function);
}

/// 装载正文。编辑器未挂载时返回 false（调用方必须处理，见 P1-3 的 pending 回放）。
pub fn set_content(json_str: &str) -> bool {
    bridge_call("set_content", set_content_raw(json_str))
}

pub fn insert_at_cursor(text: &str) -> bool {
    bridge_call("insert_at_cursor", insert_at_cursor_raw(text))
}

pub fn replace_selection(text: &str) -> bool {
    bridge_call("replace_selection", replace_selection_raw(text))
}
