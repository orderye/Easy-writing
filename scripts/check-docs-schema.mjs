#!/usr/bin/env node
/**
 * 文档 Schema 护栏：校验《实施计划》§五 的表清单与 `src-tauri/migrations/` 实际迁移一致。
 *
 * 背景：实施计划 §五 长期停留在「立项时 V001 的初始 DDL 基线」，
 * 而工程早已通过阶梯迁移演进到 V003。文档与代码各说各话时，
 * 没有任何人会在迁移里加完表后想起来回头改文档 ——
 * 这份失真是无声的：CI 全绿、文档照旧，直到有人照着文档写代码才踩坑。
 *
 * 本脚本把这条规则固化为可执行检查：
 *   - **表名双向比对是硬门禁**：迁移里有而文档没有 → 失败；
 *     文档里有而迁移里没有 → 失败（文档在描述不存在的表）。
 *   - **列级差异默认只告警**：§五 是完整 DDL 文本，
 *     约束与格式的解析比表名脆弱得多，把列差异设成硬门禁只会让 CI 因误报变红，
 *     久而久之就没人再看它。`--strict` 可把告警升级为阻断。
 *
 * 用法：
 *   node scripts/check-docs-schema.mjs            # 表名双向校验（列差异仅告警）
 *   node scripts/check-docs-schema.mjs --strict   # 列差异也阻断
 *
 * Exit 0 = 通过；Exit 1 = 失败（阻断 CI）。
 */

import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const MIGRATIONS_DIR = join(ROOT, 'src-tauri', 'migrations');
const DOC_PATH = join(ROOT, 'Docs', 'Easy-writing · 实施计划.md');

const args = process.argv.slice(2);
const strict = args.includes('--strict');

/**
 * 从 CREATE TABLE 语句体里取出列名集合。
 */
function extractColumns(body) {
  const cols = new Set();
  for (const rawLine of body.split('\n')) {
    // 行内注释（`--`）可能出现在列定义尾部，先剥掉再判断。
    const line = rawLine.split('--')[0].trim();
    if (!line) continue;
    // 表级约束不是列：PRIMARY KEY / FOREIGN KEY / UNIQUE / CHECK / CONSTRAINT。
    if (/^(PRIMARY|FOREIGN|UNIQUE|CHECK|CONSTRAINT)\b/i.test(line)) continue;
    const m = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s+/);
    if (m) cols.add(m[1]);
  }
  return cols;
}

/**
 * 解析一个 SQL 文本里的建表语句，返回 表名 → 列集合。
 *
 * 表体结束标记必须允许缩进：迁移文件里两种写法都有 ——
 * V001 的多数表是顶格 `);`，而 `chapter_revision` 一类带 CHECK 约束的表
 * 收尾是缩进过的 ` );`。只认顶格写法会整张漏掉这些表，
 * 于是「迁移里有、文档里没有」变成假错误，反过来「文档列名对不上」又成了真错误。
 */
function parseCreateTables(sql) {
  const tables = new Map();
  const re = /CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*\(([\s\S]*?)\n\s*\);/gi;
  for (const m of sql.matchAll(re)) {
    const name = m[1];
    const cols = tables.get(name) ?? new Set();
    for (const c of extractColumns(m[2])) cols.add(c);
    tables.set(name, cols);
  }
  return tables;
}

/** 解析 ALTER TABLE ... ADD COLUMN，返回 表名 → 新增列集合。 */
function parseAddedColumns(sql) {
  const added = new Map();
  const re = /ALTER\s+TABLE\s+([A-Za-z_][A-Za-z0-9_]*)\s+ADD\s+(?:COLUMN\s+)?([A-Za-z_][A-Za-z0-9_]*)/gi;
  for (const m of sql.matchAll(re)) {
    const cols = added.get(m[1]) ?? new Set();
    cols.add(m[2]);
    added.set(m[1], cols);
  }
  return added;
}

/** 读取全部迁移文件并合并出「表 → 列集合」。 */
function readMigrationSchema() {
  const files = readdirSync(MIGRATIONS_DIR)
    .filter((f) => f.endsWith('.sql'))
    .sort();
  const tables = new Map();
  for (const file of files) {
    const sql = readFileSync(join(MIGRATIONS_DIR, file), 'utf8');
    for (const [name, cols] of parseCreateTables(sql)) {
      const merged = tables.get(name) ?? new Set();
      for (const c of cols) merged.add(c);
      tables.set(name, merged);
    }
    for (const [name, cols] of parseAddedColumns(sql)) {
      const merged = tables.get(name) ?? new Set();
      for (const c of cols) merged.add(c);
      tables.set(name, merged);
    }
  }
  return { tables, files };
}

/** 截取实施计划 §五 区段。 */
function extractSchemaSection(doc) {
  const start = doc.indexOf('\n## 五、');
  if (start === -1) throw new Error(`在 ${DOC_PATH} 中找不到「## 五、」章节`);
  const end = doc.indexOf('\n## ', start + 1);
  return doc.slice(start, end === -1 ? doc.length : end);
}

const { tables: migrationTables, files: migrationFiles } = readMigrationSchema();
const doc = readFileSync(DOC_PATH, 'utf8');
const section = extractSchemaSection(doc);
const docTables = parseCreateTables(section);

const errors = [];
const warnings = [];

const migrationNames = [...migrationTables.keys()].sort();
const docNames = [...docTables.keys()].sort();

for (const name of migrationNames) {
  if (!docTables.has(name)) {
    errors.push(`迁移里有表 ${name}，但《实施计划》§五 没有记录 —— 文档落后于代码`);
  }
}
for (const name of docNames) {
  if (!migrationTables.has(name)) {
    errors.push(`《实施计划》§五 记录了表 ${name}，但 src-tauri/migrations/ 里没有这张表 —— 文档描述了不存在的表`);
  }
}

// 列级差异：两张表都存在才可比。
for (const name of migrationNames) {
  if (!docTables.has(name)) continue;
  const migCols = migrationTables.get(name);
  const docCols = docTables.get(name);
  const missing = [...migCols].filter((c) => !docCols.has(c)).sort();
  const extra = [...docCols].filter((c) => !migCols.has(c)).sort();
  for (const c of missing) {
    warnings.push(`表 ${name} 有列 ${c}，但《实施计划》§五 未记录`);
  }
  for (const c of extra) {
    warnings.push(`《实施计划》§五 中表 ${name} 有列 ${c}，但迁移文件里没有这一列`);
  }
}

console.log(`迁移来源：src-tauri/migrations/（${migrationFiles.length} 个文件，${migrationNames.length} 张表）`);
console.log(`文档来源：Docs/Easy-writing · 实施计划.md §五（${docNames.length} 张表）`);

if (errors.length === 0) {
  console.log('  表名双向比对        ✅ 一致');
} else {
  console.log('  表名双向比对        ❌ 不一致');
}

for (const w of warnings) console.warn(`⚠️  警告：${w}`);
for (const e of errors) console.error(`❌ 错误：${e}`);

if (warnings.length > 0 && strict) {
  errors.push(`--strict 下 ${warnings.length} 项列级差异被升级为错误`);
}

if (errors.length > 0) {
  console.error(`\n文档 Schema 护栏未通过（${errors.length} 项错误${warnings.length ? `，${warnings.length} 项警告` : ''}）。`);
  process.exit(1);
}
console.log(
  `\n文档 Schema 护栏通过${warnings.length ? `（${warnings.length} 项列级差异告警）` : ''}。`
);
process.exit(0);
