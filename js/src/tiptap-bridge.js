import { Editor, Extension } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import CharacterCount from '@tiptap/extension-character-count';
import Color from '@tiptap/extension-color';
import TextStyle from '@tiptap/extension-text-style';
import TextAlign from '@tiptap/extension-text-align';
import Image from '@tiptap/extension-image';
import Underline from '@tiptap/extension-underline';
import Placeholder from '@tiptap/extension-placeholder';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

let activeEditor = null;
let updateCallback = null;
let selectionCallback = null;
let termAcceptedCallback = null;
let ghostCompletionEnabled = true;
let isComposing = false;

// -------------------------------------------------------------
// Lexicon Trie for Fast Prefix Autocomplete
// -------------------------------------------------------------
class LexiconTrie {
  constructor() {
    this.root = {};
    this.termsCount = 0;
  }

  clear() {
    this.root = {};
    this.termsCount = 0;
  }

  insert(term, category = 'custom', frequency = 1) {
    if (!term || typeof term !== 'string') return;
    const clean = term.trim();
    if (clean.length < 2) return;

    let node = this.root;
    for (const char of clean) {
      if (!node[char]) {
        node[char] = { children: {} };
      }
      node = node[char].children;
    }

    if (!node.__meta) {
      node.__meta = { term: clean, category, frequency };
      this.termsCount++;
    } else {
      if (frequency > node.__meta.frequency) {
        node.__meta.frequency = frequency;
      }
      if (category && category !== 'corpus') {
        node.__meta.category = category;
      }
    }
  }

  loadTerms(list) {
    this.clear();
    if (!list) return;
    const items = typeof list === 'string' ? JSON.parse(list) : list;
    if (!Array.isArray(items)) return;

    for (const item of items) {
      if (typeof item === 'string') {
        this.insert(item, 'custom', 1);
      } else if (item && item.term) {
        this.insert(item.term, item.category || 'custom', item.frequency || 1);
      }
    }
  }

  findBestMatch(prefix) {
    if (!prefix || prefix.length < 2) return null;

    let node = this.root;
    for (const char of prefix) {
      if (!node[char]) return null;
      node = node[char].children;
    }

    const candidates = [];
    function collect(curr) {
      if (!curr) return;
      if (curr.__meta) {
        candidates.push(curr.__meta);
      }
      for (const k of Object.keys(curr)) {
        if (k !== '__meta' && curr[k] && curr[k].children) {
          collect(curr[k].children);
        }
      }
    }
    collect(node);

    if (candidates.length === 0) return null;

    // Filter candidates strictly longer than the typed prefix
    const longer = candidates.filter(c => c.term.length > prefix.length && c.term.startsWith(prefix));
    if (longer.length === 0) return null;

    // Category weighting
    const categoryWeight = {
      worldview: 100,
      character: 90,
      plot: 80,
      custom: 70,
      corpus: 50,
    };

    longer.sort((a, b) => {
      const weightA = categoryWeight[a.category] || 50;
      const weightB = categoryWeight[b.category] || 50;
      if (weightA !== weightB) return weightB - weightA;
      if (a.frequency !== b.frequency) return b.frequency - a.frequency;
      return a.term.length - b.term.length;
    });

    const best = longer[0];
    const suffix = best.term.slice(prefix.length);
    return {
      term: best.term,
      suffix,
      category: best.category,
      frequency: best.frequency,
    };
  }

