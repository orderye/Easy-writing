/**
 * 发布门禁决策器的测试（V0.3.8 补充任务书 P1-5）。
 *
 * 钉住的是那条真实存在的绕过路径：release.yml 的版本校验带着
 * `if: startsWith(github.ref, 'refs/tags/')`，手动触发时整段被跳过，
 * 随后 tauri-action 用 `github.ref_name`（= 分支名）创建 Release。
 *
 * 运行：node --test tests/scripts/release_gate.test.js
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { execFileSync } from 'node:child_process';

import { resolveReleaseGate, gitTagExists } from '../../scripts/release-gate.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SCRIPT = join(ROOT, 'scripts', 'release-gate.mjs');

/** 真实执行脚本，返回退出码与输出。 */
function runGate(env) {
  const res = spawnSync(process.execPath, [SCRIPT], {
    cwd: ROOT,
    env: { ...process.env, ...env },
    encoding: 'utf8',
  });
  return { code: res.status, out: `${res.stdout ?? ''}${res.stderr ?? ''}` };
}

// ---------------------------------------------------------------------------
// 决策逻辑
// ---------------------------------------------------------------------------

test('tag push 正常发布', () => {
  const r = resolveReleaseGate({
    eventName: 'push',
    ref: 'refs/tags/v0.3.8',
    refName: 'v0.3.8',
  });
  assert.equal(r.ok, true);
  assert.equal(r.isRelease, true);
  assert.equal(r.releaseTag, 'v0.3.8');
  assert.equal(r.checkoutRef, 'v0.3.8');
});

test('分支 push 不得创建 Release', () => {
  // 这正是被绕过的那条路径：github.ref_name 是 main，
  // 旧 workflow 会用它当 tagName 和 releaseName。
  const r = resolveReleaseGate({
    eventName: 'push',
    ref: 'refs/heads/main',
    refName: 'main',
  });
  assert.equal(r.ok, false);
  assert.equal(r.isRelease, false);
  assert.equal(r.releaseTag, '');
  assert.match(r.errors.join('\n'), /不是 tag/);
});

test('非版本格式的 tag 被拒绝', () => {
  for (const bad of ['main', 'v0.3', 'release-0.3.8', 'v0.3.8-rc1', 'V0.3.8', '']) {
    const r = resolveReleaseGate({
      eventName: 'push',
      ref: `refs/tags/${bad}`,
      refName: bad,
    });
    assert.equal(r.ok, false, `tag "${bad}" 必须被拒绝`);
    assert.equal(r.isRelease, false);
  }
});

test('手动触发未选 mode 时拒绝发布', () => {
  const r = resolveReleaseGate({
    eventName: 'workflow_dispatch',
    ref: 'refs/heads/main',
    refName: 'main',
    mode: '',
    tag: '',
  });
  assert.equal(r.ok, false);
  assert.equal(r.isRelease, false);
  assert.match(r.errors.join('\n'), /必须显式选择 mode/);
});

test('手动触发 build-only 不产出 Release', () => {
  const r = resolveReleaseGate({
    eventName: 'workflow_dispatch',
    ref: 'refs/heads/main',
    refName: 'main',
    mode: 'build-only',
  });
  assert.equal(r.ok, true);
  assert.equal(r.isRelease, false, 'build-only 绝不能发布');
  assert.equal(r.releaseTag, '');
});

test('手动触发 release-tag 必须指向真实存在的 tag', () => {
  const r = resolveReleaseGate(
    {
      eventName: 'workflow_dispatch',
      ref: 'refs/heads/main',
      refName: 'main',
      mode: 'release-tag',
      tag: 'v9.9.9',
    },
    () => false
  );
  assert.equal(r.ok, false);
  assert.equal(r.isRelease, false);
  assert.match(r.errors.join('\n'), /不存在 tag/);
});

test('手动触发 release-tag 不得用分支名当 tag', () => {
  // 即便 tagExists 被误实现成恒真，格式校验也必须先拦住分支名。
  const r = resolveReleaseGate(
    {
      eventName: 'workflow_dispatch',
      ref: 'refs/heads/main',
      refName: 'main',
      mode: 'release-tag',
      tag: 'main',
    },
    () => true
  );
  assert.equal(r.ok, false);
  assert.equal(r.isRelease, false);
  assert.match(r.errors.join('\n'), /分支名/);
});

