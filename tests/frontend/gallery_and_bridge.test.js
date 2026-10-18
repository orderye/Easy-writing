import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// 真导入被测实现，而不是在测试里手抄一份。
//
// asset_rel_to_url / copy_text_to_clipboard 通过
//   #[wasm_bindgen(inline_js = r#"..."#)]
// 内嵌在 frontend/src/bridge/asset.rs 里，运行时由 wasm-bindgen 生成 ESM 片段加载。
// 为了让测试跑在**同一份源码**上（实现改了测试就必须红），这里把 inline_js 块
// 原样抽出来落到临时 .mjs，再 import 进来。
//
// 旧写法是把实现抄一遍再断言 —— 那测的是测试文件自己，改被测代码不会失败。
// ---------------------------------------------------------------------------
const ASSET_BRIDGE_SRC = path.join(repoRoot, 'frontend/src/bridge/asset.rs');

function extractInlineJsBlocks(rustSource) {
  const blocks = [];
  const re = /inline_js\s*=\s*r#"([\s\S]*?)"#/g;
  let m;
  while ((m = re.exec(rustSource)) !== null) blocks.push(m[1]);
  assert.ok(
    blocks.length > 0,
    'asset.rs 里找不到 inline_js 块 —— 抽取逻辑已失效，请同步更新本测试'
  );
  return blocks;
}

const tmpModulePath = path.join(os.tmpdir(), `ew-asset-bridge-${process.pid}.mjs`);
fs.writeFileSync(
  tmpModulePath,
  extractInlineJsBlocks(fs.readFileSync(ASSET_BRIDGE_SRC, 'utf8')).join('\n')
);
const { asset_rel_to_url, copy_text_to_clipboard, canonicalize_asset_src } = await import(
  pathToFileURL(tmpModulePath).href
);