  findMatchFromPrecedingText(text, maxLookback = 10) {
    if (!text || text.length < 2) return null;
    const len = text.length;
    const limit = Math.min(len, maxLookback);

    for (let tryLen = limit; tryLen >= 2; tryLen--) {
      const candidatePrefix = text.slice(len - tryLen);
      // Skip if contains punctuation or whitespaces
      if (/[\s\r\n，。！？、“”‘’（）《》【】…—、；：:;,.!?'"()\[\]{}]/.test(candidatePrefix)) {
        continue;
      }
      const match = this.findBestMatch(candidatePrefix);
      if (match) {
        return {
          ...match,
          prefix: candidatePrefix,
          prefixLen: tryLen,
        };
      }
    }
    return null;
  }
}

const lexiconTrie = new LexiconTrie();
const ghostPluginKey = new PluginKey('ghostCompletionPlugin');

const GhostCompletionExtension = Extension.create({
  name: 'ghostCompletion',

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: ghostPluginKey,
        state: {
          init() {
            return { active: false, pos: 0, prefix: '', suffix: '', term: '' };
          },
          apply(tr, prev, oldState, newState) {
            const meta = tr.getMeta(ghostPluginKey);
            if (meta && meta.clear) {
              return { active: false, pos: 0, prefix: '', suffix: '', term: '' };
            }

            if (!ghostCompletionEnabled || isComposing) {
              return { active: false, pos: 0, prefix: '', suffix: '', term: '' };
            }

            const { selection } = newState;
            if (!selection.empty) {
              return { active: false, pos: 0, prefix: '', suffix: '', term: '' };
            }

            const $from = selection.$from;
            if (!$from.parent.isTextblock) {
              return { active: false, pos: 0, prefix: '', suffix: '', term: '' };
            }

            const textBefore = $from.parent.textBetween(0, $from.parentOffset, null, '\ufffc');
            const match = lexiconTrie.findMatchFromPrecedingText(textBefore);

            if (match && match.suffix) {
              return {
                active: true,
                pos: $from.pos,
                prefix: match.prefix,
                suffix: match.suffix,
                term: match.term,
              };
            }

            return { active: false, pos: 0, prefix: '', suffix: '', term: '' };
          },
        },
        props: {
          decorations(state) {
            const pluginState = ghostPluginKey.getState(state);
            if (
              pluginState &&
              pluginState.active &&
              pluginState.suffix &&
              state.selection.empty &&
              state.selection.$from.pos === pluginState.pos
            ) {
              const span = document.createElement('span');
              span.className = 'tiptap-ghost-text select-none pointer-events-none text-slate-400 dark:text-slate-500 opacity-60';
              span.textContent = pluginState.suffix;
              span.style.color = '#94a3b8';
              span.style.opacity = '0.65';
              span.style.userSelect = 'none';
              span.style.pointerEvents = 'none';
              span.style.fontStyle = 'normal';
              span.setAttribute('data-ghost-completion', pluginState.suffix);

              const widget = Decoration.widget(pluginState.pos, span, { side: 1 });
              return DecorationSet.create(state.doc, [widget]);
            }
            return DecorationSet.empty;
          },
          handleKeyDown(view, event) {
            const pluginState = ghostPluginKey.getState(view.state);
            if (pluginState && pluginState.active && pluginState.suffix) {
              // Press Tab to accept completion
              if (event.key === 'Tab' && !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey) {
                event.preventDefault();
                event.stopPropagation();

                const { suffix, term, pos } = pluginState;
                const tr = view.state.tr.insertText(suffix, pos);
                tr.setMeta(ghostPluginKey, { clear: true });
                view.dispatch(tr);

                if (typeof termAcceptedCallback === 'function') {
                  try {
                    termAcceptedCallback(term);
                  } catch (e) {
                    console.error('[TiptapBridge] Error in termAcceptedCallback:', e);
                  }
                }
                return true;
              }

              // Press Escape to dismiss
              if (event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                const tr = view.state.tr.setMeta(ghostPluginKey, { clear: true });
                view.dispatch(tr);
                return true;
              }
            }
            return false;
          },
          handleDOMEvents: {
            compositionstart(view) {
              isComposing = true;
              const tr = view.state.tr.setMeta(ghostPluginKey, { clear: true });
              view.dispatch(tr);
              return false;
            },
            compositionend(view) {
              isComposing = false;
              // Trigger state update right after IME composition finishes
              setTimeout(() => {
                if (view && !view.isDestroyed) {
                  const tr = view.state.tr;
                  view.dispatch(tr);
                }
              }, 30);
              return false;
            },
          },
        },
      }),
    ];
  },
});

export function on_selection_update(cb) {
  selectionCallback = cb;
}

export function on_lexicon_term_accepted(cb) {
  termAcceptedCallback = cb;
}

export function set_lexicon(termsJsonOrArray) {
  lexiconTrie.loadTerms(termsJsonOrArray);
}

export function set_ghost_completion_enabled(enabled) {
  ghostCompletionEnabled = !!enabled;
  if (!ghostCompletionEnabled && activeEditor && activeEditor.view) {
    const tr = activeEditor.view.state.tr.setMeta(ghostPluginKey, { clear: true });
    activeEditor.view.dispatch(tr);
  }
}

let cachedAppDataDir = '';

export function set_app_data_dir(dir) {
  cachedAppDataDir = dir || '';
}

export function get_app_data_dir() {
  return cachedAppDataDir;
}

export function resolve_asset_url(src) {
  if (!src) return '';
  if (src.startsWith('asset://local/')) {
    const rel = src.slice('asset://local/'.length);
    return convertRelToLocalUrl(rel, cachedAppDataDir);
  }
  const imgIdx = src.indexOf('images/');
  if (imgIdx !== -1 && (src.startsWith('asset:') || src.startsWith('http://asset.localhost') || src.startsWith('https://asset.localhost') || src.startsWith('/'))) {
    const rel = src.slice(imgIdx);
    return convertRelToLocalUrl(rel, cachedAppDataDir);
  }
  return src;
}

