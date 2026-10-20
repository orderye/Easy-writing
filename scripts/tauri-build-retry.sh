#!/usr/bin/env bash
# tauri build 的 macOS DMG 打包重试。
#
# ## 为什么需要
#
# V0.3.8 补充修复期间实测到：`pnpm tauri build --target aarch64-apple-darwin`
# 第一次运行时 `bundle_dmg.sh` 失败（exit 1），代码没有任何改动、清理掉
# 上一次失败遗留的挂载卷与 `rw.*.Easy-writing_*.dmg` 之后重跑就成功了。
#
# 失败还留了两样东西在构建目录里：一个仍挂载着的 /Volumes/dmg.XXXXXX 卷，
# 和一个 46 MB 的中间态 `rw.*.dmg`。它们不会自动消失，会让下一次尝试继续踩坑。
#
# 所以这里做两件事：跑之前先清残留，跑完之后如果 `.app` 在而 `.dmg` 不在，
# 就再清一次残留并重试。真正的失败仍然会以非零退出码冒出来，不会被掩盖。
#
# ## 边界
#
# 只清理 `target/*/release/bundle/macos/` 下符合 `rw.*.Easy-writing_*.dmg`
# 命名的文件，以及由这些文件挂载起来的卷。**不碰用户数据、不碰其它挂载卷。**
set -euo pipefail

ATTEMPTS=2
TARGET="${1:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_PARENT="$ROOT/target"

log() { printf '[tauri-build-retry] %s\n' "$*"; }

# 卸载由本仓库构建目录里的 rw.*.dmg 挂载出来的卷。
#
# 用 image-path 精确匹配，而不是遍历所有 /dev/diskN：
# 误弹用户的移动盘或网络盘是不可接受的后果。
detach_stale_volumes() {
  command -v hdiutil >/dev/null 2>&1 || return 0
  local info img
  info="$(hdiutil info 2>/dev/null || true)"
  [ -z "$info" ] && return 0
  # hdiutil info 以空行分隔每个镜像块；挑出 image-path 指向我方构建目录的那些
  while IFS= read -r line; do
    case "$line" in
      image-path*)
        img="${line#*image-path}"
        img="${img#*: }"
        img="$(printf '%s' "$img" | sed 's/^[[:space:]]*//')"
        case "$img" in
          "$BUNDLE_PARENT"*/rw.*.dmg|"$BUNDLE_PARENT"*/bundle/macos/rw.*)
            log "卸载残留构建卷：$img"
            hdiutil detach "$img" -force >/dev/null 2>&1 || true
            ;;
        esac
        ;;
    esac
  done <<< "$info"
}

clean_leftovers() {
  detach_stale_volumes
  find "$BUNDLE_PARENT" -type f -name 'rw.*.Easy-writing_*.dmg' -delete 2>/dev/null || true
}

# macOS 上期望的 dmg 产物是否存在。
#
# 判据必须带时间戳：`.app` / `.dmg` 可能是上一次成功构建留下的旧文件。
# 不加这个限制时，一次纯粹的编译错误也会因为「旧 .app 还在、旧 .dmg 被删过」
# 被误判成 DMG 瞬时故障而无谓重试。
ATTEMPT_START=0

newest_artifact_mtime() {
  find "$BUNDLE_PARENT" \( -type d -name 'Easy-writing.app' -o -type f -name '*.dmg' \) \
    -not -path '*/rw.*' 2>/dev/null \
    | while IFS= read -r p; do stat -f %m "$p" 2>/dev/null || stat -c %Y "$p" 2>/dev/null; done \
    | sort -rn | head -1
}

app_built_this_attempt() {
  local m
  m="$(newest_artifact_mtime)"
  [ -n "$m" ] && [ "$m" -ge "$ATTEMPT_START" ]
}

dmg_built_this_attempt() {
  local m
  m="$(find "$BUNDLE_PARENT" -type f -name '*.dmg' -path '*/bundle/dmg/*' 2>/dev/null \
       | while IFS= read -r p; do stat -f %m "$p" 2>/dev/null || stat -c %Y "$p" 2>/dev/null; done \
       | sort -rn | head -1)"
  [ -n "$m" ] && [ "$m" -ge "$ATTEMPT_START" ]
}

cd "$ROOT"

attempt=1
while [ "$attempt" -le "$ATTEMPTS" ]; do
  log "第 $attempt/$ATTEMPTS 次构建尝试"
  clean_leftovers
  ATTEMPT_START="$(date +%s)"

  if pnpm tauri build "$@"; then
    log "构建成功"
    exit 0
  fi

  # 非 macOS 平台（或没有生成 .app）不存在「只有 DMG 环节失败」这种情形；
  # .app 有了但 .dmg 没更新，才判定为 DMG 打包的瞬时故障。
  if [ "$(uname)" != "Darwin" ] || ! app_built_this_attempt || dmg_built_this_attempt; then
    log "构建失败，且不像 DMG 打包环节的瞬时故障 —— 不重试，按失败处理"
    exit 1
  fi

  log ".app 已产出但 .dmg 缺失，判定为 DMG 打包瞬时故障，清理残留后重试"
  attempt=$((attempt + 1))
done

log "已重试 $ATTEMPTS 次仍然失败，判定为真实故障"
exit 1
