//! Tiptap（ProseMirror）JSON → 导出用的内部文档模型。
//!
//! ## 为什么不在 Rust 侧重新实现一遍 ProseMirror
//!
//! 正文在库里是 Tiptap 的 JSON（`js/src/tiptap-bridge.js:473-478` 存的是
//! `editor.getJSON()`）。导出要把这份 JSON 变成 txt / docx / epub，
//! 绕不开的问题是「这段 JSON 到底有多少字」——
//! 应用内的字数口径来自 Tiptap 的 `CharacterCount.characters()`，
//! 只要导出侧算出来的数和应用里显示的不一样，作者就会认为导出坏了。
//!
//! `characters()` 的实现是：
//!
//! ```js
//! node.textBetween(0, node.content.size, undefined, ' ')
//! ```
//!
//! 按 prosemirror-model 的 `textBetween` 语义（`blockSeparator` 为 `undefined`、
//! `leafText` 为字符串 `' '`），它的含义是：
//!
//! - **不插入任何块分隔符**；
//! - 每个**叶节点**（图片 / hardBreak / horizontalRule）贡献 1 个字符；
//! - 文本节点贡献它的全部字符。
//!
//! 于是：
//!
//! > **应用字数 = 所有文本节点的字符数之和 + 叶节点个数**
//!
//! 注意这与 `content_plain` 的长度**不是一回事** ——
//! `editor.getText()` 的默认块分隔符是 `'\n\n'`（`@tiptap/core` 的 `getText`），
//! 每两个块之间会多出 2 个字符。所以导出侧必须自己按上面的规则算，
//! 不能拿 `content_plain.len()` 当字数。

use serde_json::Value;

/// 导出格式之外的行内内容。
///
/// 只保留导出真正需要的样式：加粗 / 斜体 / 下划线。
/// 其余 mark（颜色、字号、行内代码）在三种目标格式里都没有稳定对应，
/// 强行映射只会让 txt 里出现一堆乱码似的标记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        underline: bool,
    },
}

impl Inline {
    /// 不含任何样式的纯文本。
    pub fn plain(&self) -> &str {
        match self {
            Inline::Text { text, .. } => text,
        }
    }
}

/// 块级内容。
///
/// `Image` 只带图片的**引用**（`src`），真正的字节由
/// [`crate::export::assets`] 解析后交给写出器 —— 这样写出器不必知道
/// 应用数据目录在哪，也便于把同一张图只读一次。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    /// 引用块里的一个段落。
    Quote {
        inlines: Vec<Inline>,
    },
    /// 列表项（有序 / 无序共用，靠 `ordered` 区分）。
    ///
    /// `index` 是该项在所属列表中的序号（从 1 起）——
    /// 有序列表要按序号编号，无序列表忽略它。
    ListItem {
        ordered: bool,
        index: usize,
        inlines: Vec<Inline>,
    },
    Image {
        src: String,
        alt: String,
    },
    /// 分割线。
    Rule,
}

/// 一章的解析结果。
#[derive(Debug, Clone)]
pub struct ChapterDoc {
    pub blocks: Vec<Block>,
    /// 应用口径字数（见模块注释）。
    pub char_count: i64,
    /// 正文的实际字符数（含段落分隔与硬换行），仅用于导出结果里的口径对照。
    pub text_chars: i64,
    /// 是否走了「纯文本降级」：正文不是 Tiptap JSON（历史导入数据）。
    pub degraded: bool,
}

/// 节点类型 → 结构类别。
///
/// 之所以要显式列出而不是「有 content 就当容器」：
/// ProseMirror 里**空段落**的 JSON 是 `{"type":"paragraph"}`（没有 content），
/// 按 content 判断会把它误当成叶节点而多算 1 字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    /// 文本节点。
    Text,
    /// 叶节点：贡献 1 个字符（图片 / 硬换行 / 分割线）。
    Leaf,
    /// 容器节点：递归子节点。
    Container,
}

