/**
 * 文档 Schema 护栏脚本的测试。
 *
 * `scripts/check-docs-schema.mjs` 断言《实施计划》§五 的表清单与
 * `src-tauri/migrations/` 实际建表一致。这个护栏要能挡住「迁移加了表、
 * 文档没跟上」，所以它自己也必须被测过 —— 一个没被验证的门禁，
 * 和一个不存在的门禁，在出问题的当天表现是一样的。
 *
 * 全部用例都在临时目录里造文件树并真实执行脚本：
 * 正向用例需要「两边一致」，负向用例需要「故意制造不一致」，
 * 在真仓库里做这些等于把 Schema 改坏。
 *
 * 运行：node --test tests/scripts/check_docs_schema.test.js
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SCRIPT = join(ROOT, 'scripts', 'check-docs-schema.mjs');

/** 一份最小的、格式与真实迁移一致的建表语句。 */
function ddl(table, extraColumn = '') {
  return `CREATE TABLE IF NOT EXISTS ${table} (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL DEFAULT ''${extraColumn}
);
`;
}

/**
 * 在临时目录里搭出一棵与真实仓库同构的 Schema 文件树。
 *
 * @param {object} options
 * @param {string[]} options.migrationTables 迁移里建的表
 * @param {string[]} options.docTables       文档 §五 里列的表
 * @param {string}   options.migrationExtra  迁移里额外的语句（如 ALTER ADD COLUMN）
 * @param {string}   options.docExtra        文档里额外的列
 */
function fixture({
  migrationTables = ['novel'],
  docTables = ['novel'],
  migrationExtra = '',
  docExtra = '',
} = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'ew-docschema-'));

  mkdirSync(join(dir, 'scripts'), { recursive: true });
  copyFileSync(SCRIPT, join(dir, 'scripts', 'check-docs-schema.mjs'));

  mkdirSync(join(dir, 'src-tauri', 'migrations'), { recursive: true });
  writeFileSync(
    join(dir, 'src-tauri', 'migrations', 'V001__init.sql'),
    migrationTables.map((t) => ddl(t)).join('\n') + migrationExtra
  );

  mkdirSync(join(dir, 'Docs'), { recursive: true });
  const section = docTables
    .map((t) =>
      t === 'novel'
        ? ddl(t, docExtra).replace('IF NOT EXISTS ', '')
        : ddl(t).replace('IF NOT EXISTS ', '')
    )
    .join('\n');
  writeFileSync(
    join(dir, 'Docs', 'Easy-writing · 实施计划.md'),
    `# 实施计划\n\n## 五、数据库 Schema（当前基线）\n\n\`\`\`sql\n${section}\`\`\`\n\n## 六、下一章\n\n结束。\n`
  );
  return dir;
}

function run(dir, args = []) {
  const res = spawnSync(process.execPath, [join(dir, 'scripts', 'check-docs-schema.mjs'), ...args], {
    cwd: dir,
    encoding: 'utf8',
  });
  return { code: res.status, out: `${res.stdout ?? ''}${res.stderr ?? ''}` };
}

test('迁移与文档表清单一致时通过', () => {
  const dir = fixture();
  try {
    const { code, out } = run(dir);
    assert.equal(code, 0, `应当通过，输出：${out}`);
    assert.match(out, /表名双向比对\s+✅/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('迁移里有文档没记的表时失败', () => {
  // 这是护栏要挡住的真实事故：迁移加了表，文档没跟上，
  // 读者照着文档理解数据模型就会漏掉一整张表。
  const dir = fixture({ migrationTables: ['novel', 'chapter'], docTables: ['novel'] });
  try {
    const { code, out } = run(dir);
    assert.equal(code, 1, `文档缺表必须失败，输出：${out}`);
    assert.match(out, /迁移里有表 chapter/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('文档里有迁移不存在的表时失败', () => {
  // 反向失真：文档描述了一张从未被创建的表。
  const dir = fixture({ migrationTables: ['novel'], docTables: ['novel', 'ghost'] });
  try {
    const { code, out } = run(dir);
    assert.equal(code, 1, `文档多表必须失败，输出：${out}`);
    assert.match(out, /ghost/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('表一致但列有差异：默认只告警', () => {
  // 列级差异默认不阻断：§五 是完整 DDL 文本，
  // 约束与格式的解析比表名脆弱，把它设成硬门禁只会让 CI 因误报变红。
  const dir = fixture({
    migrationTables: ['novel'],
    docTables: ['novel'],
    migrationExtra: "\nALTER TABLE novel ADD COLUMN word_count INTEGER NOT NULL DEFAULT 0;\n",
  });
  try {
    const { code, out } = run(dir);
    assert.equal(code, 0, `非 strict 应只告警，输出：${out}`);
    assert.match(out, /警告/);
    assert.match(out, /word_count/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('列差异在 --strict 下失败', () => {
  const dir = fixture({
    migrationTables: ['novel'],
    docTables: ['novel'],
    migrationExtra: "\nALTER TABLE novel ADD COLUMN word_count INTEGER NOT NULL DEFAULT 0;\n",
  });
  try {
    const { code, out } = run(dir, ['--strict']);
    assert.equal(code, 1, `strict 下列差异必须失败，输出：${out}`);
    assert.match(out, /word_count/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('缩进过的表结束标记（` );`）也能被解析', () => {
  // 迁移文件里两种收尾写法都有：顶格 `);` 和缩进的 ` );`。
  // 只认顶格的那一张表会整张漏掉，于是「迁移有、文档无」变成假错误。
  const dir = mkdtempSync(join(tmpdir(), 'ew-docschema-'));
  try {
    mkdirSync(join(dir, 'scripts'), { recursive: true });
    copyFileSync(SCRIPT, join(dir, 'scripts', 'check-docs-schema.mjs'));
    mkdirSync(join(dir, 'src-tauri', 'migrations'), { recursive: true });
    writeFileSync(
      join(dir, 'src-tauri', 'migrations', 'V001__init.sql'),
      `CREATE TABLE IF NOT EXISTS novel (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL DEFAULT 'x' CHECK(kind IN ('a', 'b'))
 );
`
    );
    mkdirSync(join(dir, 'Docs'), { recursive: true });
    writeFileSync(
      join(dir, 'Docs', 'Easy-writing · 实施计划.md'),
      `# 实施计划\n\n## 五、数据库 Schema\n\n\`\`\`sql\nCREATE TABLE novel (\n    id          TEXT PRIMARY KEY,\n    title       TEXT NOT NULL DEFAULT '',\n    kind        TEXT NOT NULL DEFAULT 'x' CHECK(kind IN ('a', 'b'))\n);\n\`\`\`\n\n## 六、下一章\n`
    );
    const { code, out } = run(dir);
    assert.equal(code, 0, `缩进收尾的表必须被正确解析，输出：${out}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('当前仓库的 §五 与迁移确实一致', () => {
  // 真仓库也跑一遍：这条用例失败说明有人加了迁移却没同步《实施计划》§五。
  const res = spawnSync(process.execPath, [SCRIPT], { cwd: ROOT, encoding: 'utf8' });
  const out = `${res.stdout ?? ''}${res.stderr ?? ''}`;
  assert.equal(res.status, 0, `当前仓库文档与迁移必须一致，输出：${out}`);
});
