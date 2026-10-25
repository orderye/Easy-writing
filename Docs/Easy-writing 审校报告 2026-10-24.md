# Easy-writing 审校报告（2026-10-24）

## TL;DR

本轮审校对象是**工作区中未提交的 PR-12（任务书 T10 伏笔看板）改动**（约 1200 行，含迁移 V008），并核对最近五个已提交的 PR-8～PR-11。

**结论：改动不能按当前状态提交。** 前端 leptos 组件存在 2 个编译错误（`cargo test -p easy-writing-frontend` 无法通过），core 侧另有 7 个库内单测被新列破坏。设计本身是好的——数据模型、迁移阶梯、跨卷章距口径、跨作品拒绝、留档链路都正确且有测试钉住；问题集中在**前端组件从未被编译验证过**和**一处测试 fixture 漏改**。修完 2 个 P0 与 1 个 P1 后即可提交。

---

## 一、审校范围

| 对象 | 内容 |
| --- | --- |
| 未提交改动（重点） | 伏笔看板 T10 / PR-12：`db/foreshadow.rs`（新）、`V008__plot_resolved_chapter.sql`（新）、`settings_panel/foreshadow_board.rs`（新）、`tests/foreshadow_board.rs`（新）、`tests/frontend/foreshadow_board.test.js`（新），以及 commands 注册、settings、shared-types、migration_ladder 等 13 个修改 |
| 背景核对 | PR-08～PR-11 五个已提交功能（上下文装配透明度、全书搜索替换、提炼去重、冲突检测结构化），确认与工作区改动无冲突 |
| 文档同步 | 《实施计划》§五、`current-state-map.md`、CHANGELOG、护栏脚本 |

## 二、测试结果

| 套件 | 结果 |
| --- | --- |
| `pnpm test:frontend` | ✅ 49/49 通过（含新增 foreshadow_board.test.js 6 条契约） |
| `pnpm test:scripts` | ✅ 31/31 通过 |
| 文档 Schema 护栏 + 版本一致性 | ✅ 通过（8 个迁移文件 ↔ 20 张表双向一致，6 处版本号一致） |
| `cargo test -p easy-writing-frontend` | ❌ **编译失败**，2 个错误（见 P0） |
| `cargo test -p easy-writing-core --lib` | ❌ 101 通过 / **7 失败**（见 P1） |
| 集成测试（关键四件套） | ✅ migration_ladder 15/15、foreshadow_board 12/12、ipc_argcase 6/6、command_key_sweep 1/1 通过 |
| `cargo fmt --check` | ⚠️ 全仓 548 处不符（历史遗留，非本轮引入；但本轮 3 个新文件也在其中）。CI 未设 fmt/clippy 门禁，不阻塞 |

**重要说明**：`pnpm test:frontend` 与前端契约测试全部通过，但它们是**对源码文本做断言的契约测试**，不编译 Rust——因此前端组件的编译错误完全逃过了这些测试和本轮之外的 CI（CI 后端 job 跑的 `cargo test` 会编译 frontend crate，一旦推送即会红）。

## 三、发现的问题

### P0（阻塞提交）

**P0-1 前端组件编译失败：`load_board` 在声明前被引用**
`frontend/src/components/settings_panel/foreshadow_board.rs:24` —— `Effect::new` 的闭包里调用了 `load_board()`，但 `let load_board = ...` 在其之后才声明。Rust 局部变量必须先声明后使用，`cargo build -p easy-writing-frontend` 报 E0425。
修法：把 `let load_board = ...` 移到 `Effect::new` 之前。

**P0-2 前端组件编译失败：跳转埋设章缺少 `.clone()`**
同文件 `:209` —— `request_switch_chapter(app_ctx, h)` 传入的是 `&ChapterHeader`，函数签名要求 `ChapterHeader`（E0308）。
修法：按编译器建议改为 `h.clone()`。

### P1（提交前应修）

