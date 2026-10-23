import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 「冲突检测结构化与可配置」的跨端契约测试（任务书 T08 / PR-11）。
//
// 冲突检测涉及三段代码：后端命令、前端调用、设置页开关。
// 这里的契约面：
//
//   1. 设置页读写的三个键后端确实在读（键名对不上会让设置静默失效）；
//   2. 前端调用的 `chapter_id` 后端确实接受（否则「本章忽略」无从生效）；
//   3. 后端新加的两个结果态前端都有分支（漏一个就是运行时 panic / 提示被吞）；
//   4. 忽略命令的参数键与后端签名一致；
//   5. 新增的结构化字段（关联设定 / 证据 / 置信度 / 忽略签名）前后端都消费；
//   6. shared-types 的字段与后端解析出的字段一致。
// ---------------------------------------------------------------------------

const MODULE_SRC = path.join(repoRoot, 'src-tauri/src/ai/consistency.rs');
const AI_CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/ai_cmd.rs');
const CHAT_BOX_SRC = path.join(repoRoot, 'frontend/src/components/ai/chat_box.rs');
const MODAL_SRC = path.join(repoRoot, 'frontend/src/components/ai/conflict_modal.rs');
const APP_SHELL_SRC = path.join(repoRoot, 'frontend/src/components/layout/app_shell.rs');
const PREF_TAB_SRC = path.join(repoRoot, 'frontend/src/components/config/pref_tab.rs');
const SHARED_TYPES_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');

const moduleSrc = fs.readFileSync(MODULE_SRC, 'utf8');
const aiCmd = fs.readFileSync(AI_CMD_SRC, 'utf8');
const chatBox = fs.readFileSync(CHAT_BOX_SRC, 'utf8');
const modal = fs.readFileSync(MODAL_SRC, 'utf8');
const appShell = fs.readFileSync(APP_SHELL_SRC, 'utf8');
const prefTab = fs.readFileSync(PREF_TAB_SRC, 'utf8');
const sharedTypes = fs.readFileSync(SHARED_TYPES_SRC, 'utf8');

test('1. 设置项：写作偏好写入三个键，后端逐个读取', () => {
  for (const key of [
    'conflict_check_enabled',
    'conflict_check_scope',
    'conflict_confidence_threshold',
  ]) {
    assert.ok(
      prefTab.includes(`"${key}"`),
      `前端设置页没有读写 ${key}`
    );
    assert.ok(
      aiCmd.includes(`ConflictCheckSettings::from_conn(&conn)`),
      `后端没有通过 ConflictCheckSettings 读取设置（键 ${key}）`
    );
    assert.ok(
      moduleSrc.includes(`"${key}"`),
      `后端 ai/consistency.rs 没有读取设置键 ${key}`
    );
  }
  // 默认值：开关默认开、范围默认全部、阈值默认 0.6
  assert.ok(
    moduleSrc.includes('"conflict_check_enabled"') &&
      moduleSrc.includes('"conflict_check_scope"') &&
      moduleSrc.includes('"conflict_confidence_threshold"'),
    '后端设置读取必须用与前端一致的键名'
  );
  assert.ok(
    moduleSrc.includes('conflict_check_enabled') &&
      moduleSrc.includes('CheckScope::All') &&
      moduleSrc.includes('DEFAULT_CONFIDENCE_THRESHOLD'),
    '后端缺少设置解析链'
  );
});

test('2. 命令层：ai_check_conflict 收 chapter_id，ignore 命令键名一致', () => {
  assert.ok(
    aiCmd.includes('chapter_id: Option<String>'),
    '后端 ai_check_conflict 没有 chapter_id 参数'
  );
  assert.ok(
    chatBox.includes('invoke_cmd("ai_check_conflict")'),
    '前端没有调用 ai_check_conflict'
  );
  assert.ok(
    chatBox.includes('.arg("chapter_id", chapter_id)'),
    '前端没有把 chapter_id 传给 ai_check_conflict'
  );

  for (const token of [
    'invoke_cmd("ignore_chapter_conflict")',
    '.arg("novel_id", novel.id)',
    '.arg("chapter_id", chapter.id)',
    '.arg("signature", signature)',
    '.arg("label", label)',
  ]) {
    assert.ok(
      appShell.includes(token),
      `前端 app_shell 没有 ${token}`
    );
  }
  assert.ok(
    aiCmd.includes('pub fn ignore_chapter_conflict'),
    '后端没有 ignore_chapter_conflict 命令'
  );
  for (const arg of ['novel_id', 'chapter_id', 'signature', 'label']) {
    assert.ok(
      aiCmd.includes(`${arg}: String`) || aiCmd.includes(`${arg}: String,`),
      `后端 ignore_chapter_conflict 缺少 ${arg}`
    );
  }
});

test('3. 新结果态：前端 chat_box 对 Suppressed / Disabled 都有分支', () => {
  for (const variant of [
    'Ok(ConsistencyOutcome::Conflict { info })',
    'Ok(ConsistencyOutcome::Clean)',
    'Ok(ConsistencyOutcome::Suppressed { info, threshold })',
    'Ok(ConsistencyOutcome::Disabled)',
    'Ok(ConsistencyOutcome::NotChecked { reason })',
  ]) {
    assert.ok(
      chatBox.includes(variant),
      `chat_box 缺少分支 ${variant} —— 运行期会 panic`
    );
  }
  // 低于阈值：正文照插，但不弹窗
  assert.ok(
    chatBox.includes('insert_into_editor()'),
    'Suppressed 分支必须插入正文（不阻断写作）'
  );
});

test('4. 忽略按钮：弹窗有入口，app_shell 有回调，签名来自后端', () => {
  assert.ok(modal.includes('本章忽略此类提示'), '弹窗缺少「本章忽略此类提示」');
  assert.ok(
    modal.includes('on_ignore_this_kind'),
    '弹窗没有 on_ignore_this_kind 回调'
  );
  assert.ok(
    modal.includes('ignore_signature'),
    '弹窗没有使用后端算好的忽略签名'
  );
  assert.ok(
    appShell.includes('on_ignore_this_kind='),
    'app_shell 没有传 on_ignore_this_kind'
  );
  assert.ok(
    sharedTypes.includes('ignore_signature'),
    'shared_types 没有 ignore_signature 字段'
  );
});

test('5. 结构化字段：弹窗展示关联设定 / 证据 / 置信度', () => {
  for (const field of [
    'related_settings',
    'evidence',
    'confidence',
    'ignore_label',
    'conflict_point',
    'reason',
  ]) {
    assert.ok(
      sharedTypes.includes(`pub ${field}:`) || sharedTypes.includes(`${field}:`),
      `shared_types::ConflictInfo 缺少字段 ${field}`
    );
    assert.ok(
      modal.includes(field),
      `弹窗没有消费 ${field}`
    );
  }
  assert.ok(
    moduleSrc.includes('related_settings') &&
      moduleSrc.includes('evidence') &&
      moduleSrc.includes('confidence'),
    '后端没有解析关联设定 / 证据 / 置信度'
  );
  assert.ok(
    moduleSrc.includes('resolve_related_settings'),
    '后端没有关联设定解析（id 校验与标题回填）'
  );
});

test('6. 闸门：阈值与忽略签名的判定在后端，前端不做二次过滤', () => {
  assert.ok(
    moduleSrc.includes('apply_gate'),
    '后端缺少阈值 / 忽略闸门'
  );
  // 前端只应该消费「已经判好」的结果，不在前端重算
  assert.ok(
    !chatBox.includes('threshold <'),
    '前端不应自己重算阈值闸门'
  );
});
