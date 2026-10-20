/**
 * 版本一致性门禁脚本的测试（V0.3.8 补充任务书 P1-5）。
 *
 * `scripts/check-version.mjs` 在上一轮已经写好并接入 CI，但**从未被测试过** ——
 * 一个没被验证的门禁脚本，和一个不存在的门禁脚本，在发布当天的表现是一样的。
 * 这里在临时目录里造出完整的版本文件树，真实执行脚本并断言退出码。
 *
 * 运行：node --test tests/scripts/check_version.test.js
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SCRIPT = join(ROOT, 'scripts', 'check-version.mjs');

/**
 * 在临时目录里搭出一棵与真实仓库同构的版本文件树。
 *
 * 必须用临时目录而不是改真仓库：门禁的正向与负向用例都需要
 * 制造「版本不一致」「changelog 缺条目」这类状态，
 * 在真仓库里做等于把发布配置改坏。
 */
function fixture({
  version = '1.2.3',
  cargoVersions,
  tauriVersion,
  changelog = true,
} = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'ew-checkver-'));
  const cargo = {
    'src-tauri/Cargo.toml': version,
    'shared-types/Cargo.toml': version,
    'frontend/Cargo.toml': version,
    ...(cargoVersions ?? {}),
  };

  mkdirSync(join(dir, 'scripts'), { recursive: true });
  copyFileSync(SCRIPT, join(dir, 'scripts', 'check-version.mjs'));

  writeFileSync(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'easy-writing', version }, null, 2)
  );
  for (const [rel, v] of Object.entries(cargo)) {
    const abs = join(dir, rel);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, `[package]\nname = "x"\nversion = "${v}"\n\n[dependencies]\nversion = "9.9.9"\n`);
  }
  mkdirSync(join(dir, 'src-tauri'), { recursive: true });
  writeFileSync(
    join(dir, 'src-tauri', 'tauri.conf.json'),
    JSON.stringify({ version: tauriVersion ?? version }, null, 2)
  );
  if (changelog) {
    writeFileSync(join(dir, 'CHANGELOG.md'), `# Changelog\n\n## [${version}] - 2026-01-01\n\n- x\n`);
  }
  return dir;
}

function run(dir, args = []) {
  const res = spawnSync(process.execPath, [join(dir, 'scripts', 'check-version.mjs'), ...args], {
    cwd: dir,
    encoding: 'utf8',
  });
  return { code: res.status, out: `${res.stdout ?? ''}${res.stderr ?? ''}` };
}

test('版本一致且 tag 匹配时通过', () => {
  const dir = fixture();
  try {
    const { code, out } = run(dir, ['--tag', 'v1.2.3', '--strict']);
    assert.equal(code, 0, `应当通过，输出：${out}`);
    assert.match(out, /版本一致性检查通过/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('tag 与构建版本不一致时失败', () => {
  // 这是原始缺陷：推 v0.3.9 的 tag 而配置还是 0.3.7，发布会照常成功，
  // 只产出一个版本号错乱的安装包。
  const dir = fixture();
  try {
    const { code, out } = run(dir, ['--tag', 'v9.9.9']);
    assert.equal(code, 1, `tag 不匹配必须失败，输出：${out}`);
    assert.match(out, /发布 tag 版本不匹配/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('分支名当 tag 时失败', () => {
  const dir = fixture();
  try {
    const { code, out } = run(dir, ['--tag', 'main']);
    assert.equal(code, 1, `分支名不能当发布 tag，输出：${out}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('各版本字段互相不一致时失败', () => {
  const dir = fixture({
    cargoVersions: { 'src-tauri/Cargo.toml': '1.2.4' },
  });
  try {
    const { code, out } = run(dir);
    assert.equal(code, 1, `字段不一致必须失败，输出：${out}`);
    assert.match(out, /版本不一致/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('tauri.conf.json 版本落后时失败', () => {
  const dir = fixture({ tauriVersion: '1.2.2' });
  try {
    const { code } = run(dir, ['--tag', 'v1.2.3']);
    assert.equal(code, 1);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('缺少 changelog 条目：非 strict 只告警', () => {
  // 非 strict 下放行是有意的：否则会卡死「先发版再补 changelog」的正常流程。
  const dir = fixture({ changelog: false });
  try {
    const { code, out } = run(dir, ['--tag', 'v1.2.3']);
    assert.equal(code, 0, `非 strict 应只告警，输出：${out}`);
    assert.match(out, /警告/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('缺少 changelog 条目：strict 下失败', () => {
  // 正式发布走的就是 --strict，因此「没有发布说明」必须挡住发布。
  const dir = fixture({ changelog: false });
  try {
    const { code, out } = run(dir, ['--tag', 'v1.2.3', '--strict']);
    assert.equal(code, 1, `strict 下缺 changelog 必须失败，输出：${out}`);
    assert.match(out, /CHANGELOG/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('版本文件缺失时失败', () => {
  const dir = fixture();
  rmSync(join(dir, 'frontend', 'Cargo.toml'));
  try {
    const { code, out } = run(dir);
    assert.equal(code, 1, `缺文件必须失败而不是跳过，输出：${out}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('当前仓库的版本字段确实一致', () => {
  // 顺带把真仓库也跑一遍：这条用例失败说明有人改了版本号却没同步全部 6 处。
  const r = spawnSync(process.execPath, [SCRIPT], { cwd: ROOT, encoding: 'utf8' });
  const out = `${r.stdout ?? ''}${r.stderr ?? ''}`;
  assert.equal(r.status, 0, `当前仓库版本字段必须一致，输出：${out}`);
});
