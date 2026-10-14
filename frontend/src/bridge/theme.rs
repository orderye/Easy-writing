use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function apply_app_theme(theme) {
  const root = document.documentElement;
  try {
    localStorage.setItem('easywriting_theme', theme);
  } catch (e) {}

  let isDark = false;
  if (theme === 'dark') {
    isDark = true;
  } else if (theme === 'light') {
    isDark = false;
  } else {
    // system
    isDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  if (isDark) {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}

export function setup_theme_listener() {
  if (window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', (e) => {
      let stored = 'system';
      try {
        stored = localStorage.getItem('easywriting_theme') || 'system';
      } catch (err) {}
      if (stored === 'system') {
        if (e.matches) {
          document.documentElement.classList.add('dark');
        } else {
          document.documentElement.classList.remove('dark');
        }
      }
    });
  }
}

export function set_editor_style(fontSizePx, lineHeight) {
  const root = document.documentElement;
  if (fontSizePx && Number(fontSizePx) > 0) {
    root.style.setProperty('--editor-font-size', fontSizePx + 'px');
  }
  if (lineHeight && String(lineHeight).trim().length > 0) {
    root.style.setProperty('--editor-line-height', String(lineHeight));
  }
}
"#)]
extern "C" {
    pub fn apply_app_theme(theme: &str);
    pub fn setup_theme_listener();
    pub fn set_editor_style(font_size_px: u32, line_height: &str);
}