**P1-1 库内测试 fixture 漏加新列，7 个单测失败**
`src-tauri/src/db/lexicon.rs:326` 的 `setup_test_db` 手工建 `plot_item` 表，没有 `resolved_chapter_id` 列；而本轮把 `list_plots` 的 SELECT 加上了该列，凡经此 fixture 调用 `list_plots` 的单测全部报 `no such column: resolved_chapter_id`（7 例）。
修法：给 `setup_test_db` 的 `plot_item` 建表语句补一列 `resolved_chapter_id TEXT`（若 `ai/context.rs` 的测试 fixture 同样手工建了该表，一并检查）。
这暴露一个模式问题：**每次给核心表加列，所有手工建表的单测 fixture 都要同步**——建议后续把这类 fixture 收敛到共享的建表函数，避免每次加列都要人肉扫一遍。

### P2（不阻塞，建议顺手修）

**P2-1 CHANGELOG 缺 PR-12 条目**
`CHANGELOG.md` 的 `[Unreleased]` 下有 PR-8～PR-11 的详细条目，但没有本轮伏笔看板的段落。`git status` 确认 CHANGELOG 未被修改。提交前应补上（含迁移 V008 说明与「可测」清单）。

**P2-2 文档与代码不符：声称的共享信号不存在**
`Docs/current-state-map.md` 新增段落写「看板加载时写入共享信号 `AppContext.foreshadow_board`，状态栏只读过滤」——但 `frontend/src/state/mod.rs` 里没有这个字段，组件用的是**本地** `RwSignal`，状态栏也没有消费它。二选一：要么把文档改为「当前计数仅在看板内展示，状态栏计数未实现」，要么补上共享信号实现。目前「打开伏笔 tab 之前计数为 0」这句话在代码里根本没有对应的机制。

**P2-3 组件内死代码与重复闭包**
`foreshadow_board.rs` 中：`on_resolve` 闭包（`:38-47`）从未被使用（实际回收走的是行内 `select` 的 `on:change`）；`load_board` 与 `refresh_board` 是两个完全相同的闭包。修 P0 时顺手删掉即可。

### P3（观察项）

- **`command_key_sweep` 单测耗时 356 秒**。它对全部 117 个命令逐一发起真实调用，在外置机械盘上明显偏慢。不阻塞，但若 CI 时长敏感，可考虑给它单独的缓存或拆分。
- **`cargo fmt` 全仓不符 548 处**（历史遗留）。本轮新文件也在其中，建议单独跑一次 `cargo fmt` 收敛，不与本功能 PR 混合提交。
- 编译期间测试环境（外置硬盘 + 多 cargo 进程并行）导致本轮测试总耗时异常长（约 3 小时），属环境因素，非代码问题。

## 四、设计与实现的正面确认

以下点审校后确认正确，值得保留：

1. **迁移 V008 完全符合项目阶梯规范**：仅加列 + 索引、`ON DELETE SET NULL`、`migration_ladder` 扩到 V7→V8 全部 15 例通过、`BASELINE_VERSION` 7→8 同步、未来版本不被降级的测试也更新了。
2. **「距今章数」口径正确且有测试钉住**：用 `list_all_chapter_headers_for_novel` 的全书序下标，跨卷天然正确；埋设章缺失返回 `None` 而不是编数；已回收永不超期。
3. **`resolve_plot` 的安全边界完整**：跨作品章节被拒（有测试）、`record_setting_change` 留档（有测试）、`save_plot` 的 upsert 刻意不覆盖 `resolved_chapter_id`（有专门测试钉住这个决定，且注释解释了原因）。
4. **文档护栏全绿**：《实施计划》§五 的 DDL 与 8 个迁移文件双向一致（`check-docs-schema.mjs` 通过），版本号 6 处一致。
5. **背景提交核对无冲突**：PR-11 的 V007 与本轮 V008 在 `migration_ladder.rs` 的两批修改已正确合并，命令计数 115→117 在 `ipc_argcase.rs` 中有注释留痕。

## 五、建议的提交顺序

1. 修 P0-1、P0-2（前端组件两处编译错误）→ 跑 `cargo build -p easy-writing-frontend` 确认绿。
2. 修 P1-1（`lexicon.rs` fixture 补列）→ 跑 `cargo test -p easy-writing-core --lib` 确认 108/108。
3. 补 P2-1（CHANGELOG 条目）、P2-2（文档改口径或补实现）、P2-3（删死代码）。
4. 全套 `pnpm test && cargo test` 过后，按《提交流程》提交为 PR-12。

---

*powered by KimiCode*

<!-- kimi-badge -->
[![powered by KimiCode](../.github/assets/kimi-badge.svg)](https://www.kimi.com/code)
