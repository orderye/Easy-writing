//! T09 替换引擎：只在 Tiptap JSON 的文本节点上替换。
//!
//! 这一组用例钉住的是替换最容易出事的三处：
//!   * **样式与图片不被破坏** —— 只改 `{"type":"text"}` 的 `text` 字段；
//!   * **跨节点命中** —— ProseMirror 会把一段文字在加粗/斜体处切成多个节点，
//!     命中横跨两个节点时逐节点替换会一个都命中不了；
//!   * **不留下空文本节点** —— `{"type":"text","text":""}` 会让前端 `setContent` 抛错。

use easy_writing_lib::replace::{replace_in_chapter, ReplaceOptions};
use serde_json::{json, Value};

fn text(t: &str) -> Value {
    json!({ "type": "text", "text": t })
}

fn bold(t: &str) -> Value {
    json!({ "type": "text", "text": t, "marks": [{ "type": "bold" }] })
}

fn italic(t: &str) -> Value {
    json!({ "type": "text", "text": t, "marks": [{ "type": "italic" }] })
}

fn para(children: Vec<Value>) -> Value {
    json!({ "type": "paragraph", "content": children })
}

fn doc(blocks: Vec<Value>) -> String {
    json!({ "type": "doc", "content": blocks }).to_string()
}

fn parse(content: &str) -> Value {
    serde_json::from_str(content).unwrap()
}

/// 收集全文（按文档序拼接所有 text 节点）。
fn joined_text(v: &Value) -> String {
    let mut out = String::new();
    fn walk(v: &Value, out: &mut String) {
        if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
            for child in arr {
                if child.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = child.get("text").and_then(|t| t.as_str()) {
                        out.push_str(t);
                    }
                }
                walk(child, out);
            }
        }
    }
    walk(v, &mut out);
    out
}

const OPTS: ReplaceOptions = ReplaceOptions {
    case_sensitive: false,
    all: true,
};