// 真实实现读的是全局 window / navigator / document（不是参数），
// 所以这里临时替换全局对象来注入桩，跑完恢复。
function withGlobals(overrides, fn) {
  const saved = new Map();
  for (const [key, value] of Object.entries(overrides)) {
    saved.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  try {
    return fn();
  } finally {
    for (const [key, desc] of saved) {
      if (desc) Object.defineProperty(globalThis, key, desc);
      else delete globalThis[key];
    }
  }
}

test('1. 缩略图渲染：相对路径转为 Tauri asset URL', () => {
  const appData = '/Users/roy/Library/Application Support/Easy-writing';

  // 1.1 空路径（真实实现用 !relPath 判断，'' 与 null 都返回空串）
  assert.equal(asset_rel_to_url('', appData), '');
  assert.equal(asset_rel_to_url(null, appData), '');

  // 1.2 已是可直接使用的 URL：原样返回，**绝不**送进 convertFileSrc。
  //
  // 这里必须把 convertFileSrc 桩成"会破坏输入"的形式：
  // 如果不加白名单判断而直接转换，返回值就会被改成 asset://localhost/...，
  // 断言随即失败。用普通的等值断言抓不到这个回归 —— 因为末尾的兜底
  // return relPath 会返回同一个字符串，看起来"也对"。
  const mangle = {
    window: {
      __TAURI__: {
        core: { convertFileSrc: (p) => `asset://localhost/${encodeURIComponent(p)}` },
      },
    },
  };
  for (const u of [
    'https://example.com/cover.jpg',
    'asset://localhost/Users/img.png',
    'data:image/png;base64,abc',
    'blob:http://localhost/abc',
  ]) {
    assert.equal(
      withGlobals(mangle, () => asset_rel_to_url(u, appData)),
      u,
      `${u} 被错误地送进了 convertFileSrc（协议白名单失效）`
    );
  }

  // 1.3 有 Tauri convertFileSrc：拼出绝对路径后转换
  const rel = 'images/abc12345.png';
  const converted = withGlobals(
    {
      window: {
        __TAURI__: {
          core: { convertFileSrc: (p) => `asset://localhost/${encodeURIComponent(p)}` },
        },
      },
    },
    () => asset_rel_to_url(rel, appData)
  );
  assert.equal(converted, `asset://localhost/${encodeURIComponent(appData + '/' + rel)}`);

  // 1.4 无 Tauri 桥接：优雅降级返回原相对路径，不崩溃
  const fallback = withGlobals({ window: {} }, () => asset_rel_to_url(rel, appData));
  assert.equal(fallback, rel);

  // 1.5 appDataDir 末尾多余的斜杠必须被规整，不能拼出双斜杠
  const trailing = withGlobals(
    { window: { __TAURI__: { core: { convertFileSrc: (p) => p } } } },
    () => asset_rel_to_url(rel, appData + '///')
  );
  assert.equal(trailing, `${appData}/${rel}`);

  // 1.6 支持 asset://local/ 相对协议动态解为运行时 URL
  const canonical = 'asset://local/images/novel_1/scene.png';
  const convertedCanonical = withGlobals(
    {
      window: {
        __TAURI__: {
          core: { convertFileSrc: (p) => `asset://localhost/${encodeURIComponent(p)}` },
        },
      },
    },
    () => asset_rel_to_url(canonical, appData)
  );
  assert.equal(
    convertedCanonical,
    `asset://localhost/${encodeURIComponent(appData + '/images/novel_1/scene.png')}`
  );

  // 1.7 canonicalize_asset_src 将绝对路径或运行时 URL 归一化为跨设备通用的 asset://local/ 协议
  assert.equal(
    canonicalize_asset_src(`asset://localhost/${encodeURIComponent(appData + '/images/novel_1/scene.png')}`),
    'asset://local/images/novel_1/scene.png'
  );
});

test('2. 复制路径：现代 Clipboard API 与 execCommand 回退', () => {
  // 2.1 优先 navigator.clipboard.writeText
  let writtenText = '';
  const ok1 = withGlobals(
    {
      navigator: { clipboard: { writeText: (t) => { writtenText = t; } } },
      document: undefined,
    },
    () => copy_text_to_clipboard('images/cover_01.png')
  );
  assert.equal(ok1, true);
  assert.equal(writtenText, 'images/cover_01.png');

  // 2.2 回退 document.execCommand('copy')，且必须清理临时节点
  let execCalled = false;
  let removedChild = false;
  const ok2 = withGlobals(
    {
      navigator: {},
      document: {
        createElement: () => ({ style: {}, select() {} }),
        body: {
          appendChild() {},
          removeChild() { removedChild = true; },
        },
        execCommand: (cmd) => {
          if (cmd === 'copy') execCalled = true;
          return true;
        },
      },
    },
    () => copy_text_to_clipboard('images/cover_02.png')
  );
  assert.equal(ok2, true);
  assert.equal(execCalled, true);
  assert.equal(removedChild, true);

  // 2.3 两条路都不可用：返回 false，绝不抛异常
  const ok3 = withGlobals({ navigator: {}, document: {} }, () =>
    copy_text_to_clipboard('x')
  );
  assert.equal(ok3, false);
});

test('3. 图库「缺失」徽标的状态语义（纯逻辑示意，不覆盖真实实现）', () => {
  // 说明：本用例只是一份状态语义备忘，图中的 Set 与判定函数都是测试内自造的，
  // 与 asset_gallery.rs 的真实实现没有任何绑定关系，不构成覆盖率。
  const brokenSet = new Set();
  const reportError = (relPath) => {
    brokenSet.add(relPath);
  };

  assert.equal(brokenSet.size, 0);
  reportError('images/missing_one.png');
  reportError('images/corrupted.jpg');
  assert.equal(brokenSet.size, 2);

  const getBadgeState = (relPath) => {
    const isBroken = brokenSet.has(relPath);
    return {
      isBroken,
      badgeText: isBroken ? '⚠ 缺失' : '',
      borderClass: isBroken
        ? 'border-amber-300 dark:border-amber-800'
        : 'border-slate-200 dark:border-slate-700',
    };
  };

  assert.equal(getBadgeState('images/fine.png').isBroken, false);
  assert.equal(getBadgeState('images/fine.png').badgeText, '');

  const broken = getBadgeState('images/missing_one.png');
  assert.equal(broken.isBroken, true);
  assert.equal(broken.badgeText, '⚠ 缺失');
  assert.ok(broken.borderClass.includes('amber'));
});

test('4. 深色模式与面板收起的布局：编译生成的样式与结构规则', () => {
  const cssPath = path.join(repoRoot, 'frontend/public/styles.css');
  const cssContent = fs.readFileSync(cssPath, 'utf8');

  assert.ok(cssContent.includes('.dark'), 'CSS 中缺少 .dark 基础规则');
  assert.ok(
    cssContent.includes('dark\\:bg-slate-800') ||
      cssContent.includes('dark:bg-slate-800') ||
      cssContent.includes('--color-slate-800'),
    '缺少深色背景支持'
  );
  assert.ok(
    cssContent.includes('dark\\:border-slate-700') ||
      cssContent.includes('dark:border-slate-700'),
    '缺少深色边框支持'
  );

  const galleryCodePath = path.join(repoRoot, 'frontend/src/components/editor/asset_gallery.rs');
  const galleryCode = fs.readFileSync(galleryCodePath, 'utf8');

  assert.ok(galleryCode.includes('fixed inset-0'), '图库 Modal 根节点必须采用 fixed inset-0 悬浮视口');
  assert.ok(galleryCode.includes('z-50'), '图库 Modal 需具备高 z-index（z-50）以遮罩侧栏');
  assert.ok(galleryCode.includes('grid grid-cols-3'), '图库主体须具备弹性网格列布局');
  assert.ok(galleryCode.includes('w-64 shrink-0'), '右侧详情栏需固定宽度并禁止收缩');
});

test('5. 空工作台与防孤儿输入契约：最后一章删除后清空并禁用编辑器，并展示引导空状态', () => {
  const writingStagePath = path.join(repoRoot, 'frontend/src/components/layout/writing_stage.rs');
  const writingStageCode = fs.readFileSync(writingStagePath, 'utf8');

  assert.ok(writingStageCode.includes('当前没有打开的章节'), 'WritingStage 必须包含无打开章节时的引导文案');
  assert.ok(writingStageCode.includes('新建章节'), 'WritingStage 必须提供新建章节的操作引导按钮');

  const chapterListPath = path.join(repoRoot, 'frontend/src/components/structure_panel/chapter_list.rs');
  const chapterListCode = fs.readFileSync(chapterListPath, 'utf8');

  assert.ok(chapterListCode.includes('tiptap::set_editable(false)'), '删除最后一章后必须调用 set_editable(false) 关闭输入');
  assert.ok(chapterListCode.includes('tiptap::set_content("")'), '删除最后一章后必须调用 set_content("") 清空编辑器残留内容');

  const toolbarPath = path.join(repoRoot, 'frontend/src/components/editor/toolbar.rs');
  const toolbarCode = fs.readFileSync(toolbarPath, 'utf8');
  assert.ok(toolbarCode.includes('pointer-events-none'), '无章节时工具栏必须禁用点击交互');
});

test('6. 拖拽排序与跨卷移动失败回滚契约：落库失败立刻恢复原顺序并从底层数据库重新对齐', () => {
  const volumeListPath = path.join(repoRoot, 'frontend/src/components/structure_panel/volume_list.rs');
  const volumeListCode = fs.readFileSync(volumeListPath, 'utf8');

  // 验证 reorder_chapters 错误拦截与回滚
  assert.ok(volumeListCode.includes('章节排序保存失败，已恢复原顺序'), '同卷重排失败时必须向用户弹出明确回滚提示');
  assert.ok(volumeListCode.includes('reload_chapters_from_db'), '同卷重排失败时必须触发重新从数据库拉取真实顺序');

  // 验证 move_chapter 错误拦截与回滚
  assert.ok(volumeListCode.includes('跨卷移动章节保存失败，已恢复原位置'), '跨卷移动失败时必须向用户弹出明确回滚提示');

  // 验证 reorder_volumes 错误拦截与回滚
  assert.ok(volumeListCode.includes('分卷排序保存失败，已恢复原顺序'), '分卷重排失败时必须向用户弹出明确回滚提示');
  assert.ok(volumeListCode.includes('reload_volumes_from_db'), '分卷重排失败时必须触发重新从数据库拉取真实顺序');

  // 纯逻辑状态回滚断言
  const mockChapters = [
    { id: 'c1', sort_order: 0, title: '第一章' },
    { id: 'c2', sort_order: 1, title: '第二章' },
    { id: 'c3', sort_order: 2, title: '第三章' },
  ];
  let currentList = [...mockChapters];
  const rollbackList = [...mockChapters];

  // 模拟乐观移动：c3 移动到第一位
  const [c3] = currentList.splice(2, 1);
  currentList.unshift(c3);
  assert.equal(currentList[0].id, 'c3');

  // 模拟数据库落库失败触发回滚
  const simulateIpcFail = true;
  if (simulateIpcFail) {
    currentList = rollbackList;
  }
  assert.equal(currentList[0].id, 'c1');
  assert.equal(currentList[2].id, 'c3');
});


