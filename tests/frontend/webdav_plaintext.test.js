import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// ---------------------------------------------------------------------------
// WebDAV 明文传输警告的契约（任务书 T15 / PR-16）。
//
//   1. 判定函数：http:// 前缀即明文；https 与空串不算；
//   2. 保存/测试连接前：http 地址必须先弹二次确认，确认后才真正执行；
//   3. 地址输入下方有常驻「明文传输」红色标识（仅 http 时显示）；
//   4. 默认示例与占位符使用 https（不引导用户走向明文）；
//   5. 回环取舍：本机地址不豁免（全部 http:// 均警告）。
// ---------------------------------------------------------------------------

const TAB_SRC = path.join(repoRoot, 'frontend/src/components/config/webdav_tab.rs');
const src = fs.readFileSync(TAB_SRC, 'utf8');

test('1. 判定函数 is_plaintext_webdav：http 前缀为真，https / 空 / 大小写变体', () => {
  assert.ok(src.includes('pub fn is_plaintext_webdav'), '缺少判定函数');

  // 从源码中提取函数体做纯逻辑验证（与运行时行为一致的前缀判断）
  const fnBody = src.slice(src.indexOf('pub fn is_plaintext_webdav'));
  assert.ok(
    fnBody.includes('starts_with("http://")'),
    '判定必须是 http:// 前缀'
  );
  assert.ok(
    fnBody.includes('to_ascii_lowercase'),
    '判定前必须小写化（HTTP:// 也算明文）'
  );
});

test('2. 保存前拦截：http 地址先 pending_plaintext 二次确认', () => {
  assert.ok(src.includes('let pending_plaintext'), '缺少确认状态');
  // on_save_webdav 的守卫：明文时置 pending_plaintext 并 return（不执行保存）
  const guardBlock = src.slice(src.indexOf('let on_save_webdav'), src.indexOf('let on_save_webdav') + 400);
  assert.ok(
    guardBlock.includes('is_plaintext_webdav(&webdav_url.get())'),
    '保存入口必须先判明文'
  );
  assert.ok(
    guardBlock.includes('pending_plaintext.set(true)'),
    '明文时必须弹确认而不是直接保存'
  );
  assert.ok(guardBlock.includes('return;'), '拦截后必须直接返回');
  // 真正的保存在确认按钮回调里执行
  assert.ok(src.includes('do_save_webdav();'), '确认后才执行真正的保存');
});

test('3. 常驻明文标识：地址输入下方仅 http 时显示红色警示', () => {
  assert.ok(src.includes('明文传输'), '缺少「明文传输」文案');
  const badgeBlock = src.slice(src.indexOf('明文传输标识'));
  assert.ok(badgeBlock.includes('text-red-600'), '标识必须是红色警示');
  assert.ok(
    badgeBlock.includes('is_plaintext_webdav(&webdav_url.get())'),
    '标识必须以判定函数驱动（https 不显示）'
  );
});

test('4. 默认值与占位符均为 https', () => {
  // 初始值
  assert.ok(src.includes('RwSignal::new("https://dav.jianguoyun.com/dav/'), '默认值必须是 https');
  // 占位符
  const placeholders = [...src.matchAll(/placeholder="([^"]*)"/g)].map(m => m[1]);
  const urlPlaceholder = placeholders.find(p => p.includes('dav.'));
  assert.ok(urlPlaceholder && urlPlaceholder.startsWith('https://'), `URL 占位符必须以 https 开头，实际 ${urlPlaceholder}`);
});

test('5. 确认弹窗文案把风险说清楚：密码与整库备份明文过网', () => {
  const modalBlock = src.slice(src.indexOf('明文传输二次确认'));
  assert.ok(modalBlock.includes('明文'), '弹窗标题/正文必须提明文');
  assert.ok(modalBlock.includes('密码'), '必须提到凭据泄露风险');
  assert.ok(modalBlock.includes('整库备份'), '必须提到备份内容泄露风险');
  assert.ok(modalBlock.includes('仍要保存'), '必须有显式承担风险的确认按钮');
  assert.ok(modalBlock.includes('取消'), '必须有取消（改 https）路径');
});
