#!/usr/bin/env node
/**
 * 发布门禁决策器。
 *
 * ## 为什么需要它
 *
 * release.yml 同时支持 tag push 与 `workflow_dispatch`。原先版本校验步骤带着
 * `if: startsWith(github.ref, 'refs/tags/')`，于是**手动触发时整段校验被跳过**，
 * 紧接着 tauri-action 用 `tagName: ${{ github.ref_name }}` 创建 Release ——
 * 从 main 手动触发一次，就会产出一个名为「Easy-writing main」、tag 为 `main`
 * 的正式发布产物，版本号与内容完全脱钩。
 *
 * 只改 workflow 里的 `if:` 表达式并不能真正解决问题：分支名会不会被当成 tag、
 * 指定的 tag 是否真实存在、build-only 模式该不该发布，这些都是需要判定与测试的逻辑。
 * 把它们放进 workflow 表达式里就等于放弃测试，所以提取成本脚本。
 *
 * ## 契约
 *
 * 输入（环境变量，由 workflow 提供）：
 *   GITHUB_EVENT_NAME   push | workflow_dispatch
 *   GITHUB_REF          refs/tags/v0.3.8 | refs/heads/main
 *   GITHUB_REF_NAME     v0.3.8 | main
 *   INPUT_MODE          build-only | release-tag   （仅 workflow_dispatch）
 *   INPUT_TAG           目标 tag                    （仅 workflow_dispatch + release-tag）
 *
 * 输出（写入 GITHUB_OUTPUT，供后续 job 使用）：
 *   release_tag    正式发布使用的 tag；build-only 时为空
 *   is_release     true | false
 *   checkout_ref   后续 job 应当检出的 ref
 *
 * Exit 0 = 门禁通过；Exit 1 = 拒绝发布（workflow 会就此停止）。
 */

import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';
import { pathToFileURL } from 'node:url';

/** 正式发布的 tag 必须是严格的 `vMAJOR.MINOR.PATCH`。
 *  这条正则是「main 不能变成 Release tag」的第一道闸。 */
const RELEASE_TAG_PATTERN = /^v\d+\.\d+\.\d+$/;

/**
 * 纯决策函数。`tagExists` 以参数注入，便于测试；
 * 生产环境由 `gitTagExists` 提供。
 *
 * @returns {{ok: boolean, releaseTag: string, isRelease: boolean,
 *            checkoutRef: string, errors: string[]}}
 */
