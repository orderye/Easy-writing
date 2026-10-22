//! 导出时的图片素材解析。
//!
//! 正文里的图片引用有三种形态（`js/src/tiptap-bridge.js:319-351`）：
//!
//! 1. `asset://local/images/<sha256>.<ext>` —— 本应用自己存的图，落盘在
//!    `<app_data_dir>/images/` 下，**扁平**存放（`src-tauri/src/asset.rs:52-55`）；
//! 2. `data:image/png;base64,...` —— 编辑器允许内联 base64（`Image.allowBase64`）；
//! 3. `http(s)://...` —— 外链图。导出是离线动作，**不联网抓取**，直接跳过。
//!
//! 解析结果按**原始 src** 缓存：同一张图在正文里出现多次也只读一次。

use base64::Engine;
use std::collections::HashMap;
use std::path::Path;

/// 一张已解析的图片素材。
#[derive(Debug, Clone)]
pub struct ImageAsset {
    /// 原始引用（正文里的 `src`）。
    pub src: String,
    pub bytes: Vec<u8>,
    /// 扩展名（不含点）：`png` / `jpg` / `webp` / `gif`。
    pub extension: String,
    /// MIME 类型。
    pub mime: String,
}

/// 图片素材缓存：原始 `src` → 素材。
#[derive(Debug, Default)]
pub struct AssetStore {
    assets: HashMap<String, ImageAsset>,
}

impl AssetStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取出（必要时解析并缓存）一张图片。
    ///
    /// 返回 `None` 表示这张图无法嵌入：外链图、指向已删素材的坏引用、
    /// 或文件读不出来。调用方负责把它计入「跳过张数」。
    pub fn resolve(&mut self, app_data_dir: &Path, src: &str) -> Option<&ImageAsset> {
        if self.assets.contains_key(src) {
            return self.assets.get(src);
        }
        let asset = load_image(app_data_dir, src)?;
        self.assets.insert(src.to_string(), asset);
        self.assets.get(src)
    }

    /// 已解析的图片数量。
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

/// 读取一张图片的字节。
fn load_image(app_data_dir: &Path, src: &str) -> Option<ImageAsset> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }

    if let Some(rest) = src.strip_prefix("data:") {
        return load_inline_data(rest);
    }

    if src.starts_with("http://") || src.starts_with("https://") {
        // 外链图不联网抓取：导出必须离线可完成。
        return None;
    }

    // 取 `images/` 之后的文件名。`asset://local/images/x.png` 与
    // `http://asset.localhost/images/x.png` 两种前缀都要覆盖。
    let file_name = src.rsplit("images/").next()?.trim();
    if file_name.is_empty() || !is_safe_file_name(file_name) {
        return None;
    }

    let path = app_data_dir.join("images").join(file_name);
    let bytes = std::fs::read(path).ok()?;
    let extension = extension_of(file_name);
    Some(ImageAsset {
        src: src.to_string(),
        bytes,
        mime: mime_of(&extension).to_string(),
        extension,
    })
}

/// 解析 `data:` URL 的负载部分（`image/png;base64,xxxx`）。
fn load_inline_data(rest: &str) -> Option<ImageAsset> {
    let (meta, payload) = rest.split_once(',')?;
    let mime = meta.trim_start_matches("data:");
    let mime = mime.split(';').next().unwrap_or("image/png").trim();
    let extension = match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;
    Some(ImageAsset {
        src: String::new(),
        bytes,
        mime: mime.to_string(),
        extension: extension.to_string(),
    })
}

/// 文件名只允许「哈希 + 扩展名」这类安全字符，杜绝 `../` 越权读取。
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// 从文件名取扩展名（不含点，小写）。
fn extension_of(file_name: &str) -> String {
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "png".to_string())
}

/// 扩展名 → MIME。
fn mime_of(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    }
}
