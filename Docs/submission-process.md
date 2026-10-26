# 提交流程（双远程）

本仓库同时推送两个远程：

| 远程 | 地址 | 用途 |
|---|---|---|
| `origin` | `git@github.com:orderye/Easy-writing.git` | GitHub 主仓 |
| `cnb` | `https://cnb.cool/toroyxxx/Easy-writing.git` | CNB 镜像 |

> 📌 **注**：日常代码提交流程见下文；若涉及版本号晋级与正式发布双平台 Release（挂载安装包），请参阅 [《双远程 Release 发布流程参考规范》](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/Docs/dual-remote-release-flow.md)。

## 关键约定：CNB 用隔离 worktree 提交

**GitHub 在主目录提交，CNB 在 `.cnb-worktree` 提交。**
原因是两者的提交身份不同：

| 位置 | 分支 | 作者邮箱 |
|---|---|---|
| 主目录 | `main` | `user@example.com`（来自 `~/.gitconfig`） |
| `.cnb-worktree` | `cnb-main` | `3WJRP74JzJLZlOYQ8b7VcB+cnb.dgyOUWMigHA@noreply.cnb.cool` |

### worktree 的身份是自足的

worktree 的 `name` 与 `email` **都显式写在它自己的配置里**，不依赖全局继承。
配置位于 `.git/worktrees/-cnb-worktree/config.worktree`：

```ini
[user]
	email = 3WJRP74JzJLZlOYQ8b7VcB+cnb.dgyOUWMigHA@noreply.cnb.cool
	name = developer
[branch "cnb-main"]
	remote = cnb
	merge = refs/heads/main
[push]
	default = upstream
```

早期只写了 `email`、`name` 靠继承全局，存在隐患：任何人往全局或本文件
再加一行 `user.*` 都可能改变结果。现已补上 `name`，身份完全自足。

**验证自足性**（屏蔽全局配置后仍应正确）：

```bash
cd .cnb-worktree
HOME=/tmp/nonexistent-home git var GIT_AUTHOR_IDENT
# 期望：developer <3WJRP...@noreply.cnb.cool>
# 对照主目录同样屏蔽后会退化为系统默认（Roy），说明它确实依赖全局
```

> ⚠️ 改这个配置必须用 `git config --worktree`，**不能**用 `git -C .cnb-worktree config`——
> 后者默认写进共享的 `.git/config`，会同时影响主目录。

`push.default = upstream` 意味着在 worktree 里**直接 `git push` 就会推到 `cnb/main`**，
不需要显式指定远程。

## 标准流程（CNB 带专属身份）

**目标**：GitHub 与 CNB 各自保留独立提交，CNB 侧作者为 CNB 邮箱。
代价是**两端 hash 会分叉**（内容相同、提交不同）。

```bash
# ── 1. GitHub：主目录提交 ──
cd "/Volumes/WD Drive/项目/Easy-writing · 小说工作台"
git add <paths>                 # 按路径显式暂存，避免误提交临时文件
git commit -F - <<'MSG'
提交信息
MSG
git push origin main

# ── 2. CNB：在 worktree 里重新提交同一改动 ──
cd ".cnb-worktree"
git cherry-pick --no-commit <github的commit>   # 取改动，不取作者
git commit -F - <<'MSG'
提交信息                           # 可原样复用；作者此时已是 CNB 身份
MSG
git push                        # push.default=upstream → cnb/main
```

**关键点**：用 `cherry-pick --no-commit` 而非 `merge --ff-only`。
前者只取改动内容，重新提交时作者由 worktree 的身份配置决定；
后者会让 `cnb-main` 直接指向 GitHub 那个提交，作者也随之沿用 GitHub 身份——
那就失去了写 CNB 身份的意义。

### 为什么不能用 merge --ff-only

`merge --ff-only` 把 `cnb-main` 快进到 `main` 的同一个 commit，
两边 hash 相同，作者自然相同（都是 `user@example.com`）。
只有**重新提交**才会产生带 CNB 邮箱的新 commit。

## 两个工作区的关系

`.cnb-worktree` 与主目录是**同一仓库的两个工作区、不同分支**：

- 主目录：`main`（跟踪 `origin/main`）— 作者 `user@example.com`
- worktree：`cnb-main`（跟踪 `cnb/main`）— 作者 CNB 邮箱

**它们的内容靠 cherry-pick 手动同步，不会自动一致。**
因此每次都要「主目录提交 → worktree 重放」两步，两端 hash 必然不同。

这是刻意的取舍：换来 CNB 侧正确的提交身份。

## 校验当前状态

```bash
# 两端 tip 与作者（hash 应不同、作者应不同、内容应一致）
printf '%-16s %s\n' 'origin/main:' "$(git ls-remote origin main | cut -c1-7)"
printf '%-16s %s\n' 'cnb/main:'    "$(git ls-remote cnb main | cut -c1-7)"
git log -1 --format='%an <%ae>' origin/main
git log -1 --format='%an <%ae>' cnb/main

# worktree 生效身份（应为 CNB 邮箱）
git -C .cnb-worktree var GIT_AUTHOR_IDENT

# 两端内容一致性抽查（行数/文件应相同）
git show origin/main:Docs/submission-process.md | wc -l
git show cnb/main:Docs/submission-process.md | wc -l
```

预期结果形如：

```
origin/main:     8f13d34        ← 作者 user@example.com
cnb/main:        80d78b0        ← 作者 ...@noreply.cnb.cool
```

## 历史遗留（已知，不再处理）

CNB 上 **6 个早期提交的作者仍是 `user@example.com`**——那是本项目
在确立 worktree 约定之前，从主目录直接 `git push cnb main` 推上去的。

| 提交 | 说明 |
|---|---|
| `3949da1` | fix: 按差异核对报告修复 P0/P1/P2 缺陷 |
| `e0c3930` | refactor: 去繁化简，删除 79 个 IPC 参数结构体 |
| `8a795ea` | refactor(config): 合并 setting helper + 量测报告 |
| `502f3e6` | test(ipc): 命令清单自动提取比对 |
| `c14d548` | feat(novel): 作品删除与跨作品回收站 |
| `2031dca` | feat: 拖拽排序与编辑器插图 |
| `8f13d34` | docs: 双远程提交流程（首次尝试 worktree，但用了 ff-only，作者仍是 GitHub 身份邮箱） |

**决定：保留现状，不追溯改写。** 理由：改写需要 filter-repo + force push
CNB 全部历史，会改变所有既有 hash；而这些提交内容本身正确，
仅作者标识不符。从 `80d78b0` 起，CNB 侧提交均由 worktree 的 CNB 身份产生。

## 注意

1. **不要在主目录直接 `git push cnb main`**——那样提交的作者是 GitHub 身份，
   绕过了 worktree 的 CNB 身份约定（即上述历史遗留的成因）。
2. **不要用 `merge --ff-only main` 同步**——会让 CNB 作者变回 GitHub 身份。
   用 `cherry-pick --no-commit` + 重新 `commit`。
3. worktree 里 `git add -A` 前先 `git status` 复核，避免把构建产物纳入。
4. 临时提交信息文件不要落在仓库内（用 `git commit -F -` 从 stdin 读）。
5. 若 `git push` 被拒（历史上曾用 ff-only 同步过，导致远程已有同内容的
   GitHub 身份提交），需 `git push --force-with-lease` 覆盖该笔。
   `--force-with-lease` 会在远程出现他人新提交时拒绝执行，比 `--force` 安全。

<!-- kimi-badge -->
[![powered by KimiCode](../.github/assets/kimi-badge.svg)](https://www.kimi.com/code)
