import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 打包与签名的跨端契约（任务书 T14 / PR-15）。
//
//   1. 审计结论钉住：release.yml 的矩阵含 macOS aarch64 + x86_64 + Windows；
//   2. macOS 签名步骤以 secret 存在与否为条件（未配置不失败）；
//   3. Windows 签名步骤同上；
//   4. secrets 名称与 SOP 文档清单一致（8 个）；
//   5. README 写明 Intel 下载行与双平台首次打开方式。
// ---------------------------------------------------------------------------

const RELEASE_YML = path.join(repoRoot, '.github/workflows/release.yml');
const README = path.join(repoRoot, 'README.md');
const SOP_DOC = path.join(repoRoot, 'Docs/dual-remote-release-flow.md');

const releaseYml = fs.readFileSync(RELEASE_YML, 'utf8');
const readme = fs.readFileSync(README, 'utf8');
const sopDoc = fs.readFileSync(SOP_DOC, 'utf8');

test('1. 构建矩阵：aarch64 + x86_64 + Windows 三目标', () => {
  assert.ok(releaseYml.includes(`--target aarch64-apple-darwin`), '缺 Apple Silicon 目标');
  assert.ok(releaseYml.includes(`--target x86_64-apple-darwin`), '缺 Intel 目标（T14 新增）');
  assert.ok(releaseYml.includes(`- platform: 'windows-latest'`), '缺 Windows 目标');
});

test('2. macOS 签名：证书导入与签名变量都以 secret 为条件', () => {
  const importStep = releaseYml.indexOf('Import Apple signing certificate');
  assert.ok(importStep > 0, '缺证书导入步骤');
  const importBlock = releaseYml.slice(importStep, importStep + 1500);
  // 条件：secret 为空时跳过
  assert.ok(
    importBlock.includes(`env.APPLE_CERTIFICATE != ''`),
    '证书导入必须以 APPLE_CERTIFICATE 已配置为条件'
  );

  // 签名变量必须通过 GITHUB_ENV 条件导出，不能直接传空字符串给 tauri-action：
  // 空字符串会被 tauri 当成「要签名」，执行 codesign --sign "" 后失败。
  const exportStep = releaseYml.indexOf('Export Apple signing env');
  assert.ok(exportStep > 0, '缺 Apple 签名变量条件导出步骤');
  const exportBlock = releaseYml.slice(exportStep, exportStep + 1200);
  assert.ok(
    exportBlock.includes(`env.APPLE_SIGNING_IDENTITY_SECRET != ''`),
    '签名变量导出必须以 APPLE_SIGNING_IDENTITY 已配置为条件'
  );
  for (const name of ['APPLE_SIGNING_IDENTITY', 'APPLE_ID', 'APPLE_PASSWORD', 'APPLE_TEAM_ID']) {
    assert.ok(
      exportBlock.includes(`${name}=$${name}_SECRET`),
      `签名变量 ${name} 必须通过 GITHUB_ENV 条件导出`
    );
    assert.ok(
      !releaseYml.includes(`${name}: \${{ secrets.${name} }}`),
      `${name} 不能直接以空字符串形式传给 tauri-action`
    );
  }
});

test('3. Windows 签名：signtool 步骤以 PFX secret 为条件', () => {
  const signStep = releaseYml.indexOf('Sign Windows installers');
  assert.ok(signStep > 0, '缺 Windows 签名步骤');
  const block = releaseYml.slice(signStep, signStep + 1200);
  assert.ok(
    block.includes(`env.WINDOWS_SIGNING_PFX != ''`),
    'Windows 签名必须以 WINDOWS_SIGNING_PFX 已配置为条件'
  );
  assert.ok(block.includes('signtool'), '必须调用 signtool');
  // 时间戳：签名必须有 RFC3161 时间戳才不被过期证书拖累
  assert.ok(block.includes('/tr http://timestamp.digicert.com'), '签名必须带时间戳');
});

test('4. SOP secrets 清单：8 个名称与工作流引用一致', () => {
  const secrets = [
    'APPLE_CERTIFICATE',
    'APPLE_CERTIFICATE_PASSWORD',
    'APPLE_SIGNING_IDENTITY',
    'APPLE_ID',
    'APPLE_PASSWORD',
    'APPLE_TEAM_ID',
    'WINDOWS_SIGNING_PFX',
    'WINDOWS_SIGNING_PFX_PASSWORD',
  ];
  for (const name of secrets) {
    assert.ok(sopDoc.includes(`\`${name}\``), `SOP 缺少 secret ${name}`);
    assert.ok(releaseYml.includes(`secrets.${name}`), `release.yml 未引用 ${name}`);
  }
});

test('5. README：Intel 下载行 + 双平台首次打开方式', () => {
  assert.ok(readme.includes('macOS (Intel)'), 'README 下载表缺 Intel 行');
  assert.ok(readme.includes('xattr -cr'), 'README 缺 macOS 首次打开命令');
  assert.ok(readme.includes('仍要打开'), 'README 缺 macOS 系统设置路径');
  assert.ok(readme.includes('仍要运行此应用'), 'README 缺 Windows SmartScreen 指引');
  assert.ok(readme.includes('SHA256SUMS.txt'), 'README 应提示核对 SHA-256');
});
