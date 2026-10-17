import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import esbuild from 'esbuild';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const TIPTAP_MAJOR = '2';

/**
 * 三层护栏，分别守住不同的回退路径：
 *   1. package.json —— 防"顺手装了最新大版本"（本仓库历史上真实发生过）
 *   2. pnpm-lock.yaml —— 防锁文件被重新解析回 v3
 *   3. esbuild 模块图 —— 防"版本对了但产物里仍打进两份内核"
 * 三者部分冗余，但各自守一层，缺一层就会漏。
 */

// 取出 semver 范围里的第一个数字段。注意不能用 range.includes("2.") 判断：
// "^3.32.0" 里同样含子串 "2."，会漏判 —— 这个坑在本护栏首次负向测试时被真实踩到过。
const majorOf = (range) => {
  const m = String(range).match(/(\d+)/);
  return m ? m[1] : null;
};

test('1. package.json 中所有 @tiptap/* 依赖统一在 v2 大版本', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(repoRoot, 'package.json'), 'utf8'));
  const entries = Object.entries({ ...pkg.dependencies, ...pkg.devDependencies })
    .filter(([name]) => name.startsWith('@tiptap/'));

  assert.ok(entries.length > 0, '未找到任何 @tiptap/* 依赖，测试本身失效');

  const offMajor = entries.filter(([, range]) => majorOf(range) !== TIPTAP_MAJOR);
  assert.deepEqual(
    offMajor,
    [],
    `@tiptap/* 存在非 v${TIPTAP_MAJOR} 的声明（会导致 @tiptap/core 被打进多份实例）：\n` +
      offMajor.map(([n, r]) => `  ${n}: ${r}`).join('\n')
  );
});

test('2. pnpm-lock.yaml 中不存在 @tiptap/pm v3 条目', () => {
  const lock = fs.readFileSync(path.join(repoRoot, 'pnpm-lock.yaml'), 'utf8');
  const offenders = lock
    .split('\n')
    .filter((line) => /^\s*'?@tiptap\/pm@3\./.test(line));

  assert.deepEqual(
    offenders,
    [],
    `锁文件中出现了 @tiptap/pm v3 条目，说明依赖又被重新解析到了 v3：\n${offenders.join('\n')}`
  );

  // 反向确认 v2 确实在锁文件里（避免"grep 不到"是因为路径/包名写错）
  assert.match(lock, /@tiptap\/pm@2\./, '锁文件中找不到 @tiptap/pm v2 条目，护栏可能已经失效');
});

test('3. 桥接产物中 @tiptap/core 与 @tiptap/pm 各自只有一份实例', async () => {
  const result = await esbuild.build({
    entryPoints: [path.join(repoRoot, 'js/src/tiptap-bridge.js')],
    bundle: true,
    minify: true,
    format: 'iife',
    globalName: 'TiptapBridge',
    write: false,
    metafile: true,
    absWorkingDir: repoRoot,
  });

  const inputs = Object.keys(result.metafile.inputs);
  assert.ok(
    inputs.some((p) => p.endsWith('js/src/tiptap-bridge.js')),
    '模块图里没有入口文件，护栏失效'
  );

  // pnpm 的 peer 变体表现为 .pnpm/@tiptap+core@2.27.3_@tiptap+pm@3.32.0/...
  // 从中取出 <版本>_<peer 后缀> 作为变体标识。
  const variantsOf = (encodedPkg) => {
    const prefix = `node_modules/.pnpm/${encodedPkg}@`;
    const found = new Set();
    for (const p of inputs) {
      const i = p.indexOf(prefix);
      if (i === -1) continue;
      const rest = p.slice(i + prefix.length);
      const slash = rest.indexOf('/');
      found.add(slash === -1 ? rest : rest.slice(0, slash));
    }
    return found;
  };

  const coreVariants = variantsOf('@tiptap+core');
  const pmVariants = variantsOf('@tiptap+pm');

  assert.ok(coreVariants.size > 0, '模块图中找不到 @tiptap/core，护栏失效');
  assert.equal(
    coreVariants.size,
    1,
    `@tiptap/core 被打进多份实例（一份 bundle 里两套编辑器内核）：\n  ${[...coreVariants].join('\n  ')}`
  );
  assert.equal(
    pmVariants.size,
    1,
    `@tiptap/pm 被打进多份实例：\n  ${[...pmVariants].join('\n  ')}`
  );
});
