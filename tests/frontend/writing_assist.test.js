import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 写作辅助的跨端契约（任务书 T11 / PR-13）。
//
// 三块功能、四段代码：
//   1. 每日字数统计：save_chapter_content 按新旧字数差累加 daily_stat；
//      导入路径（commit_import_novel → import/db.rs 直接 INSERT）不经过该命令；
//   2. 专注/打字机模式：快捷键 + body class + Tiptap 桥 set_typewriter；
//   3. 章节 POV / story_time：V009 迁移列 + update_chapter_pov 命令 + 右栏筛选。
// ---------------------------------------------------------------------------

const COMMANDS_SRC = path.join(repoRoot, 'src-tauri/src/commands/mod.rs');
const LIB_SRC = path.join(repoRoot, 'src-tauri/src/lib.rs');
const CHAPTER_SRC = path.join(repoRoot, 'src-tauri/src/db/chapter.rs');
const DAILY_STAT_SRC = path.join(repoRoot, 'src-tauri/src/db/daily_stat.rs');
const IMPORT_DB_SRC = path.join(repoRoot, 'src-tauri/src/import/db.rs');
const SHARED_TYPES_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');
const MIGRATION_SRC = path.join(repoRoot, 'src-tauri/migrations/V009__daily_stat_and_chapter_pov.sql');
const BRIDGE_JS = path.join(repoRoot, 'js/src/tiptap-bridge.js');
const BRIDGE_RS = path.join(repoRoot, 'frontend/src/bridge/tiptap.rs');
const APP_SHELL = path.join(repoRoot, 'frontend/src/components/layout/app_shell.rs');
const STATUS_BAR = path.join(repoRoot, 'frontend/src/components/editor/status_bar.rs');
const STRUCTURE_MOD = path.join(repoRoot, 'frontend/src/components/structure_panel/mod.rs');
const VOLUME_LIST = path.join(repoRoot, 'frontend/src/components/structure_panel/volume_list.rs');
const SAVE_BUTTON = path.join(repoRoot, 'frontend/src/components/structure_panel/save_button.rs');
const STATE_SRC = path.join(repoRoot, 'frontend/src/state/mod.rs');
const MAIN_CSS = path.join(repoRoot, 'frontend/styles/main.css');

const commands = fs.readFileSync(COMMANDS_SRC, 'utf8');
const lib = fs.readFileSync(LIB_SRC, 'utf8');
const chapter = fs.readFileSync(CHAPTER_SRC, 'utf8');
const dailyStat = fs.readFileSync(DAILY_STAT_SRC, 'utf8');
const importDb = fs.readFileSync(IMPORT_DB_SRC, 'utf8');
const sharedTypes = fs.readFileSync(SHARED_TYPES_SRC, 'utf8');
const migration = fs.readFileSync(MIGRATION_SRC, 'utf8');
const bridgeJs = fs.readFileSync(BRIDGE_JS, 'utf8');
const bridgeRs = fs.readFileSync(BRIDGE_RS, 'utf8');
const appShell = fs.readFileSync(APP_SHELL, 'utf8');
const statusBar = fs.readFileSync(STATUS_BAR, 'utf8');
const structureMod = fs.readFileSync(STRUCTURE_MOD, 'utf8');
const volumeList = fs.readFileSync(VOLUME_LIST, 'utf8');
const saveButton = fs.readFileSync(SAVE_BUTTON, 'utf8');
const state = fs.readFileSync(STATE_SRC, 'utf8');
const mainCss = fs.readFileSync(MAIN_CSS, 'utf8');

test('1. 迁移 V009：daily_stat 表、UNIQUE 约束与 chapter 新列', () => {
  assert.ok(migration.includes('CREATE TABLE IF NOT EXISTS daily_stat'), 'V009 必须建 daily_stat');
  assert.ok(migration.includes('UNIQUE(novel_id, stat_date)'), 'daily_stat 必须有 (novel_id, stat_date) 唯一约束');
  assert.ok(migration.includes('ALTER TABLE chapter ADD COLUMN pov_character_id'), 'V009 必须给 chapter 加 pov_character_id');
  assert.ok(migration.includes('ALTER TABLE chapter ADD COLUMN story_time'), 'V009 必须给 chapter 加 story_time');
  assert.ok(migration.includes('idx_chapter_pov'), 'V009 必须建 POV 筛选索引');
});

