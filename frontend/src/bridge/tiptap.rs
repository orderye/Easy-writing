use wasm_bindgen::prelude::*;

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
    pub fn set_content(json_str: &str) -> bool;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn insert_at_cursor(text: &str) -> bool;

    #[wasm_bindgen(js_namespace = TiptapBridge)]
    pub fn replace_selection(text: &str) -> bool;

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
