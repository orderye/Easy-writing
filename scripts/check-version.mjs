#!/usr/bin/env node
/**
 * 版本一致性门禁。
 *
 * 本仓库有 6 处版本字段，全部靠人工同步 —— 历史上每次发版都要手工改 5 个文件，
 * 漏改任何一处都不会有任何报错。而 release.yml 用 tag 作为发布版本，
 * 却不校验 tag 与 tauri.conf.json 是否一致：推 v0.3.9 的 tag 而配置仍是 0.3.7
 * 时，发布流程照样成功，只会产出一个版本号错乱的安装包。
 *
 * 本脚本把这条规则固化为可执行检查，接入 CI 与 release 的第一步。
 *
 * 规则（有意区分「必须一致」与「仅记录」）：
 *   - 必须一致：package.json / 三个 Cargo.toml / tauri.conf.json
 *     —— 这些都会进入构建产物元数据，不一致会让安装包自称错误的版本。
 *   - 仅记录：CHANGELOG 是否存在对应条目 —— 缺失是流程问题，不是产物错误，
 *     因此只告警不阻断（阻断会让「先发版再补 changelog」的正常流程卡死）。
 *
 * 用法：
 *   node scripts/check-version.mjs           # 仅校验字段一致性
 *   node scripts/check-version.mjs --tag v0.3.8   # 额外校验 tag
 *   node scripts/check-version.mjs --strict  # 把 CHANGELOG 缺失也视为失败
 *
 * Exit 0 = 通过；Exit 1 = 失败（阻断 CI / 发布）。
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

const args = process.argv.slice(2);
const strict = args.includes('--strict');
const tagIndex = args.indexOf('--tag');
const expectedTag =
  tagIndex >= 0 && args[tagIndex + 1] && !args[tagIndex + 1].startsWith('--')
    ? args[tagIndex + 1]
    : args.find((a) => a.startsWith('--tag='))
      ? args.find((a) => a.startsWith('--tag=')).slice('--tag='.length)
      : null;

/** 从一个 JSON 文件里取出版本（支持 cargo 的 TOML 用正则单独处理）。 */
function readJsonVersion(relPath, jsonPath) {
  try {
    const raw = JSON.parse(readFileSync(join(ROOT, relPath), 'utf8'));
    return jsonPath.split('.').reduce((acc, k) => acc?.[k], raw);
  } catch (err) {
    return { error: `读取 ${relPath} 失败: ${err.message}` };
  }
}

/** 从一个 Cargo.toml 里取顶层 [package] 的 version（不能用 JSON 解析）。 */
function readCargoVersion(relPath) {
  try {
    const raw = readFileSync(join(ROOT, relPath), 'utf8');
    // 只匹配 [package] 段内的 version，避开 [dependencies] 里的同名键
    const packageSection = raw.split(/^\[/m).find((s) => s.startsWith('package]'));
    if (!packageSection) return { error: `${relPath} 中没有 [package] 段` };
    const m = packageSection.match(/^\s*version\s*=\s*"([^"]+)"/m);
    if (!m) return { error: `${relPath} 的 [package] 中没有 version 字段` };
    return m[1];
  } catch (err) {
    return { error: `读取 ${relPath} 失败: ${err.message}` };
  }
}

const fields = [
  { name: 'package.json', value: readJsonVersion('package.json', 'version') },
  { name: 'src-tauri/Cargo.toml', value: readCargoVersion('src-tauri/Cargo.toml') },
  { name: 'shared-types/Cargo.toml', value: readCargoVersion('shared-types/Cargo.toml') },
  { name: 'frontend/Cargo.toml', value: readCargoVersion('frontend/Cargo.toml') },
  { name: 'src-tauri/tauri.conf.json', value: readJsonVersion('src-tauri/tauri.conf.json', 'version') },
];

const errors = [];
const warnings = [];

// ---- 1. 各字段之间必须一致 ----
const usable = fields.filter((f) => typeof f.value === 'string');
for (const f of fields) {
  if (typeof f.value !== 'string') errors.push(f.value.error ?? `${f.name} 版本缺失`);
}

const canonical = usable[0]?.value;
for (const f of usable.slice(1)) {
  if (f.value !== canonical) {
    errors.push(`版本不一致：${f.name} = ${f.value}，但 ${usable[0].name} = ${canonical}`);
  }
}

console.log(`规范版本（canonical）：${canonical ?? '（无法确定）'}`);
for (const f of fields) {
  console.log(`  ${f.name.padEnd(26)} ${f.value ?? '❌ ' + f.value.error}`);
}

// ---- 2. CHANGELOG 必须有对应条目 ----
if (canonical) {
  let changelog = '';
  try {
    changelog = readFileSync(join(ROOT, 'CHANGELOG.md'), 'utf8');
  } catch {
    warnings.push('CHANGELOG.md 不存在');
  }
  const hasEntry = changelog.includes(`[${canonical}]`);
  if (!hasEntry) {
    const msg = `CHANGELOG.md 缺少 [${canonical}] 条目`;
    if (strict) errors.push(msg);
    else warnings.push(msg);
  } else {
    console.log(`  CHANGELOG.md                  ✅ 含 [${canonical}] 条目`);
  }
}

// ---- 3. 发布 tag 必须与配置版本一致 ----
if (expectedTag !== null) {
  const cleanTag = expectedTag.replace(/^v/i, '');
  if (cleanTag !== canonical) {
    errors.push(
      `发布 tag 版本不匹配：tag = ${expectedTag}（即 ${cleanTag}），但构建配置版本 = ${canonical}。` +
        `请同步 tauri.conf.json / Cargo.toml / package.json 后重新打 tag。`
    );
  } else {
    console.log(`  发布 tag                      ✅ ${expectedTag} 与构建版本一致`);
  }
}

for (const w of warnings) console.warn(`⚠️  警告：${w}`);
for (const e of errors) console.error(`❌ 错误：${e}`);

if (errors.length > 0) {
  console.error(`\n版本一致性检查未通过（${errors.length} 项错误）。`);
  process.exit(1);
}
console.log(`\n版本一致性检查通过${warnings.length ? `（${warnings.length} 项警告）` : ''}。`);
process.exit(0);
