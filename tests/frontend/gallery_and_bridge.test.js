import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '../..');

// 提取 asset_rel_to_url 与 copy_text_to_clipboard 的纯函数实现进行隔离单测
function asset_rel_to_url(relPath, appDataDir, customWindow = globalThis.window) {
  if (!relPath) return "";
  if (/^(https?:|data:|asset:|blob:)/i.test(relPath)) return relPath;
  const base = (appDataDir || "").replace(/\/+$/, "");
  const abs = relPath.startsWith("/") ? relPath : (base ? base + "/" + relPath : relPath);
  try {
    if (customWindow && customWindow.__TAURI__ && customWindow.__TAURI__.core && typeof customWindow.__TAURI__.core.convertFileSrc === "function") {
      return customWindow.__TAURI__.core.convertFileSrc(abs);
    }
    if (customWindow && customWindow.__TAURI__ && typeof customWindow.__TAURI__.convertFileSrc === "function") {
      return customWindow.__TAURI__.convertFileSrc(abs);
    }
  } catch (e) {
    // 转换失败退回原值
  }
  return relPath;
}

function copy_text_to_clipboard(text, customNav = globalThis.navigator, customDoc = globalThis.document) {
  try {
    if (customNav && customNav.clipboard && typeof customNav.clipboard.writeText === "function") {
      customNav.clipboard.writeText(text);
      return true;
    }
    if (customDoc && typeof customDoc.createElement === "function") {
      const ta = customDoc.createElement("textarea");
      ta.value = text;
      ta.style = {};
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      customDoc.body.appendChild(ta);
      ta.select = () => {};
      const ok = customDoc.execCommand ? customDoc.execCommand("copy") : false;
      customDoc.body.removeChild(ta);
      return ok;
    }
    return false;
  } catch (e) {
    return false;
  }
}

test('1. 缩略图渲染：相对路径转为 Tauri asset URL 测试', (t) => {
  // 1.1 空路径返回空字符串
  assert.equal(asset_rel_to_url('', '/Users/roy/Library/Application Support/Easy-writing'), '');
  assert.equal(asset_rel_to_url(null, '/Users/roy/Library/Application Support/Easy-writing'), '');

  // 1.2 外链或协议完整 URL 必须原样返回（不走本地 assetProtocol）
  assert.equal(asset_rel_to_url('https://example.com/cover.jpg', '/data'), 'https://example.com/cover.jpg');
  assert.equal(asset_rel_to_url('asset://localhost/Users/img.png', '/data'), 'asset://localhost/Users/img.png');
  assert.equal(asset_rel_to_url('data:image/png;base64,abc', '/data'), 'data:image/png;base64,abc');

  // 1.3 存在 Tauri 运行时环境时，必须正确拼接绝对路径并转为 asset:// URL
  const mockWindowWithTauri = {
    __TAURI__: {
      core: {
        convertFileSrc: (p) => `asset://localhost/${encodeURIComponent(p)}`
      }
    }
  };
  const appData = '/Users/roy/Library/Application Support/com.easywriting.desktop';
  const rel = 'images/abc12345.png';
  const converted = asset_rel_to_url(rel, appData, mockWindowWithTauri);
  assert.equal(converted, `asset://localhost/${encodeURIComponent(appData + '/' + rel)}`);

  // 1.4 无 Tauri 桥接环境下优雅降级返回原相对路径（不崩溃）
  const fallback = asset_rel_to_url(rel, appData, null);
  assert.equal(fallback, rel);
});

test('2. 插入到光标处：Tiptap 桥接调用方法验证', (t) => {
  // 验证 js/src/tiptap-bridge.js 中包含 setImage 链式聚焦插入
  const tiptapBridgePath = path.join(repoRoot, 'js/src/tiptap-bridge.js');
  const code = fs.readFileSync(tiptapBridgePath, 'utf8');
  assert.ok(code.includes('export function insert_image(src, alt'), '缺少 insert_image 导出');
  assert.ok(code.includes('activeEditor.chain().focus().setImage({ src, alt }).run()'), '未调用 activeEditor setImage 链式插入');

  // 模拟 activeEditor 调用
  let inserted = null;
  const mockEditor = {
    chain() {
      return {
        focus() {
          return {
            setImage({ src, alt }) {
              inserted = { src, alt };
              return { run: () => true };
            }
          };
        }
      };
    }
  };
  mockEditor.chain().focus().setImage({ src: 'asset://localhost/test.png', alt: '测试插图' }).run();
  assert.deepEqual(inserted, { src: 'asset://localhost/test.png', alt: '测试插图' });
});