test('手动触发 release-tag 成功时检出 tag 本身而非分支 HEAD', () => {
  // 关键：构建内容必须与目标 tag 严格对应。
  // 若检出分支 HEAD，operator 指定的 tag 与实际构建的代码就可能不是同一份。
  const r = resolveReleaseGate(
    {
      eventName: 'workflow_dispatch',
      ref: 'refs/heads/main',
      refName: 'main',
      mode: 'release-tag',
      tag: 'v0.3.8',
    },
    () => true
  );
  assert.equal(r.ok, true);
  assert.equal(r.isRelease, true);
  assert.equal(r.releaseTag, 'v0.3.8');
  assert.equal(r.checkoutRef, 'v0.3.8', '必须检出 tag，而不是 main');
});

test('手动触发 release-tag 缺少 tag 参数时拒绝', () => {
  const r = resolveReleaseGate({
    eventName: 'workflow_dispatch',
    ref: 'refs/heads/main',
    refName: 'main',
    mode: 'release-tag',
    tag: '   ',
  });
  assert.equal(r.ok, false);
  assert.match(r.errors.join('\n'), /必须提供目标 tag/);
});

test('未知事件类型不得默认放行', () => {
  const r = resolveReleaseGate({ eventName: 'schedule', ref: 'refs/heads/main' });
  assert.equal(r.ok, false);
  assert.equal(r.isRelease, false);
});

// ---------------------------------------------------------------------------
// 真实脚本执行（端到端，不注入 tagExists）
// ---------------------------------------------------------------------------

test('脚本执行：分支 push 退出码为 1', () => {
  const { code, out } = runGate({
    GITHUB_EVENT_NAME: 'push',
    GITHUB_REF: 'refs/heads/main',
    GITHUB_REF_NAME: 'main',
    INPUT_MODE: '',
    INPUT_TAG: '',
    GITHUB_OUTPUT: '',
  });
  assert.equal(code, 1, `门禁必须失败，输出：${out}`);
  assert.match(out, /发布门禁未通过/);
});

test('脚本执行：手动触发未选 mode 退出码为 1', () => {
  const { code } = runGate({
    GITHUB_EVENT_NAME: 'workflow_dispatch',
    GITHUB_REF: 'refs/heads/main',
    GITHUB_REF_NAME: 'main',
    INPUT_MODE: '',
    INPUT_TAG: '',
    GITHUB_OUTPUT: '',
  });
  assert.equal(code, 1);
});

test('脚本执行：build-only 退出码为 0 且不发布', () => {
  const { code, out } = runGate({
    GITHUB_EVENT_NAME: 'workflow_dispatch',
    GITHUB_REF: 'refs/heads/main',
    GITHUB_REF_NAME: 'main',
    INPUT_MODE: 'build-only',
    INPUT_TAG: '',
    GITHUB_OUTPUT: '',
  });
  assert.equal(code, 0, `build-only 应通过，输出：${out}`);
  assert.match(out, /is_release=false/);
});

test('gitTagExists 对不存在的 tag 返回 false', () => {
  assert.equal(gitTagExists('v99.99.99-does-not-exist', ROOT), false);
});

test('gitTagExists 能识别仓库中真实存在的 tag', () => {
  let existing = '';
  try {
    existing = execFileSync('git', ['tag', '-l'], { cwd: ROOT, encoding: 'utf8' })
      .split('\n')
      .map((s) => s.trim())
      .filter(Boolean)[0];
  } catch {
    existing = '';
  }
  if (!existing) {
    // 浅克隆（如 CI 默认 checkout）可能不带 tag；此时跳过而不是假装通过。
    console.log('仓库中没有可读的 tag，跳过 gitTagExists 正向用例');
    return;
  }
  assert.equal(
    gitTagExists(existing, ROOT),
    true,
    `真实存在的 tag ${existing} 必须被识别`
  );
});