test('2. 每日统计：净增按保存前后差值累加，导入路径不挂钩', () => {
  assert.ok(
    commands.includes('let delta = (word_count as i64) - (old_word_count as i64)'),
    'save_chapter_content 必须按新旧字数差计算净增'
  );
  assert.ok(
    commands.includes('add_net_words'),
    'save_chapter_content 必须调用 add_net_words'
  );
  assert.ok(
    commands.includes('daily_stat::get_today') && commands.includes('daily_stat::get_goal') && commands.includes('daily_stat::set_goal'),
    '三个每日统计命令必须都接到 db 层'
  );
  assert.ok(
    !importDb.includes('daily_stat') && !importDb.includes('add_net_words'),
    '导入路径（import/db.rs）不得触碰每日统计 —— 导入整书不能产生巨额「今日字数」'
  );
  // UPSERT 而不是先查后插：并发保存时同一天必须只有一行
  assert.ok(
    dailyStat.includes('ON CONFLICT(novel_id, stat_date) DO UPDATE'),
    'add_net_words 必须用 UPSERT'
  );
});

test('3. 命令注册：4 个新命令在 commands 与 lib.rs 都有', () => {
  for (const cmd of ['get_daily_stat', 'get_daily_goal', 'set_daily_goal', 'update_chapter_pov']) {
    assert.ok(commands.includes(`pub fn ${cmd}`), `commands/mod.rs 找不到 ${cmd}`);
    assert.ok(lib.includes(`commands::${cmd}`), `lib.rs 没有注册 ${cmd}`);
  }
});

test('4. shared-types：Chapter/ChapterHeader 带 pov/story_time，DailyStat 存在', () => {
  assert.ok(sharedTypes.includes('pub struct DailyStat'), '缺少 DailyStat');
  assert.ok(
    sharedTypes.includes('pub pov_character_id: Option<String>') && sharedTypes.includes('pub story_time: Option<String>'),
    'Chapter/ChapterHeader 缺少 pov_character_id / story_time'
  );
});

test('5. 专注/打字机模式：快捷键、CSS class、桥接齐全', () => {
  assert.ok(
    appShell.includes('is_focus_mode.update(|v| *v = !*v)'),
    'app_shell 必须有专注模式快捷键分支'
  );
  assert.ok(appShell.includes('is_typewriter_mode'), 'app_shell 必须有打字机模式分支');
  assert.ok(appShell.includes('"focus-mode"'), '专注模式必须切 body class');
  assert.ok(mainCss.includes('body.focus-mode'), 'main.css 必须有 body.focus-mode 规则');
  assert.ok(bridgeJs.includes('export function set_typewriter'), 'Tiptap 桥必须有 set_typewriter');
  assert.ok(bridgeRs.includes('pub fn set_typewriter'), 'Rust 桥必须声明 set_typewriter');
  // 不销毁编辑器：焦点模式相关代码不得触碰 destroy_editor
  const focusRelated = appShell + statusBar + state;
  assert.ok(
    !focusRelated.includes('destroy_editor'),
    '专注模式实现不得调用 destroy_editor（沿用切章不销毁的原则）'
  );
});

test('6. 状态栏：显示今日字数与目标进度', () => {
  assert.ok(statusBar.includes('today_net_words'), '状态栏必须读 today_net_words');
  assert.ok(statusBar.includes('daily_goal'), '状态栏必须读 daily_goal');
  assert.ok(saveButton.includes('get_daily_stat'), '保存成功后必须刷新今日字数');
});

test('7. 右栏：POV/story_time 筛选与章节元数据编辑', () => {
  assert.ok(
    volumeList.includes('chapter_pov_filter') && volumeList.includes('chapter_story_time_filter'),
    '卷章树必须按 POV/story_time 过滤'
  );
  assert.ok(
    structureMod.includes('update_chapter_pov'),
    '右栏元数据卡必须能保存 POV/story_time'
  );
  assert.ok(state.includes('chapter_pov_filter') && state.includes('is_focus_mode'), 'AppContext 缺少新状态');
});
