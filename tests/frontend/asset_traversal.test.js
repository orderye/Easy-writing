import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// ---------------------------------------------------------------------------
// T12 · Asset 协议与 CSP：越界路径回归测试
//
// 与 gallery_and_bridge.test.js 一样，从 asset.rs 的 inline_js 块抽取**同一份源码**
// 来测，而不是在测试里手抄实现。
//
// 本文件测的是**前端函数 canonicalize_asset_src** 是否清洗 `..`：
// 结论 —— 未收紧前朴素 `indexOf("images/")` 切片会把 `../` 原样拼回
// `asset://local/`，越界 URL 会被下发。现已收紧：越出 images/ 根返回 ""，
// 根内的 `a/../b.png` 则规范化为 b.png。
//
// 注意边界：Tauri asset 协议自身是否拦截越界 URL 属于**运行时行为**，
// 无法在纯单元测试里验证；那部分由 src-tauri/tests/asset_protocol_scope.rs
// 用 Tauri 自己的 Scope / SafePathBuf 覆盖。本文件不声称验证协议层。
// ---------------------------------------------------------------------------
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const assetBridgeSrc = path.join(repoRoot, 'frontend/src/bridge/asset.rs');

function extractInlineJs(rustSource) {
  const blocks = [];
  const re = /inline_js\s*=\s*r#"([\s\S]*?)"#/g;
  let m;
  while ((m = re.exec(rustSource)) !== null) blocks.push(m[1]);
  assert.ok(blocks.length > 0, 'asset.rs 里找不到 inline_js 块 —— 抽取逻辑已失效');
  return blocks;
}

const tmpModulePath = path.join(os.tmpdir(), `ew-asset-traversal-${process.pid}.mjs`);
fs.writeFileSync(
  tmpModulePath,
  extractInlineJs(fs.readFileSync(assetBridgeSrc, 'utf8')).join('\n')
);
const { canonicalize_asset_src } = await import(pathToFileURL(tmpModulePath).href);

const appData = '/Users/roy/Library/Application Support/Easy-writing';
const runtimeUrl = (abs) => `asset://localhost/${encodeURIComponent(abs)}`;

test('1. 正常路径不受影响：绝对路径 / 运行时 URL / 已是 canonical 协议', () => {
  assert.equal(
    canonicalize_asset_src(runtimeUrl(`${appData}/images/novel_1/scene.png`)),
    'asset://local/images/novel_1/scene.png'
  );
  assert.equal(
    canonicalize_asset_src(`${appData}/images/abc12345.png`),
    'asset://local/images/abc12345.png'
  );
  assert.equal(
    canonicalize_asset_src('asset://local/images/abc12345.png'),
    'asset://local/images/abc12345.png'
  );
  // 非图片路径 / 外链 / data / blob / 空值原样保留，保持向后兼容。
  assert.equal(canonicalize_asset_src('https://example.com/cover.jpg'), 'https://example.com/cover.jpg');
  assert.equal(canonicalize_asset_src('data:image/png;base64,abc'), 'data:image/png;base64,abc');
  assert.equal(canonicalize_asset_src('blob:http://localhost/abc'), 'blob:http://localhost/abc');
  assert.equal(canonicalize_asset_src('foo.png'), 'foo.png');
  assert.equal(canonicalize_asset_src(''), '');
});

test('2. 朴素的 images/ 切片确实保留 ../（收紧前的行为）', () => {
  // 断言的是「原始输入形态」本身 —— 若 canonicalize_asset_src 不再清洗，
  // 下面这些输入必然产出含 ../ 的 URL。本用例把结论钉死在测试里：
  // 不能因为现在返回 "" 就把这些输入从覆盖里删掉。
  const hostile = [
    `${appData}/images/../secret.txt`,
    `${appData}/images/../../etc/passwd`,
    'images/../secret.txt',
    'asset://local/images/../secret.txt',
    runtimeUrl(`${appData}/images/../secret.txt`),
    'images/%2e%2e/secret.txt', // URL 编码的 .. 解码后同样是越界
  ];
  assert.equal(hostile.length, 6);
  for (const src of hostile) {
    const decoded = (() => { try { return decodeURIComponent(src); } catch { return src; } })();
    assert.ok(
      decoded.includes('images/') && decoded.includes('..'),
      `测试输入已失去越界特征：${src}`
    );
  }
});

test('3. 收紧后：越出 images/ 根的 .. 返回空串，不下发越界 URL', () => {
  const escaping = [
    `${appData}/images/../secret.txt`,
    `${appData}/images/../../etc/passwd`,
    'images/../secret.txt',
    'asset://local/images/../secret.txt',
    runtimeUrl(`${appData}/images/../secret.txt`),
    'images/%2e%2e/secret.txt',
  ];
  for (const src of escaping) {
    assert.equal(canonicalize_asset_src(src), '', `越界输入未拒绝：${src}`);
  }
});

test('4. 收紧后：images/ 根内的 .. / . 段被规范化，不误伤合法图片', () => {
  assert.equal(canonicalize_asset_src('images/a/../b.png'), 'asset://local/images/b.png');
  assert.equal(canonicalize_asset_src('images/a/./b.png'), 'asset://local/images/a/b.png');
  assert.equal(canonicalize_asset_src('images//a//b.png'), 'asset://local/images/a/b.png');
  assert.equal(
    canonicalize_asset_src('images/sub/../cover.png'),
    'asset://local/images/cover.png'
  );
});
