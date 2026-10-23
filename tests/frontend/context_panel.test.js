import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 「本次上下文」面板的跨端契约测试（任务书 T06 / PR-08）。
//
// 面板是纯前端组件，无法在 node 里跑 Leptos 视图；这里钉住的是
// 前后端必须对齐的**契约面**：
//
//   1. 面板调用的命令后端必须存在（改名会整链断掉）；
//   2. 后端装配器会产出的每个 entity_type / reason，前端必须有对应的
//      分组与文案（漏一个就是面板里出现「其他」或孤儿分组）；
//   3. 请求级排除（excluded_entry_ids）的发送与清空链路；
//   4. 接口配置表单的 context_window 预算参数前后端一致；
//   5. shared-types 的清单字段与前端消费的字段一致。
// ---------------------------------------------------------------------------

const PANEL_SRC = path.join(repoRoot, 'frontend/src/components/ai/context_panel.rs');
const CHAT_BOX_SRC = path.join(repoRoot, 'frontend/src/components/ai/chat_box.rs');
const CONFIG_MODAL_SRC = path.join(repoRoot, 'frontend/src/components/config/config_modal.rs');
const AI_CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/ai_cmd.rs');
const CONTEXT_SRC = path.join(repoRoot, 'src-tauri/src/ai/context.rs');
const CONFIG_CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/config_cmd.rs');
const SHARED_TYPES_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');

const panel = fs.readFileSync(PANEL_SRC, 'utf8');
const chatBox = fs.readFileSync(CHAT_BOX_SRC, 'utf8');
const configModal = fs.readFileSync(CONFIG_MODAL_SRC, 'utf8');
const aiCmd = fs.readFileSync(AI_CMD_SRC, 'utf8');
const contextAsm = fs.readFileSync(CONTEXT_SRC, 'utf8');
const configCmd = fs.readFileSync(CONFIG_CMD_SRC, 'utf8');
const sharedTypes = fs.readFileSync(SHARED_TYPES_SRC, 'utf8');

