import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const PALETTE_SRC = path.join(repoRoot, 'frontend/src/components/search/command_palette.rs');
const APP_SHELL_SRC = path.join(repoRoot, 'frontend/src/components/layout/app_shell.rs');
const STATE_SRC = path.join(repoRoot, 'frontend/src/state/mod.rs');
const FORESHADOW_SRC = path.join(repoRoot, 'frontend/src/components/settings_panel/foreshadow_board.rs');
const STATUS_BAR_SRC = path.join(repoRoot, 'frontend/src/components/editor/status_bar.rs');

const palette = fs.readFileSync(PALETTE_SRC, 'utf8');
const appShell = fs.readFileSync(APP_SHELL_SRC, 'utf8');
const state = fs.readFileSync(STATE_SRC, 'utf8');
const foreshadow = fs.readFileSync(FORESHADOW_SRC, 'utf8');
const statusBar = fs.readFileSync(STATUS_BAR_SRC, 'utf8');

test('1. 命令面板包含核心快捷命令并支持关键词过滤', () => {
  for (const id of [
    '"save"',
    '"search"',
    '"focus_mode"',
    '"typewriter_mode"',
    '"export"',
    '"foreshadow"',
    '"extract"',
    '"config"',
  ]) {
    assert.ok(palette.includes(id), `ALL_COMMANDS 缺少命令 ${id}`);
  }
  assert.ok(
    palette.includes('filtered_commands') && palette.includes('.contains(&q)'),
    '命令面板缺少关键词过滤逻辑'
  );
});

test('2. app_shell 绑定 Cmd/Ctrl+K 与 Escape 关闭命令面板', () => {
  assert.ok(
    appShell.includes('eq_ignore_ascii_case("k")') &&
      appShell.includes('is_command_palette_open.update'),
    'app_shell 缺少 Cmd/Ctrl+K 切换命令面板快捷键'
  );
  assert.ok(
    appShell.includes('"Escape"') && appShell.includes('is_command_palette_open.set(false)'),
    'app_shell 缺少 Escape 关闭命令面板逻辑'
  );
  assert.ok(
    appShell.includes('<CommandPalette'),
    'app_shell 未挂载 CommandPalette 组件'
  );
});

test('3. 状态栏通过 AppContext.foreshadow_board 共享信号展示未回收与超期伏笔计数', () => {
  assert.ok(
    state.includes('pub foreshadow_board: RwSignal<Vec<ForeshadowRow>>'),
    'AppContext 缺少 foreshadow_board 共享信号'
  );
  assert.ok(
    foreshadow.includes('let board = app_ctx.foreshadow_board;'),
    'ForeshadowBoardView 未使用共享信号 app_ctx.foreshadow_board'
  );
  assert.ok(
    statusBar.includes('app_ctx.foreshadow_board.get()') &&
      statusBar.includes('active_settings_tab.set("foreshadow"'),
    'StatusBar 未只读消费 foreshadow_board 或缺少跳转伏笔看板入口'
  );
});
