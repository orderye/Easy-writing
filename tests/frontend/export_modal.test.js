import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 导出链路的契约测试（任务书 T02 / PR-07）。
//
// 分两层：
//
// 1. **真实实现**：把 frontend/src/bridge/events.rs 的 inline_js 块原样抽出来
//    加载，测事件订阅的载荷透传 —— 抄一遍实现再断言是自欺欺人，
//    实现改了测试不会红。
// 2. **跨端契约**：前端弹层调用的命令名、事件名必须与后端对得上。
//    后端改名而前端没改，运行时才会炸；这里提前一步拦住。
// ---------------------------------------------------------------------------

const EVENTS_BRIDGE_SRC = path.join(repoRoot, 'frontend/src/bridge/events.rs');
const EXPORT_MODAL_SRC = path.join(repoRoot, 'frontend/src/components/export/export_modal.rs');
const EXPORT_CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/export_cmd.rs');

function extractInlineJsBlocks(rustSource) {
  const blocks = [];
  const re = /inline_js\s*=\s*r#"([\s\S]*?)"#/g;
  let m;
  while ((m = re.exec(rustSource)) !== null) blocks.push(m[1]);
  assert.ok(
    blocks.length > 0,
    'events.rs 里找不到 inline_js 块 —— 抽取逻辑已失效，请同步更新本测试'
  );
  return blocks;
}

const tmpModulePath = path.join(os.tmpdir(), `ew-events-bridge-${process.pid}.mjs`);
fs.writeFileSync(
  tmpModulePath,
  extractInlineJsBlocks(fs.readFileSync(EVENTS_BRIDGE_SRC, 'utf8')).join('\n')
);
const { tauri_event_listen } = await import(pathToFileURL(tmpModulePath).href);

function withWindow(win, fn) {
  const saved = Object.getOwnPropertyDescriptor(globalThis, 'window');
  Object.defineProperty(globalThis, 'window', { value: win, configurable: true, writable: true });
  try {
    return fn();
  } finally {
    if (saved) Object.defineProperty(globalThis, 'window', saved);
    else delete globalThis.window;
  }
}

test('1. 事件订阅：有 Tauri 接口时把 payload 转交给回调', () => {
  const registered = [];
  const fakeTauri = {
    event: {
      listen: (name, cb) => {
        registered.push([name, cb]);
      },
    },
  };

  const received = [];
  withWindow({ __TAURI__: fakeTauri }, () => {
    tauri_event_listen('novel-export://progress', (payload) => {
      received.push(payload);
    });
  });

  assert.equal(registered.length, 1, '应当注册一个监听器');
  assert.equal(registered[0][0], 'novel-export://progress');

  // 模拟后端 emit：Tauri 传的是 { event, id, payload }。
  registered[0][1]({ event: 'novel-export://progress', id: 1, payload: { done: 3, total: 10 } });
  assert.deepEqual(received[0], { done: 3, total: 10 });
});

test('2. 事件订阅：没有 Tauri 接口时只告警，不抛异常', () => {
  withWindow({}, () => {
    assert.doesNotThrow(() => {
      tauri_event_listen('novel-export://progress', () => {});
    });
  });
});

test('3. 事件订阅：回调抛错不影响监听器本身', () => {
  const registered = [];
  const fakeTauri = {
    event: {
      listen: (name, cb) => {
        registered.push([name, cb]);
      },
    },
  };
  withWindow({ __TAURI__: fakeTauri }, () => {
    tauri_event_listen('novel-export://progress', () => {
      throw new Error('回调炸了');
    });
  });
  assert.doesNotThrow(() => {
    registered[0][1]({ payload: { done: 1 } });
  });
});

test('4. 跨端契约：前端调用的三个导出命令与后端注册的一致', () => {
  const frontend = fs.readFileSync(EXPORT_MODAL_SRC, 'utf8');
  const backend = fs.readFileSync(EXPORT_CMD_SRC, 'utf8');

  for (const cmd of ['choose_export_file', 'export_novel', 'cancel_export']) {
    assert.ok(
      frontend.includes(`"${cmd}"`),
      `前端导出弹层没有调用 ${cmd} —— 后端已注册，前端漏接会让功能整条链断掉`
    );
    assert.ok(
      backend.includes(`pub async fn ${cmd}`) || backend.includes(`pub fn ${cmd}`),
      `后端找不到命令 ${cmd}`
    );
  }
});

test('5. 跨端契约：进度事件名前后端一致', () => {
  const frontend = fs.readFileSync(EXPORT_MODAL_SRC, 'utf8');
  const backend = fs.readFileSync(EXPORT_CMD_SRC, 'utf8');

  const backendMatch = backend.match(/pub const PROGRESS_EVENT: &str = "([^"]+)"/);
  assert.ok(backendMatch, '后端没有 PROGRESS_EVENT 常量');
  const eventName = backendMatch[1];

  assert.ok(
    frontend.includes(`"${eventName}"`),
    `前端监听的进度事件名与后端不一致（后端：${eventName}）`
  );
});

test('6. 导出选项：模板占位符与后端渲染支持的占位符一致', () => {
  const backend = fs.readFileSync(EXPORT_CMD_SRC, 'utf8');
  const exportMod = fs.readFileSync(
    path.join(repoRoot, 'src-tauri/src/export/mod.rs'),
    'utf8'
  );
  const frontend = fs.readFileSync(EXPORT_MODAL_SRC, 'utf8');

  // 后端 render_template 认识的占位符。
  for (const token of ['novel', 'volume_index', 'volume_title', 'chapter_index', 'chapter_title']) {
    assert.ok(
      exportMod.includes(`"${token}"`),
      `后端 render_template 不认识占位符 ${token}`
    );
  }
  // 前端提示文案里列出的占位符必须都是后端认识的。
  const hint = frontend.match(/"占位符：([^"]+)"/);
  assert.ok(hint, '前端没有给出占位符提示');
  for (const raw of hint[1].split(/\s+/).filter(Boolean)) {
    // 前端提示里带花括号（{novel}），后端匹配的是不含花括号的名字（"novel"）。
    const token = raw.replace(/[{}]/g, '');
    assert.ok(
      exportMod.includes(`"${token}"`),
      `前端提示了后端不认识的占位符 ${raw}`
    );
  }
  assert.ok(backend.length > 0);
});
