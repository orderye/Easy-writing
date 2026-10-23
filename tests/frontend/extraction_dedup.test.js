import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 提炼成本与去重的跨端契约（任务书 T07 / PR-10）。
//
// 纯前端无法在 node 里跑 Leptos 视图，这里钉住前后端必须对齐的契约面：
//   1. 候选结构新增 possible_duplicate_of，两端字段名一致；
//   2. V006 迁移真的加了列与索引；
//   3. 审核面板提供「疑似同人」提示与合并入口，合并走专用命令；
//   4. 设置项 extraction_mode / extraction_chunk_threshold 在前后端同名；
//   5. 两种提炼模式最终产出同构的候选（解析函数在同一处归一）。
// ---------------------------------------------------------------------------

const MODAL_SRC = path.join(repoRoot, 'frontend/src/components/ai/extraction_modal.rs');
const PREF_SRC = path.join(repoRoot, 'frontend/src/components/config/pref_tab.rs');
const CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/ai_cmd.rs');
const ENGINE_SRC = path.join(repoRoot, 'src-tauri/src/agent/extraction.rs');
const SHARED_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');
const MIGRATION_SRC = path.join(repoRoot, 'src-tauri/migrations/V006__extraction_duplicate.sql');
const LIB_SRC = path.join(repoRoot, 'src-tauri/src/lib.rs');

const modal = fs.readFileSync(MODAL_SRC, 'utf8');
const pref = fs.readFileSync(PREF_SRC, 'utf8');
const cmd = fs.readFileSync(CMD_SRC, 'utf8');
const engine = fs.readFileSync(ENGINE_SRC, 'utf8');
const shared = fs.readFileSync(SHARED_SRC, 'utf8');
const migration = fs.readFileSync(MIGRATION_SRC, 'utf8');
const lib = fs.readFileSync(LIB_SRC, 'utf8');

test('候选结构与迁移都带 possible_duplicate_of', () => {
  assert.ok(
    shared.includes('pub possible_duplicate_of: Option<String>'),
    'shared-types 的候选必须有 possible_duplicate_of',
  );
  assert.ok(
    migration.includes('ADD COLUMN possible_duplicate_of TEXT'),
    'V006 必须给 extraction_candidate 加列',
  );
  assert.ok(
    migration.includes('idx_extraction_duplicate'),
    'V006 必须建索引（审核面板按锚点查询）',
  );
});

test('合并命令存在、带 snake_case 契约且已注册', () => {
  assert.ok(
    cmd.includes('pub fn merge_character_candidate'),
    'ai_cmd 应提供合并命令',
  );
  const attrIdx = cmd.indexOf('pub fn merge_character_candidate');
  const attrStart = cmd.lastIndexOf('#[tauri::command', attrIdx);
  assert.ok(
    cmd.slice(attrStart, attrIdx).includes('rename_all = "snake_case"'),
    '合并命令必须声明 snake_case',
  );
  assert.ok(
    lib.includes('commands::ai_cmd::merge_character_candidate'),
    '命令必须进 generate_handler',
  );
  assert.ok(
    modal.includes('invoke_cmd("merge_character_candidate")'),
    '审核面板必须调用合并命令',
  );
});

test('审核面板展示疑似提示并提供合并入口', () => {
  assert.ok(modal.includes('possible_duplicate_of'), '面板需读取锚点字段');
  assert.ok(modal.includes('疑似'), '面板需显示「疑似」提示');
  assert.ok(modal.includes('合并到'), '面板需提供合并按钮');
  assert.ok(
    modal.includes('list_characters'),
    '面板需要角色列表才能把锚点翻译成名字',
  );
  assert.ok(
    modal.includes('与本批候选'),
    '候选池锚点（尚未入库）也要有提示',
  );
});

test('提炼设置前后端同名且默认值一致', () => {
  assert.ok(engine.includes('"extraction_mode"'), '引擎需读取 extraction_mode');
  assert.ok(
    engine.includes('"extraction_chunk_threshold"'),
    '引擎需读取 extraction_chunk_threshold',
  );
  assert.ok(engine.includes('DEFAULT_CHUNK_THRESHOLD_CHARS: usize = 12_000'));
  assert.ok(pref.includes('"extraction_mode"'), '偏好页需持久化 extraction_mode');
  assert.ok(
    pref.includes('"extraction_chunk_threshold"'),
    '偏好页需持久化 extraction_chunk_threshold',
  );
  assert.ok(pref.includes('"single_call"'), '偏好页需提供 single_call 选项');
  assert.ok(pref.includes('"parallel_4"'), '偏好页需提供 parallel_4 选项');
});

test('两种提炼模式共用同一套候选解析与入库', () => {
  assert.ok(engine.includes('pub enum ExtractionMode'), '应有模式枚举');
  assert.ok(
    engine.includes('pub fn parse_single_call_candidates'),
    'single_call 需独立解析并归一为 RawCandidate',
  );
  assert.ok(
    engine.includes('pub fn split_into_chunks'),
    '超长章节需分块',
  );
  assert.ok(
    engine.includes('merge_chunk_summaries'),
    '分块的章纲必须合并成一条',
  );
  // 两条路径最终都走同一个入库循环（insert_candidate 只出现一次）
  const insertCount = (engine.match(/insert_candidate\(/g) || []).length;
  assert.equal(insertCount, 1, '入库逻辑只能有一份，避免两条路径结构漂移');
});

test('疑似同人判定是「提示」而不是自动合并', () => {
  assert.ok(
    engine.includes('只用于提示，不用于自动合并'),
    '必须在源码里写清这条边界',
  );
  assert.ok(engine.includes('pub fn titles_look_same'));
  assert.ok(engine.includes('pub fn detect_possible_duplicate'));
});