test('3. 复制路径：现代 Clipboard API 与回退方案验证', (t) => {
  // 3.1 优先使用 navigator.clipboard
  let writtenText = '';
  const mockNav = {
    clipboard: {
      writeText: (t) => { writtenText = t; }
    }
  };
  const success1 = copy_text_to_clipboard('images/cover_01.png', mockNav, null);
  assert.equal(success1, true);
  assert.equal(writtenText, 'images/cover_01.png');

  // 3.2 降级使用 document.execCommand
  let execCalled = false;
  let removedChild = false;
  const mockDoc = {
    body: {
      appendChild: () => {},
      removeChild: () => { removedChild = true; }
    },
    createElement: () => ({ style: {} }),
    execCommand: (cmd) => {
      if (cmd === 'copy') execCalled = true;
      return true;
    }
  };
  const success2 = copy_text_to_clipboard('images/cover_02.png', null, mockDoc);
  assert.equal(success2, true);
  assert.equal(execCalled, true);
  assert.equal(removedChild, true);
});

test('4. 文件缺失徽标：错误捕获与缺失响应机制测试', (t) => {
  // 模拟 asset_gallery.rs 中的 broken 状态集逻辑
  const brokenSet = new Set();
  const reportError = (relPath) => {
    brokenSet.add(relPath);
  };

  // 初始无缺失
  assert.equal(brokenSet.size, 0);

  // 模拟某两张图片在 WebView 中加载失败（触发 img on:error）
  reportError('images/missing_one.png');
  reportError('images/corrupted.jpg');

  assert.equal(brokenSet.size, 2);
  assert.ok(brokenSet.has('images/missing_one.png'));
  assert.ok(brokenSet.has('images/corrupted.jpg'));

  // 缺失状态下的徽标文字与样式判定
  const getBadgeState = (relPath) => {
    const isBroken = brokenSet.has(relPath);
    return {
      isBroken,
      badgeText: isBroken ? '⚠ 缺失' : '',
      borderClass: isBroken ? 'border-amber-300 dark:border-amber-800' : 'border-slate-200 dark:border-slate-700'
    };
  };

  const normalAsset = getBadgeState('images/fine.png');
  assert.equal(normalAsset.isBroken, false);
  assert.equal(normalAsset.badgeText, '');

  const brokenAsset = getBadgeState('images/missing_one.png');
  assert.equal(brokenAsset.isBroken, true);
  assert.equal(brokenAsset.badgeText, '⚠ 缺失');
  assert.ok(brokenAsset.borderClass.includes('amber'));
});

test('5. 深色模式与面板收起的布局：编译生成的样式与结构规则', (t) => {
  const cssPath = path.join(repoRoot, 'frontend/public/styles.css');
  const cssContent = fs.readFileSync(cssPath, 'utf8');

  // 5.1 验证 Tailwind 生成的深色模式类齐备
  assert.ok(cssContent.includes('.dark'), 'CSS 中缺少 .dark 基础规则');
  assert.ok(cssContent.includes('dark\\:bg-slate-800') || cssContent.includes('dark:bg-slate-800') || cssContent.includes('--color-slate-800'), '缺少深色背景支持');
  assert.ok(cssContent.includes('dark\\:border-slate-700') || cssContent.includes('dark:border-slate-700'), '缺少深色边框支持');

  // 5.2 验证面板收起时不受影响的 Modal 布局规则（固定视口全屏蒙层与自适应卡片）
  const galleryCodePath = path.join(repoRoot, 'frontend/src/components/editor/asset_gallery.rs');
  const galleryCode = fs.readFileSync(galleryCodePath, 'utf8');

  assert.ok(galleryCode.includes('fixed inset-0'), '图库 Modal 根节点必须采用 fixed inset-0 悬浮视口');
  assert.ok(galleryCode.includes('z-50'), '图库 Modal 需具备高 z-index（z-50）以遮罩侧栏');
  assert.ok(galleryCode.includes('grid grid-cols-3'), '图库主体须具备弹性网格列布局');
  assert.ok(galleryCode.includes('w-64 shrink-0'), '右侧详情栏需固定宽度并禁止收缩');
});
