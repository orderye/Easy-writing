import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 全书搜索与替换弹层的跨端契约测试（任务书 T09 / PR-09）。
//
// 弹层是纯前端组件，无法在 node 里跑 Leptos 视图；这里钉住的是
// 前后端必须对齐的**契约面**：
//
//   1. 四个命令的调用点与后端命令名一致（改名会整链断掉）；
//   2. Cmd/Ctrl+F 与 Esc 的全局绑定真的挂在 app_shell 上；
//   3. 「预览 → 勾选 → 执行 → 撤销」这条链的每一步都在；
//   4. 撤销入口只在执行成功后出现（不能让人点一个必然失败的按钮）；
//   5. 不足 3 字符走字面回退这件事，对用户是可见的。
// ---------------------------------------------------------------------------

const MODAL_SRC = path.join(repoRoot, 'frontend/src/components/search/search_modal.rs');
const SHELL_SRC = path.join(repoRoot, 'frontend/src/components/layout/app_shell.rs');
const STATE_SRC = path.join(repoRoot, 'frontend/src/state/mod.rs');
const CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/search_cmd.rs');
const SEARCH_DB_SRC = path.join(repoRoot, 'src-tauri/src/db/search.rs');
const SHARED_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');

const modal = fs.readFileSync(MODAL_SRC, 'utf8');
const shell = fs.readFileSync(SHELL_SRC, 'utf8');
const state = fs.readFileSync(STATE_SRC, 'utf8');
const cmd = fs.readFileSync(CMD_SRC, 'utf8');
const searchDb = fs.readFileSync(SEARCH_DB_SRC, 'utf8');
const shared = fs.readFileSync(SHARED_SRC, 'utf8');

const COMMANDS = [
  'search_chapters',
  'preview_replace',
  'apply_replace',
  'undo_replace_batch',
];

test('前端调用的四个命令都在后端注册为 tauri command', () => {
  for (const name of COMMANDS) {
    assert.ok(
      modal.includes(`invoke_cmd("${name}")`),
      `search_modal.rs 应调用 ${name}`,
    );
    assert.ok(
      cmd.includes('pub fn ' + name) && cmd.includes('#[tauri::command'),
      `search_cmd.rs 应定义 ${name} 为 tauri command`,
    );
  }
});

test('四个命令都声明了 snake_case 参数契约', () => {
  // 命令数：4 个 #[tauri::command] 全部带 rename_all = "snake_case"
  const commandAttrs = cmd.match(/#\[tauri::command[^\]]*\]/g) || [];
  assert.equal(commandAttrs.length, 4, `search_cmd.rs 应有 4 个命令，实际 ${commandAttrs.length}`);
  for (const attr of commandAttrs) {
    assert.ok(
      attr.includes('rename_all = "snake_case"'),
      `命令缺少 snake_case 契约：${attr}`,
    );
  }
});

test('shared-types 提供四个返回类型且字段与前端消费一致', () => {
  for (const typeName of [
    'ChapterSearchHit',
    'ReplacePreviewItem',
    'ReplaceReport',
    'UndoReplaceReport',
  ]) {
    assert.ok(shared.includes(`pub struct ${typeName}`), `shared-types 应有 ${typeName}`);
  }

  // 前端实际读的字段（少一个就是运行时 undefined）
  for (const field of ['batch_id', 'chapters_changed', 'total_replacements', 'skipped_chapters']) {
    assert.ok(
      shared.includes(`pub ${field}`),
      `ReplaceReport 应有字段 ${field}（前端 summary 会读它）`,
    );
  }
  for (const field of ['chapters_restored', 'missing_chapters']) {
    assert.ok(shared.includes(`pub ${field}`), `UndoReplaceReport 应有字段 ${field}`);
  }
  for (const field of ['snippets', 'has_body', 'volume_id']) {
    assert.ok(shared.includes(`pub ${field}`), `ChapterSearchHit 应有字段 ${field}`);
  }
});

test('Cmd/Ctrl+F 打开、Esc 关闭搜索弹层', () => {
  assert.ok(
    shell.includes('is_search_open'),
    'app_shell 的全局快捷键必须能打开搜索弹层',
  );
  assert.ok(
    /meta_key\(\)\s*\|\|\s*event\.ctrl_key\(\)\)[\s\S]{0,120}eq_ignore_ascii_case\("f"\)/.test(shell),
    '应注册 Cmd/Ctrl+F',
  );
  assert.ok(
    shell.includes('"Escape"'),
    '应注册 Esc 关闭',
  );
  assert.ok(
    shell.includes('SearchModal'),
    'app_shell 必须挂载 SearchModal',
  );
  assert.ok(
    state.includes('is_search_open: RwSignal<bool>'),
    'AppContext 应有 is_search_open',
  );
});

test('预览 → 勾选 → 执行 → 撤销 的调用链完整', () => {
  // 预览后默认全选
  assert.ok(
    /selected\.set\(items\.iter\(\)\.map\(\|i\| i\.chapter_id\.clone\(\)\)\.collect\(\)\)/.test(modal),
    '预览结果应默认全选',
  );
  // 执行时把勾选集合发给后端
  assert.ok(modal.includes('.opt("chapter_ids", Some(ids))'), '执行应携带 chapter_ids');
  // 执行成功后记下 batch_id 供撤销
  assert.ok(
    modal.includes('last_batch.set(Some(report.batch_id.clone()))'),
    '执行成功应记录 batch_id',
  );
  // 撤销按 batch_id 调用
  assert.ok(
    modal.includes('.arg("batch_id", batch)'),
    '撤销应传 batch_id',
  );
});

test('撤销按钮只在有批次时出现，且明确告知可撤销', () => {
  assert.ok(
    modal.includes('last_batch.get().is_some()'),
    '撤销入口应受 last_batch 控制（没执行过就不出现）',
  );
  assert.ok(
    /撤销/.test(modal),
    '结果摘要应有撤销入口文案',
  );
  assert.ok(
    modal.includes('替换前会自动为每章建立版本快照'),
    '面板应告知作者替换前有快照兜底',
  );
});

test('两字词回退对用户可见', () => {
  assert.ok(
    modal.includes('不足 3 字符的查找词走字面匹配'),
    '面板应提示 trigram 的 3 字符限制',
  );
  assert.ok(
    searchDb.includes('MIN_FTS_CHARS'),
    'db/search.rs 应保留 3 字符阈值的显式常量',
  );
});

test('格式不支持的章节被标出且会被跳过', () => {
  assert.ok(modal.includes('格式不支持'), '预览应标出无法替换的章节');
  assert.ok(
    modal.includes('skipped_chapters'),
    '执行结果应报告被跳过的章节',
  );
});

test('搜索与替换是两个 tab，且结果点击可跳章', () => {
  assert.ok(modal.includes('"search"') && modal.includes('"replace"'), '应有搜索/替换两个 tab');
  assert.ok(
    modal.includes('request_switch_chapter'),
    '点结果应复用切章逻辑（含未保存拦截）',
  );
});
