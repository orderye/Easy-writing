import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 伏笔看板的跨端契约（任务书 T10 / PR-12）。
//
// 看板涉及三段代码：后端命令、设置面板 tab、plot_lines 的新建入口。
// 这里的契约面：
//
//   1. 两个新命令后端确实注册了（改名会整链断掉）；
//   2. 设置面板有第 6 个「伏笔」tab；
//   3. plot_lines 新建伏笔时传当前章节 id（修复数据缺口）；
//   4. 看板组件调用 resolve_plot 时带 resolved_chapter_id；
//   5. shared-types 的 ForeshadowRow 字段与后端产出一致。
// ---------------------------------------------------------------------------

const SETTINGS_MOD_SRC = path.join(repoRoot, 'frontend/src/components/settings_panel/mod.rs');
const FORESHADOW_SRC = path.join(repoRoot, 'frontend/src/components/settings_panel/foreshadow_board.rs');
const PLOT_LINES_SRC = path.join(repoRoot, 'frontend/src/components/settings_panel/plot_lines.rs');
const COMMANDS_SRC = path.join(repoRoot, 'src-tauri/src/commands/mod.rs');
const LIB_SRC = path.join(repoRoot, 'src-tauri/src/lib.rs');
const SHARED_TYPES_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');
const MIGRATION_SRC = path.join(repoRoot, 'src-tauri/migrations/V008__plot_resolved_chapter.sql');

const settingsMod = fs.readFileSync(SETTINGS_MOD_SRC, 'utf8');
const foreshadow = fs.readFileSync(FORESHADOW_SRC, 'utf8');
const plotLines = fs.readFileSync(PLOT_LINES_SRC, 'utf8');
const commands = fs.readFileSync(COMMANDS_SRC, 'utf8');
const lib = fs.readFileSync(LIB_SRC, 'utf8');
const sharedTypes = fs.readFileSync(SHARED_TYPES_SRC, 'utf8');
const migration = fs.readFileSync(MIGRATION_SRC, 'utf8');

test('1. 命令注册：list_foreshadow_board 与 resolve_plot 在后端与 lib.rs 都有', () => {
  for (const cmd of ['list_foreshadow_board', 'resolve_plot']) {
    assert.ok(
      commands.includes(`pub fn ${cmd}`),
      `commands/mod.rs 找不到命令 ${cmd}`
    );
    assert.ok(
      lib.includes(`commands::${cmd}`),
      `lib.rs 没有注册 ${cmd}`
    );
  }
});

test('2. 设置面板：grid-cols-6 且有「伏笔」tab', () => {
  assert.ok(
    settingsMod.includes('grid-cols-6'),
    '设置面板 tab 行应是 grid-cols-6（6 个 tab）'
  );
  assert.ok(
    settingsMod.includes('"伏笔"'),
    '设置面板缺少「伏笔」tab 按钮'
  );
  assert.ok(
    settingsMod.includes('"foreshadow" => view! { <ForeshadowBoardView'),
    '设置面板 match 里没有 foreshadow 分支'
  );
});

test('3. plot_lines 新建伏笔时传当前章节 id', () => {
  assert.ok(
    plotLines.includes('app_ctx.current_chapter.get().map(|c| c.id)'),
    'plot_lines 新建伏笔时没有传当前章节 id'
  );
});

test('4. 看板调用 resolve_plot 时带 resolved_chapter_id', () => {
  assert.ok(
    foreshadow.includes('invoke_cmd("resolve_plot")'),
    '看板没有调用 resolve_plot'
  );
  assert.ok(
    foreshadow.includes('resolved_chapter_id'),
    '看板调用 resolve_plot 时没有带 resolved_chapter_id'
  );
});

test('5. shared-types：ForeshadowRow 字段与后端产出一致', () => {
  for (const field of [
    'id', 'novel_id', 'title', 'content', 'kind', 'status',
    'chapter_id', 'resolved_chapter_id', 'chapter_index',
    'resolved_chapter_index', 'chapters_since_plant', 'overdue',
    'related_characters',
  ]) {
    assert.ok(
      sharedTypes.includes(`pub ${field}:`),
      `ForeshadowRow 缺少字段 ${field}`
    );
  }
  // PlotItem 必须有 resolved_chapter_id
  assert.ok(
    sharedTypes.includes('pub resolved_chapter_id: Option<String>'),
    'PlotItem 缺少 resolved_chapter_id'
  );
});

test('6. 迁移 V008：plot_item 加 resolved_chapter_id 列 + 索引', () => {
  assert.ok(
    migration.includes('ALTER TABLE plot_item ADD COLUMN resolved_chapter_id'),
    'V008 必须给 plot_item 加 resolved_chapter_id 列'
  );
  assert.ok(
    migration.includes('idx_plot_resolved_chapter'),
    'V008 必须建 idx_plot_resolved_chapter 索引'
  );
});