#[test]
fn replaces_inside_plain_text() {
    let content = doc(vec![para(vec![text("他握紧断水剑。")])]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    assert_eq!(out.replacements, 1);
    assert_eq!(joined_text(&parse(&out.content)), "他握紧青霜剑。");
}

#[test]
fn preserves_marks_of_the_node_it_writes_into() {
    let content = doc(vec![para(vec![
        text("他握紧"),
        bold("断水剑"),
        text("，剑光如雪。"),
    ])]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    let v = parse(&out.content);
    let children = v["content"][0]["content"].as_array().unwrap();
    // 加粗节点仍在，且仍带 bold mark —— 替换不能把样式抹掉
    let bold_node = children
        .iter()
        .find(|n| n["text"] == "青霜剑")
        .expect("替换文本应落在原加粗节点上");
    assert_eq!(
        bold_node["marks"],
        json!([{ "type": "bold" }]),
        "替换后的文本必须继承原节点的 marks"
    );
}

#[test]
fn preserves_image_nodes() {
    let img = json!({ "type": "image", "attrs": { "src": "asset://a.png", "alt": "插图" } });
    let content = doc(vec![
        para(vec![text("断水剑"), img.clone()]),
        para(vec![text("断水剑在此。")]),
    ]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    assert_eq!(out.replacements, 2);
    let v = parse(&out.content);
    let imgs: Vec<&Value> = v["content"][0]["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["type"] == "image")
        .collect();
    assert_eq!(imgs.len(), 1, "图片节点必须原样保留");
    assert_eq!(imgs[0], &img, "图片节点的 attrs 不得被改动");
}

#[test]
fn matches_across_node_boundary() {
    // 「断水」在普通节点、「剑」在加粗节点 —— 这是逐节点替换必然漏掉的情形，
    // 但 content_plain 里看得见「断水剑」，作者也会在预览里看到 1 处命中。
    let content = doc(vec![para(vec![text("他握紧断水"), bold("剑"), text("。")])]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    assert_eq!(out.replacements, 1, "跨节点命中必须算作一处");
    assert_eq!(joined_text(&parse(&out.content)), "他握紧青霜剑。");
}

#[test]
fn cross_node_replacement_inherits_first_node_marks() {
    // 跨节点时替换文本落在**首个**命中节点上，继承它的样式（这里无样式）。
    let content = doc(vec![para(vec![text("断水"), bold("剑"), text("。")])]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    let v = parse(&out.content);
    let children = v["content"][0]["content"].as_array().unwrap();
    assert_eq!(children[0]["text"], "青霜剑");
    assert!(
        children[0].get("marks").is_none(),
        "首个命中节点无样式，替换文本也不应凭空多出 marks"
    );
}

#[test]
fn removes_text_nodes_that_become_empty() {
    // 整节点被替换吃空时必须整条移除：ProseMirror 不接受空 text 节点。
    let content = doc(vec![para(vec![text("断水剑"), text("在此。")])]);
    let out = replace_in_chapter(&content, "断水剑", "", OPTS).unwrap();

    let v = parse(&out.content);
    let children = v["content"][0]["content"].as_array().unwrap();
    for child in children {
        if child["type"] == "text" {
            assert_ne!(
                child["text"], "",
                "不得留下空 text 节点，前端 setContent 会抛错"
            );
        }
    }
    assert_eq!(joined_text(&v), "在此。");
}

#[test]
fn count_is_zero_and_content_unchanged_when_nothing_matches() {
    let content = doc(vec![para(vec![text("风平浪静。")])]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    assert_eq!(out.replacements, 0);
    assert_eq!(out.content, content, "无命中时正文必须逐字不变");
}

#[test]
fn invalid_tiptap_json_is_rejected_rather_than_written_back() {
    // 拿 content_plain 硬替换再写回会写坏 JSON —— 宁可跳过该章并上报。
    assert!(replace_in_chapter("not json", "a", "b", OPTS).is_none());
    assert!(replace_in_chapter("<p>x</p>", "a", "b", OPTS).is_none());
    assert!(replace_in_chapter(r#"{"type":"paragraph"}"#, "a", "b", OPTS).is_none());
}

#[test]
fn word_count_and_plain_text_follow_editor_metrics() {
    let content = doc(vec![para(vec![text("断水剑在此。")])]);
    let before_plain = {
        let v = parse(&content);
        easy_writing_lib::export::doc::blocks_to_plain_text(
            &easy_writing_lib::export::doc::parse_chapter(&content, "").blocks,
        )
        .len()
    };
    let out = replace_in_chapter(&content, "断水剑", "青霜剑在此。", OPTS).unwrap();

    assert_eq!(out.content_plain, "青霜剑在此。在此。");
    assert!(out.word_count > 0);
    assert!(
        out.content_plain.len() > before_plain,
        "替换成更长的词后纯文本应变长"
    );
    // 口径一致性：直接用同一套导出函数重算，必须与 outcome 相等
    let recomputed = easy_writing_lib::export::doc::blocks_to_plain_text(
        &easy_writing_lib::export::doc::parse_chapter(&out.content, "").blocks,
    );
    assert_eq!(out.content_plain, recomputed);
}

#[test]
fn case_insensitive_by_default_but_respects_the_flag() {
    let content = doc(vec![para(vec![text("The Sword is broken.")])]);

    let insensitive = replace_in_chapter(&content, "sword", "blade", OPTS).unwrap();
    assert_eq!(insensitive.replacements, 1, "默认不区分大小写");

    let sensitive = replace_in_chapter(
        &content,
        "sword",
        "blade",
        ReplaceOptions {
            case_sensitive: true,
            all: true,
        },
    )
    .unwrap();
    assert_eq!(sensitive.replacements, 0, "区分大小写时不应命中 Sword");
}

#[test]
fn all_occurrences_in_one_chapter_are_replaced() {
    let content = doc(vec![
        para(vec![text("断水剑断了。")]),
        para(vec![text("他修好断水剑，断水剑又亮了。")]),
    ]);
    let out = replace_in_chapter(&content, "断水剑", "青霜剑", OPTS).unwrap();

    assert_eq!(out.replacements, 3);
    assert_eq!(
        joined_text(&parse(&out.content)),
        "青霜剑断了。他修好青霜剑，青霜剑又亮了。"
    );
}
