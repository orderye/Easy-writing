import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import CharacterCount from '@tiptap/extension-character-count';
import Color from '@tiptap/extension-color';
import TextStyle from '@tiptap/extension-text-style';
import TextAlign from '@tiptap/extension-text-align';
import Image from '@tiptap/extension-image';
import Underline from '@tiptap/extension-underline';
import Placeholder from '@tiptap/extension-placeholder';

let activeEditor = null;
let updateCallback = null;
let selectionCallback = null;

export function on_selection_update(cb) {
  selectionCallback = cb;
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
  return JSON.stringify(activeEditor.getJSON());
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
    activeEditor.commands.setContent(json, false);
    return true;
  } catch (err) {
    console.error('[TiptapBridge] Failed to set content JSON:', err);
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
      top: Math.min(start.top, end.top),
      bottom: Math.max(start.bottom, end.bottom),
      left: (start.left + end.left) / 2,
    });
  } catch (e) {
    return null;
  }
}

export function focus() {
  if (activeEditor) {
    activeEditor.commands.focus();
  }
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
  if (activeEditor) activeEditor.chain().focus().setImage({ src, alt }).run();
}

export function undo() {
  if (activeEditor) activeEditor.chain().focus().undo().run();
}

export function redo() {
  if (activeEditor) activeEditor.chain().focus().redo().run();
}

// Expose on global window object for direct invocation or wasm-bindgen
if (typeof window !== 'undefined') {
  window.TiptapBridge = {
    mount_editor,
    destroy_editor,
    on_selection_update,
    get_content_json,
    get_content_plain,
    get_word_count,
    set_content,
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
  };
}
