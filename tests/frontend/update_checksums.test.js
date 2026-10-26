import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 更新校验和的跨端契约（任务书 T13 / PR-14）。
//
//   1. release.yml 有 checksums job：等全平台产物上传后生成 SHA256SUMS.txt
//      并传回 Release（GitHub 端自动化）；
//   2. shared-types 的 AppUpdateAsset 带 sha256 字段；
//   3. 后端解析 SHA256SUMS 并按文件名附加（update_cmd.rs）；
//   4. 前端更新弹窗展示 SHA-256 与校验方法，空值不展示；
//   5. 应用保持「只跳转下载页、不自动安装」——不得出现自动安装/下载执行字样。
// ---------------------------------------------------------------------------

const RELEASE_YML = path.join(repoRoot, '.github/workflows/release.yml');
const SHARED_TYPES_SRC = path.join(repoRoot, 'shared-types/src/lib.rs');
const UPDATE_CMD_SRC = path.join(repoRoot, 'src-tauri/src/commands/update_cmd.rs');
const UPDATE_MODAL_SRC = path.join(repoRoot, 'frontend/src/components/config/update_modal.rs');
const SOP_DOC = path.join(repoRoot, 'Docs/dual-remote-release-flow.md');

const releaseYml = fs.readFileSync(RELEASE_YML, 'utf8');
const sharedTypes = fs.readFileSync(SHARED_TYPES_SRC, 'utf8');
const updateCmd = fs.readFileSync(UPDATE_CMD_SRC, 'utf8');
const updateModal = fs.readFileSync(UPDATE_MODAL_SRC, 'utf8');
const sopDoc = fs.readFileSync(SOP_DOC, 'utf8');

test('1. release.yml：checksums job 在 gate+release 之后，生成并上传 SHA256SUMS.txt', () => {
  assert.ok(/checksums:/.test(releaseYml), '缺少 checksums job');
  const jobBlock = releaseYml.slice(releaseYml.indexOf('checksums:'));
  assert.ok(jobBlock.includes('needs: [gate, release]'), 'checksums 必须等 gate 与 release 完成');
  assert.ok(jobBlock.includes('SHA256SUMS.txt'), '必须生成 SHA256SUMS.txt');
  assert.ok(jobBlock.includes('gh release upload'), '必须上传回 Release');
  // 生成命令必须是标准格式（可被 sha256sum -c 校验）
  assert.ok(jobBlock.includes('sha256sum --binary * > SHA256SUMS.txt'));
});

test('2. shared-types：AppUpdateAsset.sha256 字段存在且默认空串', () => {
  assert.ok(sharedTypes.includes('pub sha256: String'), 'AppUpdateAsset 缺少 sha256 字段');
});

test('3. 后端：解析 SHA256SUMS 并按文件名附加', () => {
  assert.ok(updateCmd.includes('pub fn parse_sha256sums'), '缺少解析函数');
  assert.ok(updateCmd.includes('pub fn attach_sha256_to_assets'), '缺少附加函数');
  assert.ok(updateCmd.includes('SHA256SUMS_NAME'), '缺少常量');
  // 附加逻辑必须真的接进 check_app_update：发现 sums 资产时拉取解析
  assert.ok(
    updateCmd.includes('attach_sha256_to_assets(assets, &parse_sha256sums(&content))'),
    'check_app_update 未接入校验和附加'
  );
});

test('4. 更新弹窗：展示 SHA-256 与校验方法，空值不展示', () => {
  assert.ok(updateModal.includes('SHA-256'), '弹窗缺少 SHA-256 标签');
  assert.ok(updateModal.includes('shasum -a 256'), '缺少 macOS 校验方法');
  assert.ok(updateModal.includes('certutil -hashfile'), '缺少 Windows 校验方法');
  // 空值分支
  assert.ok(updateModal.includes('if !sha.is_empty()'), '哈希为空时必须不展示');
});

test('5. 保持「只跳转下载页、不自动安装」', () => {
  const modalAndCmd = updateModal + updateCmd;
  for (const banned of ['auto_install', 'autoUpdate', 'install_command', '自动安装']) {
    assert.ok(!modalAndCmd.includes(banned), `不得出现自动安装相关字样：${banned}`);
  }
  assert.ok(updateModal.includes('on_open_download'), '仍通过 open_external_url 跳转');
});

test('6. 发布 SOP 文档：GitHub CI 自动生成 + CNB 手动同步说明', () => {
  assert.ok(sopDoc.includes('SHA256SUMS'), 'SOP 缺少校验和章节');
  assert.ok(sopDoc.includes('checksums'), 'SOP 未引用 CI 的 checksums job');
  assert.ok(sopDoc.includes('certutil -hashfile') || sopDoc.includes('shasum -a 256'), 'SOP 缺少校验方法');
});
