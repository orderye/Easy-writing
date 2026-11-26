use wasm_bindgen::prelude::*;

/// 把后端返回的图片相对路径（如 `images/abc.png`）转为 WebView 可加载的 URL。
///
/// Tauri 的 assetProtocol scope 配的是 `$APPDATA/images/**`，因此需要先拼出本地
/// 绝对路径、再经 `convertFileSrc` 转成 `asset://` URL 才能显示。
///
/// 变换全部在 JS 侧完成：WASM 侧拿不到 APPDATA 绝对路径，
/// 而 Tauri 的 path API 只在 JS 侧可用。拼接失败时原样返回，
/// 图片显示不出来但不影响正文编辑。
#[wasm_bindgen(inline_js = r#"
export function asset_rel_to_url(relPath, appDataDir) {
  if (!relPath) return "";
  if (relPath.startsWith("asset://local/")) {
    relPath = relPath.slice("asset://local/".length);
  }
  // 已是可直接使用的 URL 时原样返回（例如用户粘贴的外链，但不包含已脱敏的 asset://local/）
  if (/^(https?:|data:|blob:)/i.test(relPath) || (/^asset:/i.test(relPath) && !relPath.startsWith("asset://local/"))) return relPath;
  let base = appDataDir || "";
  while (base.endsWith("/")) {
    base = base.slice(0, -1);
  }
  const abs = relPath.startsWith("/") ? relPath : (base ? base + "/" + relPath : relPath);
  try {
    if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.convertFileSrc === "function") {
      return window.__TAURI__.core.convertFileSrc(abs);
    }
    if (window.__TAURI__ && typeof window.__TAURI__.convertFileSrc === "function") {
      return window.__TAURI__.convertFileSrc(abs);
    }
  } catch (e) {
    // 转换失败退回原值
  }
  return relPath;
}

export function canonicalize_asset_src(src) {
  if (!src) return "";
  if (/^(data:|blob:)/i.test(src)) return src;
  if (/^(https?:)/i.test(src) && !src.includes("asset.localhost")) return src;

  let rel = src;
  if (rel.startsWith("asset://local/")) {
    rel = rel.slice("asset://local/".length);
  }
  try {
    rel = decodeURIComponent(rel);
  } catch (e) {}
  // 只处理 images/ 相对路径；其他输入（外链、无法解码的路径）原样返回。
  const idx = rel.indexOf("images/");
  if (idx === -1) return src;

  // 把 images/ 之后的相对路径规范化：吃掉 "." 与空段，遇到 ".." 就回退一层；
  // 一旦回退越过 images/ 根，说明输入试图越界，返回空串让图片显示不出来但不放行。
  // 注意：这层只保证前端**不下发**越界 URL；asset 协议自身是否拦截由 Rust 侧
  // asset_protocol_scope 回归测试（Tauri Scope / SafePathBuf）覆盖。
  const segments = [];
  for (const seg of rel.slice(idx + "images/".length).split("/")) {
    if (!seg || seg === ".") continue;
    if (seg === "..") {
      if (segments.length === 0) return "";
      segments.pop();
    } else {
      segments.push(seg);
    }
  }
  return "asset://local/images/" + segments.join("/");
}
"#)]
extern "C" {
    fn asset_rel_to_url(rel_path: &str, app_data_dir: &str) -> String;
    fn canonicalize_asset_src(src: &str) -> String;
}

/// 把相对路径转为 asset URL。`app_data_dir` 为空时退化为原路径。
pub fn resolve_asset_url(rel_path: &str, app_data_dir: &str) -> String {
    asset_rel_to_url(rel_path, app_data_dir)
}

/// 把包含本机绝对路径的 asset URL 或运行时 URL 归一化为跨设备通用的 `asset://local/<rel_path>`。
#[allow(dead_code)]
pub fn to_canonical_asset_url(src: &str) -> String {
    canonicalize_asset_src(src)
}
#[wasm_bindgen(inline_js = r#"
export function copy_text_to_clipboard(text) {
  try {
    if (navigator.clipboard && typeof navigator.clipboard.writeText === "function") {
      navigator.clipboard.writeText(text);
      return true;
    }
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch (e) {
    return false;
  }
}
"#)]
extern "C" {
    fn copy_text_to_clipboard(text: &str) -> bool;
}

/// 复制文本到剪贴板（图库的「复制路径」用）。失败返回 false，不 panic。
pub fn copy_asset_path(text: &str) -> bool {
    copy_text_to_clipboard(text)
}