export function normalize_asset_url(src) {
  if (!src) return '';
  if (src.startsWith('asset://local/')) return src;
  if (src.startsWith('data:') || src.startsWith('blob:')) return src;
  if ((src.startsWith('http://') || src.startsWith('https://')) && !src.includes('asset.localhost')) {
    return src;
  }
  try {
    const decoded = decodeURIComponent(src);
    const imgIdx = decoded.indexOf('images/');
    if (imgIdx !== -1) {
      return 'asset://local/' + decoded.slice(imgIdx);
    }
  } catch (e) {}
  const imgIdx = src.indexOf('images/');
  if (imgIdx !== -1) {
    return 'asset://local/' + src.slice(imgIdx);
  }
  return src;
}

function convertRelToLocalUrl(relPath, appDataDir) {
  if (!relPath) return '';
  let base = appDataDir || '';
  while (base.endsWith('/')) {
    base = base.slice(0, -1);
  }
  const abs = relPath.startsWith('/') ? relPath : (base ? base + '/' + relPath : relPath);
  try {
    if (typeof window !== 'undefined' && window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.convertFileSrc === 'function') {
      return window.__TAURI__.core.convertFileSrc(abs);
    }
    if (typeof window !== 'undefined' && window.__TAURI__ && typeof window.__TAURI__.convertFileSrc === 'function') {
      return window.__TAURI__.convertFileSrc(abs);
    }
  } catch (e) {}
  return relPath;
}

function transformDocImages(node, transformSrc) {
  if (!node || typeof node !== 'object') return node;
  if (node.type === 'image' && node.attrs && node.attrs.src) {
    return {
      ...node,
      attrs: {
        ...node.attrs,
        src: transformSrc(node.attrs.src),
      },
    };
  }
  if (Array.isArray(node.content)) {
    return {
      ...node,
      content: node.content.map(child => transformDocImages(child, transformSrc)),
    };
  }
  return node;
}

export function mount_editor(elementId, initialJson, onUpdateCb) {
  const container = document.getElementById(elementId);
  if (!container) {
    console.error(`[TiptapBridge] Container element #${elementId} not found.`);
    return false;
  }

  if (activeEditor) {
    activeEditor.destroy();
    activeEditor = null;
  }

  updateCallback = onUpdateCb;

  let content = {};
  if (initialJson && initialJson.trim() !== '') {
    try {
      content = JSON.parse(initialJson);
      content = transformDocImages(content, resolve_asset_url);
    } catch (e) {
      console.warn('[TiptapBridge] Failed to parse initial JSON, fallback to empty:', e);
      content = {};
    }
  }

  activeEditor = new Editor({
    element: container,
    extensions: [
      StarterKit.configure({
        history: true,
      }),
      Underline,
      TextStyle,
      Color,
      TextAlign.configure({
        types: ['heading', 'paragraph'],
      }),
      Image.configure({
        allowBase64: true,
        inline: false,
      }),
      CharacterCount,
      Placeholder.configure({
        placeholder: '在此开始创作中长篇小说正文……（可在下方借助 AI 辅助对话创作）',
      }),
      GhostCompletionExtension,
    ],
    content,
    autofocus: true,
    editable: true,
    onUpdate({ editor }) {
      if (typeof updateCallback === 'function') {
        try {
          updateCallback();
        } catch (err) {
          console.error('[TiptapBridge] Error in onUpdate callback:', err);
        }
      }
    },
    onSelectionUpdate({ editor }) {
      scrollToCaretCenter();
      if (typeof selectionCallback === 'function') {
        try {
          selectionCallback();
        } catch (err) {
          console.error('[TiptapBridge] Error in onSelectionUpdate callback:', err);
        }
      }
    },
  });

  return true;
}

export function destroy_editor() {
  if (activeEditor) {
    activeEditor.destroy();
    activeEditor = null;
    updateCallback = null;
  }
}

export function get_content_json() {
  if (!activeEditor) return '{}';
  const json = activeEditor.getJSON();
  const normalized = transformDocImages(json, normalize_asset_url);
  return JSON.stringify(normalized);
}

export function get_content_plain() {
  if (!activeEditor) return '';
  return activeEditor.getText();
}

export function get_word_count() {
  if (!activeEditor) return 0;
  return activeEditor.storage.characterCount.characters();
}

export function set_content(jsonStr) {
  if (!activeEditor) return false;
  try {
    const json = JSON.parse(jsonStr || '{}');
    const resolved = transformDocImages(json, resolve_asset_url);
    activeEditor.commands.setContent(resolved, false);
    return true;
  } catch (err) {
    console.error('[TiptapBridge] Failed to set content JSON:', err);
    return false;
  }
}