fn kind_of(node_type: &str) -> NodeKind {
    match node_type {
        "text" => NodeKind::Text,
        "image" | "hardBreak" | "horizontalRule" => NodeKind::Leaf,
        // 未知类型按容器处理：少算总比多算好，且我们只支持本应用自己的 schema。
        _ => NodeKind::Container,
    }
}

/// 按应用口径统计字数：文本字符数之和 + 叶节点个数。
///
/// 见模块注释里对 `CharacterCount.characters()` 的拆解。
pub fn tiptap_char_count(doc: &Value) -> i64 {
    count_node(doc)
}

fn count_node(node: &Value) -> i64 {
    let Some(map) = node.as_object() else {
        return 0;
    };
    let node_type = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match kind_of(node_type) {
        NodeKind::Text => map
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.chars().count() as i64)
            .unwrap_or(0),
        NodeKind::Leaf => 1,
        NodeKind::Container => map
            .get("content")
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().map(count_node).sum())
            .unwrap_or(0),
    }
}

/// 解析一章正文。
///
/// `content` 是 Tiptap JSON；解析失败（非 JSON，或 JSON 里没有可识别的节点）
/// 时降级为纯文本：用 `content_plain` 按空行切段。
/// 降级会在导出结果里被点名（`degraded_chapters`），
/// 因为这类章节导出的是没有层级与图片的纯文本，作者有权知道。
pub fn parse_chapter(content: &str, content_plain: &str) -> ChapterDoc {
    let parsed: Option<Value> = serde_json::from_str(content).ok();
    let usable = parsed
        .as_ref()
        .map(|v| v.get("type").and_then(|t| t.as_str()) == Some("doc"))
        .unwrap_or(false);

    if !usable {
        let blocks = plain_text_to_blocks(content_plain);
        let text_chars = blocks
            .iter()
            .map(block_text)
            .map(|s| s.chars().count() as i64)
            .sum();
        // 降级章节的口径与导入时写入的 word_count 一致：非空白字符数。
        let char_count = content_plain
            .chars()
            .filter(|c| !c.is_whitespace())
            .count() as i64;
        return ChapterDoc {
            blocks,
            char_count,
            text_chars,
            degraded: true,
        };
    }

    let doc = parsed.expect("usable 为真时 parsed 必然有值");
    let blocks = parse_blocks(doc.get("content").and_then(|c| c.as_array()));
    let text_chars = blocks
        .iter()
        .map(block_text)
        .map(|s| s.chars().count() as i64)
        .sum();
    ChapterDoc {
        blocks,
        char_count: tiptap_char_count(&doc),
        text_chars,
        degraded: false,
    }
}

/// 纯文本 → 段落块（降级路径）。
fn plain_text_to_blocks(plain: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    for para in plain.split("\n\n") {
        let trimmed = para.trim_matches(|c| c == '\n' || c == '\r');
        if trimmed.trim().is_empty() {
            continue;
        }
        blocks.push(Block::Paragraph {
            inlines: vec![Inline::Text {
                text: trimmed.to_string(),
                bold: false,
                italic: false,
                underline: false,
            }],
        });
    }
    blocks
}

/// 解析 doc 的子节点为块序列。
fn parse_blocks(children: Option<&Vec<Value>>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let Some(children) = children else {
        return blocks;
    };
    for node in children {
        parse_node(node, &mut blocks);
    }
    blocks
}