export function resolveReleaseGate(input, tagExists = () => true) {
  const errors = [];
  const fail = (msg) => errors.push(msg);

  const event = String(input.eventName ?? '').trim();
  const ref = String(input.ref ?? '').trim();
  const refName = String(input.refName ?? '').trim();
  const mode = String(input.mode ?? '').trim();
  const inputTag = String(input.tag ?? '').trim();

  const result = {
    ok: false,
    releaseTag: '',
    isRelease: false,
    checkoutRef: refName || 'HEAD',
    errors,
  };

  if (event === 'push') {
    // workflow 的 on.push 只订阅 refs/tags/v*，因此这里出现的应当总是 tag。
    // 但仍然显式校验：万一触发条件被放宽，绝不能让分支名溜进 Release。
    if (!ref.startsWith('refs/tags/')) {
      fail(
        `push 事件的 ref 不是 tag（${ref || '（空）'}）。正式发布只能由 tag 触发，` +
          `分支推送不得创建 Release。`
      );
      return result;
    }
    if (!RELEASE_TAG_PATTERN.test(refName)) {
      fail(
        `tag "${refName}" 不符合 vMAJOR.MINOR.PATCH 格式，拒绝作为正式发布 tag。`
      );
      return result;
    }
    result.releaseTag = refName;
    result.isRelease = true;
    result.checkoutRef = refName;
    result.ok = true;
    return result;
  }

  if (event === 'workflow_dispatch') {
    if (mode === 'build-only') {
      // 只构建、不发布：不要求 tag，也绝不产出 Release。
      // 仍然要检出触发时的分支，供人工验证构建产物。
      result.isRelease = false;
      result.releaseTag = '';
      result.checkoutRef = ref || refName || 'HEAD';
      result.ok = true;
      return result;
    }

    if (mode !== 'release-tag') {
      fail(
        `workflow_dispatch 必须显式选择 mode（build-only 或 release-tag），当前为 "${mode || '（未提供）'}"。` +
          `不允许在没有明确意图的情况下发布。`
      );
      return result;
    }

    // mode === 'release-tag'
    if (!inputTag) {
      fail('release-tag 模式必须提供目标 tag。');
      return result;
    }
    if (!RELEASE_TAG_PATTERN.test(inputTag)) {
      fail(
        `目标 tag "${inputTag}" 不符合 vMAJOR.MINOR.PATCH 格式。` +
          `分支名（如 main）绝不能被用作 Release tag。`
      );
      return result;
    }
    if (!tagExists(inputTag)) {
      fail(
        `仓库中不存在 tag "${inputTag}"。发布必须指向一个真实存在的 tag，` +
          `否则构建内容与该 tag 无从对应。请先创建 tag，或改用 build-only。`
      );
      return result;
    }

    result.releaseTag = inputTag;
    result.isRelease = true;
    // 关键：检出 tag 本身而不是触发时的分支 HEAD，
    // 保证构建内容与目标 tag 严格对应。
    result.checkoutRef = inputTag;
    result.ok = true;
    return result;
  }

  fail(`不支持的事件类型 "${event || '（空）'}"，无法判定发布意图。`);
  return result;
}

/** 生产环境的 tag 存在性检查。 */
export function gitTagExists(tag, cwd = process.cwd()) {
  try {
    // `^{}` 解引用：附注 tag（annotated tag）也要能被认出来。
    execFileSync('git', ['rev-parse', '-q', '--verify', `refs/tags/${tag}^{}`], {
      cwd,
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    return true;
  } catch {
    return false;
  }
}

function main() {
  const result = resolveReleaseGate(
    {
      eventName: process.env.GITHUB_EVENT_NAME,
      ref: process.env.GITHUB_REF,
      refName: process.env.GITHUB_REF_NAME,
      mode: process.env.INPUT_MODE,
      tag: process.env.INPUT_TAG,
    },
    gitTagExists
  );

  console.log(
    `事件=${process.env.GITHUB_EVENT_NAME ?? '(空)'} ref=${process.env.GITHUB_REF ?? '(空)'} ` +
      `mode=${process.env.INPUT_MODE ?? '(空)'} tag=${process.env.INPUT_TAG ?? '(空)'}`
  );
  console.log(
    `判定：is_release=${result.isRelease} release_tag=${result.releaseTag || '(无)'} ` +
      `checkout_ref=${result.checkoutRef}`
  );

  if (!result.ok) {
    for (const e of result.errors) console.error(`❌ 发布门禁未通过：${e}`);
    process.exit(1);
  }

  const outFile = process.env.GITHUB_OUTPUT;
  if (outFile) {
    appendFileSync(
      resolvePath(outFile),
      `release_tag=${result.releaseTag}\n` +
        `is_release=${result.isRelease}\n` +
        `checkout_ref=${result.checkoutRef}\n`
    );
  }
  console.log('✅ 发布门禁通过。');
}

// 仅在被直接执行时运行 main；被 import（测试）时不产生副作用。
// 用 pathToFileURL 而不是手拼 'file://' —— 本仓库路径含空格与 `·`，
// 手拼出来的 URL 无法与 import.meta.url 相等，会让脚本在 CI 里静默不执行。
const invokedDirectly =
  !!process.argv[1] && import.meta.url === pathToFileURL(resolvePath(process.argv[1])).href;
if (invokedDirectly) {
  main();
}