export function set_editable(editable) {
  if (!activeEditor) return false;
  try {
    activeEditor.setEditable(Boolean(editable));
    return true;
  } catch (err) {
    console.error('[TiptapBridge] Failed to set editor editable:', err);
    return false;
  }
}

export function insert_at_cursor(text) {
  if (!activeEditor) return false;
  activeEditor.commands.insertContent(text);
  return true;
}

export function replace_selection(text) {
  if (!activeEditor) return false;
  activeEditor.commands.insertContent(text);
  return true;
}

export function get_selection_text() {
  if (!activeEditor) return null;
  const { from, to } = activeEditor.state.selection;
  if (from === to) return null;
  return activeEditor.state.doc.textBetween(from, to, ' ');
}

export function get_selection_coords() {
  if (!activeEditor) return null;
  const { from, to } = activeEditor.state.selection;
  if (from === to) return null;
  try {
    const { view } = activeEditor;
    const start = view.coordsAtPos(from);
    const end = view.coordsAtPos(to);
    return JSON.stringify({
      left: (start.left + end.left) / 2,
      top: start.top,
      bottom: end.bottom,
    });
  } catch (err) {
    console.error('[TiptapBridge] Error calculating selection coordinates:', err);
    return null;
  }
}

export function focus() {
  if (activeEditor) activeEditor.commands.focus();
}

export function toggle_bold() {
  if (activeEditor) activeEditor.chain().focus().toggleBold().run();
}

export function toggle_italic() {
  if (activeEditor) activeEditor.chain().focus().toggleItalic().run();
}

export function toggle_underline() {
  if (activeEditor) activeEditor.chain().focus().toggleUnderline().run();
}

export function toggle_heading(level) {
  if (activeEditor) activeEditor.chain().focus().toggleHeading({ level }).run();
}

export function set_text_align(alignment) {
  if (activeEditor) activeEditor.chain().focus().setTextAlign(alignment).run();
}

export function toggle_bullet_list() {
  if (activeEditor) activeEditor.chain().focus().toggleBulletList().run();
}

export function toggle_ordered_list() {
  if (activeEditor) activeEditor.chain().focus().toggleOrderedList().run();
}

export function toggle_blockquote() {
  if (activeEditor) activeEditor.chain().focus().toggleBlockquote().run();
}

export function insert_horizontal_rule() {
  if (activeEditor) activeEditor.chain().focus().setHorizontalRule().run();
}

export function insert_image(src, alt = '') {
  if (activeEditor) {
    const runtimeUrl = resolve_asset_url(src);
    activeEditor.chain().focus().setImage({ src: runtimeUrl, alt }).run();
  }
}

export function undo() {
  if (activeEditor) activeEditor.chain().focus().undo().run();
}

export function redo() {
  if (activeEditor) activeEditor.chain().focus().redo().run();
}

// -------------------------------------------------------------
// Typewriter mode (T11): scroll caret line to vertical center
// -------------------------------------------------------------
let typewriterEnabled = false;

function scrollToCaretCenter() {
  if (!typewriterEnabled || !activeEditor) return;
  try {
    const { view } = activeEditor;
    const coords = view.coordsAtPos(view.state.selection.from);
    // 编辑器容器是 ProseMirror 根节点的滚动祖先
    const scroller = view.dom.closest('.overflow-y-auto') || view.dom.parentElement;
    if (!scroller) return;
    const rect = scroller.getBoundingClientRect();
    const target = (coords.top + coords.bottom) / 2 - rect.top - rect.height / 2;
    scroller.scrollBy({ top: target, behavior: 'auto' });
  } catch (e) {
    // 光标坐标在未聚焦/空文档时可能不可得：静默跳过即可
  }
}

export function set_typewriter(enabled) {
  typewriterEnabled = !!enabled;
  if (typewriterEnabled) scrollToCaretCenter();
}

// Expose on global window object for direct invocation or wasm-bindgen
if (typeof window !== 'undefined') {
  window.TiptapBridge = {
    mount_editor,
    destroy_editor,
    set_app_data_dir,
    get_app_data_dir,
    resolve_asset_url,
    normalize_asset_url,
    on_selection_update,
    on_lexicon_term_accepted,
    set_lexicon,
    set_ghost_completion_enabled,
    get_content_json,
    get_content_plain,
    get_word_count,
    set_content,
    set_editable,
    insert_at_cursor,
    replace_selection,
    get_selection_text,
    get_selection_coords,
    focus,
    toggle_bold,
    toggle_italic,
    toggle_underline,
    toggle_heading,
    set_text_align,
    toggle_bullet_list,
    toggle_ordered_list,
    toggle_blockquote,
    insert_horizontal_rule,
    insert_image,
    undo,
    redo,
    set_typewriter,
  };
}