fn parse_node(node: &Value, out: &mut Vec<Block>) {
    let Some(map) = node.as_object() else {
        return;
    };
    let node_type = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match node_type {
        "paragraph" => {
            let (inlines, images) = parse_inlines(map.get("content"));
            if !inlines.is_empty() {
                out.push(Block::Paragraph { inlines });
            }
            out.extend(images);
        }
        "heading" => {
            let level = map
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1)
                .clamp(1, 6) as u8;
            let (inlines, images) = parse_inlines(map.get("content"));
            if !inlines.is_empty() {
                out.push(Block::Heading { level, inlines });
            }
            out.extend(images);
        }
        "blockquote" => {
            let children = map.get("content").and_then(|c| c.as_array());
            let Some(children) = children else {
                return;
            };
            for child in children {
                let child_type = child.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if child_type == "paragraph" {
                    let (inlines, _) = parse_inlines(child.get("content"));
                    if !inlines.is_empty() {
                        out.push(Block::Quote { inlines });
                    }
                }
            }
        }
        "bulletList" | "orderedList" => {
            let ordered = node_type == "orderedList";
            let children = map.get("content").and_then(|c| c.as_array());
            let Some(children) = children else {
                return;
            };
            let mut item_index = 0usize;
            for item in children {
                let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if item_type != "listItem" {
                    continue;
                }
                item_index += 1;
                let item_children = item.get("content").and_then(|c| c.as_array());
                let Some(item_children) = item_children else {
                    continue;
                };
                for para in item_children {
                    let para_type = para.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if para_type != "paragraph" {
                        continue;
                    }
                    let (inlines, images) = parse_inlines(para.get("content"));
                    if !inlines.is_empty() {
                        out.push(Block::ListItem {
                            ordered,
                            index: item_index,
                            inlines,
                        });
                    }
                    out.extend(images);
                }
            }
        }
        "codeBlock" => {
            let (inlines, _) = parse_inlines(map.get("content"));
            if !inlines.is_empty() {
                out.push(Block::Paragraph { inlines });
            }
        }
        "image" => {
            let attrs = map.get("attrs");
            let src = attrs
                .and_then(|a| a.get("src"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let alt = attrs
                .and_then(|a| a.get("alt"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            if !src.is_empty() {
                out.push(Block::Image { src, alt });
            }
        }
        "horizontalRule" => out.push(Block::Rule),
        // 其它容器（doc、未知类型）继续下钻。
        _ => {
            if let Some(children) = map.get("content").and_then(|c| c.as_array()) {
                for child in children {
                    parse_node(child, out);
                }
            }
        }
    }
}

/// 解析行内内容，返回（行内片段, 其中夹带的图片）。
///
/// 图片节点在 Tiptap 里默认是块级节点，但作者可以把它拖进行内；
/// 这里把它单独摘出来，避免行内片段里混进二进制引用。
fn parse_inlines(content: Option<&Value>) -> (Vec<Inline>, Vec<Block>) {
    let mut inlines = Vec::new();
    let mut images = Vec::new();
    let Some(content) = content else {
        return (inlines, images);
    };
    let Some(children) = content.as_array() else {
        return (inlines, images);
    };
    for node in children {
        let Some(map) = node.as_object() else {
            continue;
        };
        let node_type = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match node_type {
            "text" => {
                let text = map.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                let marks = map.get("marks").and_then(|m| m.as_array());
                let mut bold = false;
                let mut italic = false;
                let mut underline = false;
                if let Some(marks) = marks {
                    for mark in marks {
                        let mark_type = mark.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match mark_type {
                            "bold" | "strong" => bold = true,
                            "italic" | "em" => italic = true,
                            "underline" => underline = true,
                            _ => {}
                        }
                    }
                }
                inlines.push(Inline::Text {
                    text: text.to_string(),
                    bold,
                    italic,
                    underline,
                });
            }
            "hardBreak" => inlines.push(Inline::Text {
                text: "\n".to_string(),
                bold: false,
                italic: false,
                underline: false,
            }),
            "image" => {
                let attrs = map.get("attrs");
                let src = attrs
                    .and_then(|a| a.get("src"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let alt = attrs
                    .and_then(|a| a.get("alt"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                if !src.is_empty() {
                    images.push(Block::Image { src, alt });
                }
            }
            _ => {}
        }
    }
    (inlines, images)
}

/// 一个块的全部纯文本。
pub fn block_text(block: &Block) -> String {
    match block {
        Block::Heading { inlines, .. }
        | Block::Paragraph { inlines }
        | Block::Quote { inlines }
        | Block::ListItem { inlines, .. } => inlines.iter().map(|i| i.plain()).collect(),
        Block::Image { alt, .. } => alt.clone(),
        Block::Rule => String::new(),
    }
}

/// 整个文档的纯文本（段之间用空行分隔）。
pub fn blocks_to_plain_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(block_text)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
