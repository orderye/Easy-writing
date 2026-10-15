use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn mount(canvas_id: &str, options_json: &str) -> bool;

    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn setData(canvas_id: &str, nodes_json: &str, links_json: &str, active_id: &str) -> bool;

    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn setActiveNode(canvas_id: &str, node_id: &str) -> bool;

    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn zoomIn(canvas_id: &str);

    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn zoomOut(canvas_id: &str);

    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn resetView(canvas_id: &str);

    #[wasm_bindgen(js_namespace = CharacterGraphBridge)]
    pub fn destroy(canvas_id: &str);
}