/** 后端装配器产出的 (entity_type, reason) 对。 */
function backendPairs() {
  const pairs = [];
  const re = /push_entry\(\s*"([a-z_]+)",\s*[^,]+,\s*[^,]+,\s*"([a-z_]+)"/g;
  let m;
  while ((m = re.exec(contextAsm)) !== null) pairs.push([m[1], m[2]]);
  assert.ok(pairs.length >= 10, '装配器里找不到 push_entry 调用 —— 抽取逻辑已失效');
  return pairs;
}

/** 前端 reason_label 的匹配臂。 */
function frontendReasons() {
  const start = panel.indexOf('fn reason_label');
  const end = panel.indexOf('}', panel.indexOf('match reason', start));
  const body = panel.slice(start, end);
  const reasons = [];
  const re = /"([a-z_]+)"\s*=>/g;
  let m;
  while ((m = re.exec(body)) !== null) reasons.push(m[1]);
  return reasons;
}

/** 前端 GROUP_ORDER 的分组名。 */
function frontendGroups() {
  const start = panel.indexOf('const GROUP_ORDER');
  const end = panel.indexOf('];', start);
  const body = panel.slice(start, end);
  const groups = [];
  const re = /\("([a-z_]+)",/g;
  let m;
  while ((m = re.exec(body)) !== null) groups.push(m[1]);
  return groups;
}

test('1. 跨端契约：面板调用的两个命令后端都注册了', () => {
  for (const cmd of ['get_context_manifest', 'set_entry_pinned']) {
    assert.ok(
      panel.includes(`"${cmd}"`),
      `前端面板没有调用 ${cmd}`
    );
    assert.ok(
      aiCmd.includes(`pub fn ${cmd}`),
      `后端 ai_cmd.rs 找不到命令 ${cmd}`
    );
  }
});

test('2. reason 全集对应：后端产出的每个 reason 前端都有文案', () => {
  const backendReasons = [...new Set(backendPairs().map(([, r]) => r))];
  const frontend = frontendReasons();
  for (const reason of backendReasons) {
    assert.ok(
      frontend.includes(reason),
      `前端 reason_label 缺少后端会产出的 reason「${reason}」—— 面板会把它显示成「其他」`
    );
  }
});

test('3. entity_type 全集对应：后端产出的每个实体类型都在前端分组里', () => {
  const backendTypes = [...new Set(backendPairs().map(([t]) => t))];
  const frontend = frontendGroups();
  assert.deepEqual(
    [...backendTypes].sort(),
    [...frontend].sort(),
    '后端装配的实体类型集合必须与前端分组集合完全一致'
  );
});

test('4. 请求级排除：excluded_entry_ids 随请求发送、发送后清空并刷新清单', () => {
  // 前端：把排除清单作为可选参数发送
  assert.ok(
    chatBox.includes('.opt("excluded_entry_ids", Some(excluded_entries.get()))'),
    'chat_box 没有把 excluded_entry_ids 传给 ai_chat_stream'
  );
  // 前端：发送成功后清空排除清单并触发清单重载
  assert.ok(
    chatBox.includes('excluded_entries.set(Vec::new())'),
    'chat_box 发送成功后没有清空 excluded_entries（排除应只在当次请求生效）'
  );
  assert.ok(
    chatBox.includes('manifest_refresh.set(manifest_refresh.get() + 1)'),
    'chat_box 发送成功后没有触发清单刷新'
  );
  // 后端：ai_chat_stream 确实声明了该参数
  assert.ok(
    aiCmd.includes('excluded_entry_ids: Option<Vec<String>>'),
    '后端 ai_chat_stream 没有 excluded_entry_ids 参数'
  );
});

test('5. 接口配置：表单提交 context_window，后端两个写命令都接收', () => {
  assert.ok(
    configModal.includes('.opt("context_window",'),
    'config_modal 没有提交 context_window 参数'
  );
  for (const cmd of ['create_llm_profile', 'update_llm_profile']) {
    assert.ok(
      configCmd.includes(`pub async fn ${cmd}`) || configCmd.includes(`pub fn ${cmd}`),
      `后端找不到命令 ${cmd}`
    );
  }
  assert.ok(
    configCmd.includes('context_window: Option<u32>'),
    '后端接口配置写命令没有 context_window 参数'
  );
});

test('6. shared-types：清单字段与前端消费的字段一致', () => {
  // ContextManifest 的四个字段
  for (const field of ['budget_tokens', 'used_tokens', 'entries', 'trimmed']) {
    assert.ok(
      sharedTypes.includes(`pub ${field}:`),
      `shared_types::ContextManifest 缺少字段 ${field}`
    );
    assert.ok(
      panel.includes(`m.${field}`) || panel.includes(`.${field}`),
      `前端面板没有消费字段 ${field}`
    );
  }
  // ContextManifestEntry 的字段
  for (const field of ['entity_type', 'entry_id', 'title', 'reason', 'token_count', 'included']) {
    assert.ok(
      sharedTypes.includes(`pub ${field}:`),
      `shared_types::ContextManifestEntry 缺少字段 ${field}`
    );
    assert.ok(
      panel.includes(`e.${field}`) || panel.includes(`entry.${field}`),
      `前端面板没有消费条目字段 ${field}`
    );
  }
});

test('7. 面板挂载：聊天框在消息列表上方渲染 ContextPanel', () => {
  assert.ok(
    chatBox.includes('<ContextPanel'),
    'chat_box 没有渲染 ContextPanel 组件'
  );
  assert.ok(
    chatBox.includes('excluded=excluded_entries'),
    'ContextPanel 没有接收 excluded 信号'
  );
  assert.ok(
    chatBox.includes('refresh_tick=manifest_refresh'),
    'ContextPanel 没有接收刷新信号'
  );
});
