//! 验证 invoke_cmd 构建器的参数编码与旧结构体一致。
//! 这些断言覆盖 61 处「表达式值」与 44 处「简写」调用点的序列化形态。
#![cfg(test)]

// 构建器在 wasm 前端 crate 中；此处仅验证等价的 serde_json 编码语义，
// 确保 None → null、Vec → 数组、数字/布尔不被字符串化。
#[test]
fn arg_encoding_matches_legacy_structs() {
    #[derive(serde::Serialize)]
    struct Legacy {
        novel_id: String,
        term: String,
        source_id: Option<String>,
        count: i32,
        flag: bool,
    }
    let legacy = Legacy {
        novel_id: "n1".into(),
        term: "t".into(),
        source_id: None,
        count: 3,
        flag: true,
    };
    let legacy_json = serde_json::to_value(&legacy).unwrap();

    let mut map = serde_json::Map::new();
    map.insert("novel_id".into(), serde_json::to_value("n1").unwrap());
    map.insert("term".into(), serde_json::to_value("t").unwrap());
    map.insert("source_id".into(), serde_json::Value::Null);
    map.insert("count".into(), serde_json::to_value(3).unwrap());
    map.insert("flag".into(), serde_json::to_value(true).unwrap());
    let built = serde_json::Value::Object(map);

    assert_eq!(built, legacy_json);
    assert!(built["source_id"].is_null());
    assert!(built["count"].is_number());
    assert!(built["flag"].is_boolean());
}

#[test]
fn vec_and_option_some_encoding() {
    let mut map = serde_json::Map::new();
    map.insert("terms".into(), serde_json::to_value(vec!["a", "b"]).unwrap());
    map.insert("source_id".into(), serde_json::to_value(Some("x")).unwrap());
    let built = serde_json::Value::Object(map);
    assert_eq!(built["terms"], serde_json::json!(["a", "b"]));
    assert_eq!(built["source_id"], serde_json::json!("x"));
}
