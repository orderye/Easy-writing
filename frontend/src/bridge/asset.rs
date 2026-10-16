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
  // 已是可直接使用的 URL 时原样返回（例如用户粘贴的外链）
  if (/^(https?:|data:|asset:|blob:)/i.test(relPath)) return relPath;
  const base = (appDataDir || "").replace(//+$/, "");
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
"#)]
extern "C" {
    fn asset_rel_to_url(rel_path: &str, app_data_dir: &str) -> String;
}

/// 把相对路径转为 asset URL。`app_data_dir` 为空时退化为原路径。
pub fn resolve_asset_url(rel_path: &str, app_data_dir: &str) -> String {
    asset_rel_to_url(rel_path, app_data_dir)
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
