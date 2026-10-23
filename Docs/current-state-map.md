# Easy-writing 现状映射表（任务书再基线）

> **文档性质**：本文件是《[Easy-writing 升级任务书（交付编程 Agent）](./Easy-writing%20升级任务书（交付编程%20Agent）.md)》的**源码级复核结果**。
> 任务书 §0 自述「审核仅基于 README 与实施计划，未读源码，因此本文中的文件路径、表名均为假设，**必须先在仓库中核对**」。
> 本文件即为那次核对的产出，并据此对 18 个任务重新基线。
>
> **使用规则**：按任务书全局规则 1 —— **映射表不确认，不进入改动**。后续每个 PR 引用本文件对应条目，不重复论证。
>
> **核对方式**：全部结论来自实际读取源码与配置文件，关键条目附 `文件:行号` 证据；凡无法确证的，一律标注「未能确证」而非推测。

**核对基线**：仓库 `orderye/Easy-writing` @ `819b860`（版本 0.3.8）· 核对日期 2026-10-22

---

## 一、仓库事实基线

| 维度 | 实际值 | 证据 |
|---|---|---|
| 版本 | 0.3.8（`package.json` / `src-tauri/Cargo.toml` / `frontend/Cargo.toml` / `shared-types/Cargo.toml` / `src-tauri/tauri.conf.json` 五处一致） | `scripts/check-version.mjs` 守护 |
| Tauri 命令数 | **109**（全部在 `src-tauri/src/lib.rs` 的 `generate_handler!` 中注册，无未注册/孤儿命令） | `src-tauri/src/commands/*.rs`、`src-tauri/tests/ipc_argcase.rs:91` |
| 数据库表 | 迁移文件内 **19** 张 + 运行时创建 `schema_version` = **20** | `src-tauri/migrations/*.sql`、`src-tauri/src/db/mod.rs:26` |
| 集成测试文件 | `src-tauri/tests/` 下 **23** 个 `.rs` | 另有 `tests/frontend` 2 个、`tests/scripts` 2 个 |
| 前端 IPC 调用点 | **132** 处 | `frontend/src/**/*.rs` 的 `invoke_cmd` / `invoke_no_args` |
| SQLite | rusqlite 0.32.1（`bundled`）→ libsqlite3-sys 0.30.1 → **SQLite 3.46.0**，`-DSQLITE_ENABLE_FTS5` 已启用，**含 trigram 分词器** | `src-tauri/Cargo.toml:19`、`libsqlite3-sys-0.30.1/build.rs:127-129`、`sqlite3.c:255381` |
| 数据库文件 | `<app_data_dir>/easy-writing.db`，WAL，`foreign_keys=ON`，连接池 `r2d2` max_size=8 | `src-tauri/src/db/mod.rs:117-130` |
| 「字数」口径 | 应用内「字数」= Tiptap `CharacterCount.characters()`，**是字符数（含标点），不是词数** —— T02「导出字数与应用统计一致」与 T11「净增字数」都以此为准 | `js/src/tiptap-bridge.js:485-488` |

### 1.1 迁移机制 —— **与任务书假设不同（重要）**

任务书全局规则 3 写「数据库变更只能新增迁移文件（**refinery**，下一个可用版本号）」。

**仓库中没有 refinery**（`Cargo.toml` / `Cargo.lock` / `src-tauri/Cargo.toml` 均无该依赖）。迁移是**手写阶梯**，位于 `src-tauri/src/db/mod.rs:117-178`：

```rust
pub const BASELINE_VERSION: i64 = 4;                       // db/mod.rs:39
if applied < 1 { conn.execute_batch(include_str!(".../V001__init.sql"))?; ... }
if applied < 2 { run_legacy_safe_migrations(&conn)?; ... }  // db/mod.rs:53
if applied < 3 { conn.execute_batch(include_str!(".../V003__indices_and_cleanup.sql"))?; ... }
if applied < 4 { conn.execute_batch(include_str!(".../V004__entry_pinned.sql"))?; ... }
```

**因此「新增一个迁移」实际是四步，不是一步**：

1. 新增 `src-tauri/migrations/V00N__<name>.sql`
2. 在 `init_db` 中加 `if applied < N { conn.execute_batch(include_str!(...))?; conn.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (N)", [])?; }`
3. `BASELINE_VERSION` 改为 `N`
4. 扩展 `src-tauri/tests/migration_ladder.rs`（该测试硬编码 v1–v4 断言，如 `v1_database_upgrades_continuously_to_v4_keeping_all_data`）

> **约束**：`V002` 走的是「按 `;` 朴素切分逐条执行」的 legacy-safe 路径（`db/mod.rs:53-93`），要求「一行一条语句、字符串字面量内不含 `;`」。**新建的 V00N 走 `execute_batch`，不受该约束** —— 但触发器（`BEGIN … END`）含分号，必须确认走的是 `execute_batch` 路径。
>
> `schema_version` 表本身**不在任何迁移文件里**，而是运行时创建（`db/mod.rs:26`）。「当前 Schema」文档必须把它列进去。

### 1.2 设置项存储 —— **键值表，无迁移成本**

```sql
-- V001__init.sql:229
CREATE TABLE IF NOT EXISTS app_setting (key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '');
```

- 访问层：`src-tauri/src/db/app_setting.rs:3-20`（`get_setting` / `set_setting`，`ON CONFLICT DO UPDATE`）
- 命令：`get_app_setting` / `set_app_setting`（`src-tauri/src/commands/mod.rs:539-547`）
- 现有键：`auto_save_interval`、`font_size`、`line_height`、`auto_extract`、`theme`、`left_panel_visible`、`right_panel_visible`（种子，`V001:235-242`）、`update_check_url`、`ignored_update_version`、`last_update_check_time`（`commands/update_cmd.rs:54,55,140`）
- **结论**：`daily_goal`、`extraction_mode`、`context_window`、冲突检测三项开关等**全部无需迁移**，直接写行即可。
- **注意**：`src-tauri/src/sync/restore.rs:52` 硬编码了 `PRESERVED_SETTING_KEYS = "'theme', 'left_panel_visible', 'right_panel_visible'"` —— 需要在远端恢复后保留下来的本机设置，必须加进这个列表。

其它 JSON-in-TEXT 列（与 `app_setting` 是两种不同模式）：`llm_profile.params`、`agent.params`、`character.aliases`（JSON 数组）、`character.profile`（JSON 对象）、`extraction_candidate.payload`。

### 1.3 新增 IPC 命令必须同步的三处契约

| 文件 | 约束 |
|---|---|
| `src-tauri/tests/ipc_argcase.rs` | 每个 `#[tauri::command]` **必须**声明 `rename_all = "snake_case"`。该测试只扫描 `COMMAND_SOURCES` 里写死的 **5 个文件**（`commands/mod.rs`、`ai_cmd.rs`、`config_cmd.rs`、`sync_cmd.rs`、`update_cmd.rs`）——**新建命令模块必须加进这个列表**，否则不被覆盖 |
| `src-tauri/tests/command_key_sweep.rs` | handler 列表与 `cases` 是**手工维护**的，新增命令必须同步登记 |
| `src-tauri/tests/ipc_command_inventory.rs` | 自动比对「前端调用的命令」与「后端存在的命令」；新增命令若暂不接 UI，必须登记进 `RESERVED_COMMANDS`（`:29`），一旦接上 UI 就必须摘掉 |

### 1.4 软删除与 CHECK 约束

- **软删除**：`deleted_at` 列存在于 `novel`、`volume`、`chapter`、`asset`、`worldview_entry`、`character`、`plot_item`；`chapter_revision`、`extraction_candidate`、`setting_change`、`character_relation`、`novel_lexicon` **没有**软删除。
- **CHECK 约束**（加列/改值必须遵守）：`llm_profile.protocol` ∈ (`openai`,`anthropic`)；`chapter.status` ∈ (`draft`,`editing`,`finalized`)；`plot_item.kind` ∈ (`mainline`,`conflict`,`foreshadow`)；`plot_item.status` ∈ (`planted`,`resolved`)；`extraction_candidate.status` ∈ (`pending`,`accepted`,`ignored`)；`setting_change.target_type` ∈ (`worldview`,`character`,`plot`,`outline`,`volume`,`chapter`)；`chapter_revision.revision_type` ∈ (`auto`,`manual`,`ai_before`,`ai_after`,`rollback_before`)；`agent.key` 为 6 个固定值且 UNIQUE。

### 1.5 表清单（迁移文件实际内容）

**V001**（18 张）：`llm_profile` · `novel` · `volume` · `chapter` · `chapter_revision` · `asset` · `worldview_entry` · `character` · `plot_item` · `agent` · `chat_session` · `chat_message` · `extraction_candidate` · `setting_change` · `prompt_template` · `webdav_config` · `app_setting` · `character_relation`

**V002**（+1）：`novel_lexicon`（`id, novel_id, term, category, frequency, source_id, created_at, updated_at`）
**V003**：仅索引（`idx_asset_novel_hash`、`idx_asset_hash`、`idx_plot_item_chapter`、`idx_setting_change_target`、`idx_chapter_revision_auto`）
**V004**：仅加列（`worldview_entry` / `character` / `plot_item` 各增 `pinned INTEGER NOT NULL DEFAULT 0`）
**运行时**（+1）：`schema_version`（`version INTEGER PRIMARY KEY, applied_at TEXT`）

**全文索引已落地（V005）**：FTS5 trigram 虚拟表 `chapter_fts` + 三条同步触发器（`chapter` 的 INSERT/UPDATE/DELETE），存量一次性回填。这是全仓库唯一的虚拟表与触发器集合，见 `src-tauri/migrations/V005__chapter_fts.sql`。

---

## 二、任务书 18 项逐条映射表

判定口径：**已符合** = 任务书描述的能力已存在且满足验收；**部分符合** = 主体存在但关键验收项缺失；**不符合** = 能力不存在；**审计符合** = 「先审计」结论为不需要改。

### 第一类：文档同步

#### T01 实施计划更新为现状版 —— **部分符合**

| 任务书假设 | 实际 | 缺口 |
|---|---|---|
| 实施计划需补 EPUB 导入、图库、词库、关系图谱、多作品看板、版本历史等已实现功能 | §八「P1 功能与交付闭环（全部 100% 交付）」的表**只有 10 项**，且成文早于图库 / 关系图谱 / 多作品看板三项工作；文档内 `图库`、`Force-Graph` **零命中**。已收录的有：EPUB/txt/md 导入、章节历史版本、词库、关系拓扑**网表** | **部分** |
| 删除「导入 MVP 不含」等过时描述 | `Docs/Easy-writing · 实施计划.md:43` 仍写「导入外部稿件：MVP 暂不包含批量解析…」，与 §八 矛盾 | **需改** |
| 需新增「当前 Schema」章节 | 现有 §五标题为「数据库 Schema（**初始 V001 DDL 基线**）」（`Docs/Easy-writing · 实施计划.md:261`）——**已过时**，实际有 V002/V003 共 20 张表 | **需改**：标题与内容刷新为 V001–V00N 实际结构 |
| 需提供 `scripts/check-docs-schema` 校验并接入 CI | **不存在**（`scripts/` 仅有 `check-version.mjs`、`release-gate.mjs`、`tauri-build-retry.sh`） | **需新建** |
| 删除「导入 MVP 不含」等过时描述 | 需逐条核对 | 待核 |

**工作量**：中。**关键设计决定**：护栏脚本才是防再次失真的持久机制 —— 建议 §五 的**内容**在全部迁移落地后做一次终版刷新，脚本 + CI 先行落地。

#### T02 导出功能（txt / docx / epub）—— **不符合（全新）** → **已落地（PR-07，2026-10-22）**

> 落地说明：Rust 导出模块 `src-tauri/src/export/`（`mod.rs` 编排 / `doc.rs` 文档模型 /
> `assets.rs` 图片解析 / `txt.rs` / `docx.rs` / `epub.rs`），
> 命令层 `src-tauri/src/commands/export_cmd.rs`（3 个命令），
> 前端弹层 `frontend/src/components/export/export_modal.rs`，
> 集成测试 `src-tauri/tests/export_novel.rs`（12 例）+ `tests/frontend/export_modal.test.js`（6 例）。
> **本任务未新增数据库迁移**（`BASELINE_VERSION` 保持 3）。
> **依赖偏离**：映射表原计划引入 `docx`（docx-rs）与 `epub-builder`，
> 实际**只新增 `tauri-plugin-dialog`** —— DOCX 与 EPUB 都用仓库已有的 `zip` 9 自拼。
> 理由：这两份格式的「几件套」形状稳定，自拼能精确控制
> 「图片只写一份、目录层级与卷结构一一对应、产物能被本仓库导入器读回」，
> 而引入两套重型依赖会带来体积与版本风险。任务书原文允许「评估并说明」。

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 新增命令 `export_novel` | `export_novel` / `choose_export_file` / `cancel_export` 三个命令（拆开是为了让「取消导出」与「关掉对话框」可区分） | **已符合** |
| 范围可选整书/多卷/多章 | `ExportOptions.volume_ids` / `chapter_ids` 均支持，留空即整书 | **已符合** |
| 卷章标题模板 | `volume_title_template` / `chapter_title_template`，占位符 `{novel}` `{volume_index}` `{volume_title}` `{chapter_index}` `{chapter_title}`，未知占位符原样保留 | **已符合** |
| 段首缩进或空行 | `ParagraphSeparator::Indent` / `BlankLine`；txt 用全角空格，docx 用 `firstLineChars`，epub 用 CSS `text-indent` | **已符合** |
| 是否含图片 | `include_images`；txt 恒跳过并计数，docx 写 `word/media/`，epub 写 `EPUB/images/` | **已符合** |
| 文件名 | 由系统「另存为」对话框决定（`tauri-plugin-dialog`），应用不替作者选路径 | **已符合** |
| Tiptap JSON 在 Rust 端转换 | `export/doc.rs` 把 ProseMirror JSON 解析成内部文档模型，三种写出器共用 | **已符合** |
| docx / epub 依赖评估并说明 | 评估后**不引入** `docx-rs` / `epub-builder`，改用现有 `zip` 自拼（见上方落地说明） | **已符合（说明）** |
| 流式写文件 | 逐章写；docx 的 `word/document.xml` 边写章边压 zip，epub 逐章写 xhtml | **已符合** |
| 事件上报进度 | 事件 `novel-export://progress`，载荷 `ExportProgress`；前端 `frontend/src/bridge/events.rs` 订阅 | **已符合** |
| 支持取消 | `cancel_export` 置取消位，导出循环在**章边界**检查，临时文件被删除 | **已符合** |
| 不阻塞 UI | 命令体只做准备工作，重活在 `spawn_blocking` 里跑 | **已符合** |
| 前端新增导出弹层 | `frontend/src/components/export/export_modal.rs`，入口在设置面板工具条「📤 导出」 | **已符合** |
| 300 章/百万字样例可导出且界面不卡死 | `export_novel.rs::large_sample_export_completes_and_matches_stats`（300 章 × 3300 字） | **已符合** |
| 导出字数与应用统计一致 | 导出时按 `doc::tiptap_char_count` **现算**（文本字符数 + 叶节点数），与状态栏 `CharacterCount.characters()` 同口径；不用库里可能过期的 `word_count` 列 | **已符合** |
| docx 可被 Word 打开 | 结构合法（`[Content_Types].xml` / `_rels/.rels` / `word/document.xml` / `word/styles.xml`），测试断言条目齐备 | **已符合**（真机打开待人工确认） |
| epub 可被主流阅读器打开 | `mimetype` 为首条目且不压缩、`container.xml` → OPF、NCX + nav 双目录；**并被本仓库的 `import/epub.rs` 读回**（2 卷 3 章全部还原） | **已符合**（真机打开待人工确认） |
| 含图片和不含图片两种场景均有测试 | `export_with_images_embeds_media` / `export_without_images_skips_and_counts` | **已符合** |
| 取消后不留半成品文件 | 先写 `<目标名>.ew-export-part`，成功才 rename；失败或取消一律删除临时文件 | **已符合** |

#### T03 备份体积控制与保留策略 —— **部分符合** → **已落地（PR-04，2026-10-22）**

> 落地说明：保留策略引擎落在 `src-tauri/src/sync/retention.rs`，
> 集成测试 `src-tauri/tests/backup_retention.rs`（14 例）。
> **本任务未新增数据库迁移**：策略参数存 `app_setting`，快照去重用 Rust 侧
> 计算 SHA-256，不加列。
> 唯一一处与初版设计不同：「最近 N 份」按**文件名字典序**而非解析出的
> 时间戳排序 —— 既有轮转测试用「非法月份」构造过备份名，名字排序对任何
> 名字都成立，不会出现「时间戳解析失败就没有归属」的空洞。

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| ① 章节版本快照按内容哈希去重 | `chapter_revision` 无内容哈希列；`db/chapter_revision.rs:145 maybe_auto_snapshot` 靠「≥300s 且字数差≥50 或 ≥1800s 且字数差≥10」判定 | **缺** |
| ② 每章快照设上限（默认 50，可配置），超限优先清最旧自动快照，保留手动 | **已实现**，但上限是**硬编码 30**、不可配（`db/chapter_revision.rs:190-201`，`revision_type='auto'` 保留最新 30） | **部分**（数量与可配性） |
| ③ 远端保留策略：最近 N 份 + 每日 1 份 + 每周 1 份 | `sync/backup.rs:236 plan_rotation(names, new_backup_verified, keep)` **只做「保留最近 keep 份」**，无每日/每周维度；且 `keep` 是写死的常量 `KEEP: usize = 20`（`backup.rs:480`），不可配 | **部分** |
| ③ 补充：在新备份上传成功后才清理 | **已实现且更严格**：`new_backup_verified=false` 时返回空集，一个都不删（`backup.rs:237-239`） | **已符合** |
| ③ 补充：只处理 `backup_*.db.gz` 命名的文件 | **已实现**（`backup.rs:240-246`） | **已符合** |
| ④ 清理提供 dry-run 预览 | `plan_rotation` 本身是纯函数即天然 dry-run，但**没有暴露给用户的预览入口** | **缺** |
| ⑤ 备份压缩前执行 VACUUM（在备份副本上） | **无** | **缺** |

**工作量**：中。**注意**：`plan_rotation` 的两条安全铁律（先验证后删、只碰 `backup_*`）**必须保留**，扩展策略时不得放宽。

#### T04 多设备覆盖保护 —— **部分符合** → **已落地（PR-05，2026-10-22）**

> 落地说明：设备身份与同步状态在 `src-tauri/src/sync/device.rs`（存库外的
> `sync_state.json`），来源身份 sidecar 为 `<backup>.db.gz.meta.json`，
> 集成测试 `src-tauri/tests/device_guard.rs`（11 例）。
> **本任务未新增数据库迁移**：身份与同步状态刻意存在数据库之外。

**先审计项**（任务书要求先审计 `restore.rs` 替换数据库文件前是否已关闭 r2d2 连接池或要求重启）：

> **结论：符合，且实现方式比任务书假设的更安全。**
> `sync/restore.rs:747 overwrite_target_database` **不替换数据库文件**，而是走 SQLite Online Backup API（`rusqlite::backup::Backup::new(&src_conn, &mut target_conn)` + `run_to_completion`，`:768-777`），把内容页直接拷进**已打开的**目标连接。
> 因此**不存在**「数据库文件被占用」问题，**不需要**关闭连接池，也**不需要**重启应用。任务书基于「文件替换」假设提出的这条审计项在本仓库不成立。

其余核对：

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 生成并保存 `device_id` | **不存在**（全仓库零命中） | **缺** |
| 备份同时上传同名 `.meta.json`（device_id、created_at、库内最大 updated_at、schema_version、app_version、sha256） | sidecar 机制**已存在**：`sync/manifest.rs:478 write_sidecar` / `:504 sidecar_path_for`，文件名规则是 `<backup>.db.gz.json`（**不是** `.meta.json`）；`BackupManifest` 已有 `created_at`(`:87`)、`app_version`(`:89`)、`schema_version`(`:91`)、`compressed_sha256`(`:103`)、`missing_assets`(`:111`) | **部分**：缺 `device_id`、缺「库内最大 updated_at」 |
| 上传前读远端最新 meta，其他设备且晚于本地 `last_sync_at` → 提示「继续上传/先恢复/取消」 | `last_sync_at` **不存在**；冲突判定逻辑**不存在** | **缺** |
| 恢复前强制校验 sha256 | **已实现**：`manifest.rs:283 verify_manifest` + `compressed_sha256` 绑定（`manifest.rs:98-103` 注释说明「少了它，把 A 备份的清单挂到 B 备份旁边不会被发现」） | **已符合** |
| 先在本地做保底快照 | **已实现**：`restore.rs:684 commit_restored_database` 阶段 1 创建 `pre_restore_*.db` 快照，失败则拒绝覆盖 | **已符合** |
| 拒绝恢复 schema_version 高于当前应用的备份 | **已实现**：`sync/backup.rs:192-198` 预检时比较 `schema_version > BASELINE_VERSION` 并给出明确提示（`该备份由更高版本的 Easy-writing（schema v{}）生成…`）。另 `schema_version` 读不到时**报错而非按 v0 放行**（`backup.rs:185-190`） | **已符合** |
| 「退出自动同步」遇冲突不弹阻塞窗口，跳过并下次启动提示 | 机制**不存在** | **缺** |

**工作量**：中。**关键设计决定**：`device_id` 与 `last_sync_at` **必须存在库外**（如 `app_data_dir/sync_state.json`），不能存进 `app_setting` 或业务表 —— 恢复流程会整体覆盖 `.db`（`restore.rs:747`），库里存的身份会被**来源设备**的 id 覆盖，跨设备保护随即失效。

#### T05 换设备后的密钥缺失处理 —— **部分符合** → **已落地（PR-06，2026-10-22）**

> 落地说明：统一错误码与体检在 `src-tauri/src/keys.rs`，
> 钥匙串访问被抽象成 `KeyringStore` trait（`src-tauri/src/keyring.rs`），
> 集成测试 `src-tauri/tests/key_missing.rs`（9 例）。
> **本任务未新增数据库迁移**：密钥状态在系统钥匙串里，体检结果不落库。

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 启动和恢复完成后检查所有 `llm_profile.key_ref` 与 `webdav_config.pwd_ref` 在钥匙串中是否存在 | `sync/restore.rs:1121 collect_dangling_keys` **只扫 `llm_profile.key_ref`**，**不扫 `webdav_config.pwd_ref`**；且只在恢复流程里调用（`restore.rs:1071`），**启动时无检查** | **缺一半** |
| 缺失则在设置页显示「缺少密钥」徽标 | 只有恢复结果弹窗里列出缺失项（`frontend/src/components/config/webdav_tab.rs:415,466-470`），**无设置页常驻徽标、无启动时检查、无跳转链接** | **缺** |
| AI 与 WebDAV 调用前预检，统一返回 `KEY_MISSING` | **预检已存在**，但**没有统一错误码**。`agent/dispatcher.rs:41-43` 在取到空密钥时返回「当前绑定的模型接口未检测到有效的 API Key 密钥，已在调用前拦截…」；`commands/config_cmd.rs:212-214` 同样拦截；`commands/sync_cmd.rs:27-28` 用 `map_err` 返回「未在系统钥匙串中找到 WebDAV 密码凭据」。三处都是**各自手写的中文自由文本**，没有 `KEY_MISSING` 这样的机器可判定标识 | **部分**（预检有，错误码无） |
| 恢复完成后弹窗汇总需要补填的项 | **已实现**（`restore.rs` 返回 `dangling_keys`，`shared-types/src/lib.rs:597 DanglingKeyInfo`） | **已符合** |
| 备份中不得包含任何密钥 | **已符合**：`manifest.rs:46-51` 明确 `COUNTED_TABLES` 不含 `llm_profile` / `webdav_config`；`manifest.rs:17` 注明「API Key、WebDAV 密码、钥匙串 key_ref、服务器地址一律不写入」 | **已符合** |
| 有测试（可对 keyring 做 trait 抽象后 mock） | `src-tauri/src/keyring.rs` **只有 3 个自由函数、无 trait 抽象**，因此当前无法 mock | **缺** |

**工作量**：中。**关键设计决定**：本任务的核心不是「加个徽标」，而是**把三处各自手写的中文错误统一成机器可判定的 `KEY_MISSING` 错误码**，并把检查覆盖面从「恢复时、仅 `llm_profile`」扩到「启动时 + `webdav_config.pwd_ref`」。

#### T06 上下文装配透明度与预算 —— **已落地（PR-08，迁移 V004）**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 重构 `ai/context.rs` | `src-tauri/src/ai/context.rs` 按优先级（作品梗概 > 当前章 > 出场角色 > 待回收伏笔 > 世界观）装配；常量 `DEFAULT_CONTEXT_BUDGET_CHARS = 12_000`(`:31`)、`DEFAULT_CONTEXT_BUDGET_TOKENS = 8_000`(`:34`)、`MAX_SINGLE_ENTRY_CHARS = 900`(`:40`) | **已符合** |
| 角色（name 与 aliases）命中本章正文 | `character_appears` 按本名或别名匹配，`split_by_presence` 分出场/未出场 | **已符合** |
| 词库术语命中 | 装配器按词库术语命中正文产生 `lexicon` 条目（reason=`lexicon`） | **已符合** |
| 最近 K 章摘要 | 当前章之前章节以摘要入选（reason=`recent_summary`） | **已符合** |
| 被标记为「硬性」的设定始终带入 | V004 迁移为三表加 `pinned INTEGER NOT NULL DEFAULT 0`；`set_entry_pinned` 命令持久化；pinned 条目在段内最前且免于预算裁剪（reason=`pinned`） | **已符合** |
| `llm_profile.params` 增加 `context_window` | `shared-types` 的 `GenerationParams` 增 `context_window: Option<u32>`；预算解析 `params.context_window.unwrap_or(8000)` token，字符预算按 1.5 字符/token 换算；接口配置弹层可填写 | **已符合** |
| 通过 `TokenEstimator` trait 估算 token | `TokenEstimator` trait（`context.rs:59`）+ `CharCoefficientEstimator` 默认实现（1 token ≈ 1.5 字符） | **已符合** |
| 返回 `ContextManifest`（类型、id、标题、入选原因、token、被裁剪项、预算） | `shared_types::ContextManifest { budget_tokens, used_tokens, entries, trimmed }`，条目含 `entity_type` / `entry_id` / `title` / `reason` / `token_count` / `included`；命令 `get_context_manifest` | **已符合** |
| 前端「本次上下文」面板，可取消勾选或强制加入 | 前端 `context_panel.rs` 挂进聊天侧栏；每条可勾选（pinned 强制加入），命令 `set_entry_pinned` | **已符合** |

**验收**：`src-tauri/tests/context_manifest.rs`（6 个命令级用例：预算解析链、reason 全集、pinned 持久化与切换、紧缩预算裁剪、无配置报错、无章节降级）；`src/ai/context.rs` 内 `#[cfg(test)]` 单元测试覆盖优先级/命中/裁剪；`tests/frontend/context_panel.test.js`（7 条前端契约）。

**工作量**：已落地。**依赖**：`T08` 可复用本任务的 `pinned`；`T10` 可复用本任务的匹配逻辑。

#### T07 提炼成本与去重 —— **已落地（PR-10，迁移 V006）**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 新增设置 `extraction_mode`：`parallel_4`（默认，现状）与 `single_call` | 设置键 `extraction_mode` 存 `app_setting`，缺省 `parallel_4`；`single_call` 用一次调用返回四类候选。设置页「写作偏好」可切换。并行模式仍为四类 Agent 受限并行（`Semaphore::new(2)`） | **已符合** |
| 去重增强：归一化名称、别名匹配已有角色与候选池 | `detect_possible_duplicate` 在**入库前**判定：先查既有角色（别名精确命中、名称/别名的包含关系、2–4 字短名共享至少一半字符），再查待审候选池；命中写入 `possible_duplicate_of`。规则**只用于提示，不自动合并** | **已符合** |
| 命中则标记 `possible_duplicate_of`（新增列） | V006 为 `extraction_candidate` 加 `possible_duplicate_of TEXT` + `idx_extraction_duplicate`；候选结构体与待审列表均已带该字段 | **已符合** |
| 提取面板提示「疑似同一人物，是否合并」，合并把新名称写入 `aliases` | 面板显示「疑似与已有角色「林舟」是同一人」+「合并到「林舟」」；`merge_character_candidate` 把候选标题写入 `aliases`（归一化去重）、描述并入角色卡 `raw_notes`、同步词库，并把候选标记 `accepted`。候选池锚点只提示「与本批候选疑似同人」 | **已符合** |
| 超长章节按段落分块提炼后合并（阈值可配） | `split_into_chunks` 按行切块（单行超阈值时按字符硬切），阈值 `extraction_chunk_threshold`（默认 12000 字符，低于 500 按默认）；分块产生多条章纲时合并为一条（「；」连接） | **已符合** |

**验收**：`src-tauri/tests/extraction_dedup.rs`（10 例：小名疑似、别名命中、无关名不提示、批内命中、锚点落库回读、合并写别名/并角色卡/标记采纳/不新建重复角色、别名幂等、非角色拒绝合并、设置默认值与解析）；`src/agent/extraction.rs` 单元测试 9 例（分块边界、超长行硬切、single_call 两种输出形态同构、相似度规则、锚点优先级、章纲合并）；`tests/frontend/extraction_dedup.test.js`（6 条跨端契约）。

**工作量**：已落地。

#### T08 冲突检测结构化与可配置 —— **部分符合**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 检测输出固定为结构化 JSON：冲突点、关联设定（类型+id）、证据、置信度 | `ai/consistency.rs:80-90` 的 prompt **已是固定 JSON**：`has_conflict`/`conflict_point`/`setting_module`/`setting_title`/`reason`/`proposed_change`；`consistency.rs:26 interpret_response` 有解析与「非 JSON 判为未检测」的分支（`:141`） | **部分**：缺「关联设定的 **id**」「证据」「置信度」三个字段 |
| 设置项：前置检查开关、检查范围、置信度阈值 | 三项**全无**（`app_setting` 可承载，无需迁移） | **缺** |
| `ConflictCard` 展示依据 | 现有 `frontend/src/components/ai/conflict_modal.rs` 展示 `reason`/`proposed_change` | **部分** |
| 提供「本章忽略此类提示」 | **无** | **缺**（需表） |
| 保持 `setting_change` 留档与撤销不变 | `db/settings.rs:333 record_setting_change` / `:389 revert_setting_change` 已在 | **已符合**（勿动） |

**工作量**：中。**依赖**：需要 T06 的 `pinned`（检查范围「仅硬性设定/全部」）。

#### T09 全书搜索与替换 —— **已落地（PR-09，迁移 V005）**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 先确认 rusqlite 特性支持 trigram（≥3.34） | bundled SQLite **3.46.0**，`-DSQLITE_ENABLE_FTS5` 已启用，trigram 分词器可用。**未新增依赖、未改 Cargo 特性** | **已符合** |
| 新增迁移创建 FTS5 虚拟表（trigram），覆盖章节标题与 `content_plain`，触发器同步并回填存量 | `V005__chapter_fts.sql`：`chapter_fts`（`chapter_id`/`novel_id` UNINDEXED + `title` + `content_plain`，`tokenize='trigram'`）+ 三条触发器（`chapter_fts_ai`/`au`/`ad`，软删出索引）+ 存量回填 | **已符合** |
| 查询不足 3 字符时回退 LIKE | `db/search.rs` 的 `MIN_FTS_CHARS = 3`：≥3 字符走 `MATCH`（按 `rank`），<3 字符走 `content_plain LIKE`，两条路径返回同构结果；面板对作者明示该限制 | **已符合** |
| 命令 `search_chapters` 返回命中片段与章节定位 | 新命令 `search_chapters(novel_id, query, limit?)` → `ChapterSearchHit{chapter_id, volume_id, title, hits, snippets, has_body}`，最多 3 条片段；MATCH 串按短语转义，`AND`/`"`/`*` 不炸 | **已符合** |
| 替换在 Tiptap JSON 的文本节点上进行，不破坏样式标记与图片节点 | `replace.rs` 在**拼接后的文本流**上匹配再映射回节点区间：跨节点命中也能命中；只改 `{"type":"text"}` 的 `text`；marks 与 image 节点原样保留；删空的 text 节点整条移除 | **已符合** |
| 执行前为受影响章节自动创建版本快照，支持整批撤销 | `apply_replace` 在**同一事务**内逐章「先写 `manual` 快照（含 `batch_id`）→ 再改写」，任一章失败整体回滚；`undo_replace_batch` 按 `batch_id` 整批还原，撤销后与替换前逐字一致 | **已符合** |
| 百万字样例搜索响应可接受 | `perf_benchmark.rs::perf_fulltext_search_baseline` 覆盖 600 章合成作品；索引体积与耗时见 PR 描述实测 | **已符合（基准已接 FTS）** |

**验收**：`src-tauri/tests/chapter_search.rs`（8 例：FTS 命中、两字回退 LIKE、FTS 语法字符、软删与索引同步、正文更新后索引跟随、作品隔离、limit 语义、空查询）；`replace_engine.rs`（11 例：marks/图片保留、**跨节点命中**、空 text 节点移除、字数与 `content_plain` 口径、大小写、全量替换、非法 JSON 拒绝）；`apply_replace.rs`（5 例：批次快照共享 `batch_id`、逐字还原、批次隔离、`manual` 类型不被自动清理、无命中不留快照）；`tests/frontend/search_modal.test.js`（9 条前端契约）。

**README 承诺已修正**：`README.md:66` 由「尚未交付」改为真实描述（含 3 字符限制）。

#### T10 伏笔看板 —— **部分符合**

- `plot_item` 已有 `status` CHECK ∈ (`planted`,`resolved`)（`V001:128`）与 `chapter_id`（`V001:129`）——**埋设章节字段已有**。
- 前端 `frontend/src/components/settings_panel/plot_lines.rs` 已有伏笔列表，且**状态切换按钮已在渲染「已埋设」/「已回收」**（`:136-149`），新建时默认 `kind="foreshadow"` / `status="planted"`（`:29-30`）。命令 `list_plots` / `save_plot` / `delete_plot` 齐备（`commands/mod.rs:466-494`）。
- **但没有**：伏笔总览看板、距今章数计算、超过阈值未回收高亮、关联角色、状态栏未回收计数、一键跳转、`resolved_chapter_id`（回收章节**未被存储**，`resolved` 只是一个状态位）、「标记回收」操作。
- **额外坑**：前端新建伏笔时 `save_plot` 传的是 `chapter_id: None`（`plot_lines.rs:31,107`）—— **埋设章节只有 AI 提炼路径会写**（`ai_cmd.rs:503`），人工新建的伏笔没有埋设章节。T10 必须一并解决这个数据缺口，否则「距今章数」对人工伏笔无意义。
- **工作量**：中偏大。**依赖**：复用 T06 的匹配逻辑（判断某伏笔涉及哪些角色）。

#### T11 写作辅助 —— **不符合（全新）**

- `daily_stat` 表、`daily_goal` 设置：**零命中**。
- 专注/打字机模式：**零命中**。
- `chapter.pov_character_id`、`chapter.story_time`：**零命中**（`chapter` DDL 见 `V001:39-54`）。
- **可复用的既有机制**：`chapter.word_count` 已在维护（`chapter` DDL `:47`），保存路径为命令 `save_chapter_content`；`frontend/src/components/editor/status_bar.rs` 已有状态栏。
- **工作量**：大。

### 第三类：安全与发布

#### T12 Asset 协议与 CSP（**先审计**）—— **审计结论：配置符合；越界路径需实测**

| 任务书假设 | 实际 | 判定 |
|---|---|---|
| `assetProtocol.scope` 是否覆盖实际图片目录层级（`*` 通常只匹配一层，按需改为 `**`） | `src-tauri/tauri.conf.json:28` scope 已是 `["$APPDATA/images/**"]`，**已是 `**`**。且 `src-tauri/src/asset.rs:52-55` 显示图片是**扁平**存放（`images/<sha256>.<ext>`），本就没有嵌套子目录 | **符合** |
| CSP 不得为 null，仅放行必要的 img-src（asset 协议）与 connect-src | `tauri.conf.json:30` CSP **已非 null**：`default-src 'self' tauri:; img-src 'self' tauri: blob: data: asset: http://asset.localhost; style-src …; script-src … 'wasm-unsafe-eval'; connect-src 'self' ipc: http://ipc.localhost tauri:;` | **符合** |
| 嵌套目录中的图片可显示 | 目录扁平，无嵌套场景 | **不适用** |
| 通过 `../` 构造的越界路径无法读取 | 依赖 Tauri asset 协议自身；仓库内**无针对此的测试**。且前端 `frontend/src/bridge/asset.rs:37-54` 的 `canonicalize_asset_src` 是**朴素的 `indexOf("images/")` 切片** —— 它把 `images/` 之后的内容原样拼回 `asset://local/`，路径里的 `../` 不会被清洗。是否可越界取决于 Tauri 协议自身的解析，**必须实测** | **缺测试（且需实测前端这一层）** |

**工作量**：小（**补回归测试钉住结论**）。若实测发现 `canonicalize_asset_src` 的朴素切片确实能被 `../` 绕过，则需一并收紧该函数（清洗路径中的 `..` 段），这是本任务唯一的潜在代码改动点。**附带确认**：`connect-src` 未含 `https:` —— 需确认所有外部网络访问都走 Rust 侧 `reqwest`（已确认：`commands/update_cmd.rs:63`、`ai/gateway.rs`），故前端无跨域请求，**无需放宽 CSP**。

#### T13 更新检查与校验和 —— **部分符合**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 发布流程生成 SHA256SUMS 并随 Release 上传 | **无**（`.github/workflows/release.yml` 无此步骤；`Docs/双远程发布流程.md` 亦无） | **缺** |
| 应用内更新提示展示安装包 SHA-256 与校验方法 | `commands/update_cmd.rs` 已取回 release 的 `assets`（name/size/download_url，`:106-114`）并按平台选下载链接（`:118-135`）；**但无 SHA-256** | **缺** |
| 仍保持「只跳转下载页、不自动安装」 | **已实现且更严格**：`commands/update_cmd.rs:158 open_external_url` 只允许 http(s)，并额外拒绝本机/私有网段与云元数据地址（`extract_host`/`is_local_or_private_host`，`:204-251`） | **已符合** |
| Docs 中的发布流程已更新 | 待改 | **缺** |

**工作量**：小–中。

#### T14 打包与签名（**先审计**，依赖 T13）—— **部分符合**

**审计结论**：现有构建矩阵**不满足**任务书要求。

| 维度 | 实际 | 证据 |
|---|---|---|
| 构建矩阵 | 仅 2 项：`macos-latest`（`--target aarch64-apple-darwin`）与 `windows-latest`（无 target 参数 → x86_64-pc-windows-msvc） | `.github/workflows/release.yml` `strategy.matrix.include` |
| macOS Intel / universal | **无** | 同上 |
| macOS 签名与公证 | **无** | 全文无 `codesign`/`notarytool`/`APPLE_*` |
| Windows 代码签名 | **无** | 全文无 `signtool`/`WINDOWS_*` |
| CI job 现状 | 5 个：版本一致性 / 前端 / 后端 / WebDAV 真实服务端集成 / 性能门禁 | `.github/workflows/ci.yml` |

**工作量**：中。**硬约束**：所有用 secrets 的步骤必须加 `if:` 条件，**未配置 secrets 时 CI 仍须跑通**。

#### T15 WebDAV 明文传输警告 —— **部分符合**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 保存配置与测试连接时，若地址为 `http://` 则弹出明确警告并要求二次确认 | `sync/webdav.rs` **无任何协议校验**（全文无 `http://` 判断）；前端 `components/config/webdav_tab.rs:64,258` 的默认值与占位符**已是 `https://`** | **缺** |
| 设置页显示「明文传输」标识 | **无** | **缺** |
| 默认示例与占位符使用 https | **已符合** | **已符合** |
| 本机回环地址是否豁免需在 PR 中说明取舍 | 待决 | — |

**工作量**：小。**建议取舍**：回环地址（`127.0.0.1`/`::1`/`localhost`）豁免 —— 流量不出本机，且本地测试服务器是合法用例。

#### T16 平台口径统一 —— **不符合**

- README **3 处**写「Linux Secret Service」：`README.md:72`、`:89`、`:149`。
- PRD 亦提及（`Docs/Easy-writing · 中长篇小说工作台 框架方案与 PRD.md:24,355`：「Linux、移动端与网页版」暂不做）。
- 实现侧：`src-tauri/Cargo.toml` 的 `keyring` 启用了 `sync-secret-service` 特性 → **代码支持、文档口径与发布目标（macOS/Windows）冲突**。
- **工作量**：小。

#### T17 体积口径与文档命名 —— **不符合**

- 体积：README `:85` 写「~18MB」，`Docs/Easy-writing · 实施计划.md:52` 写「~15MB」。**真实产物**（`release-packages/Easy-writing_0.3.6_*`）：macOS DMG **7 364 860 B ≈ 7.4 MB**、macOS zip **7 097 565 B ≈ 7.1 MB**、Windows zip **5 833 991 B ≈ 5.8 MB**。**两个数字都与真实值差 2 倍以上**，不只是「不一致」。
- 文档命名：`Docs/` 下 25 个条目中大量中文文件名（如 `Easy-writing 审校报告-第三方.md`、`双远程发布流程.md`、`提交流程.md` 等），且被 README 与其它文档大量引用。
- 无链接检查脚本。
- **工作量**：中（重命名 + 全仓库链接更新 + 链接检查脚本）。

#### T18 CHANGELOG 与 README 截图 —— **部分符合**

| 任务书要求 | 实际 | 判定 |
|---|---|---|
| 新增 `CHANGELOG.md`（Keep a Changelog 格式），依据现有 Release 与关键提交回填 | **已存在**，`CHANGELOG.md` **23 845 B**，覆盖 `0.1.0`–`0.3.8` 共 8 个版本，中文标题 + Keep a Changelog 结构 | **已符合** |
| README 增加 7 处功能截图位（写作台、设定面板、AI 修饰对比、冲突卡片、提炼面板、关系图谱、备份设置），图片放 `Docs/images/` 并写 alt 文本 | `Docs/images/` 现仅有 `ui-layout.png` / `ui-layout.webp`；README 无截图位 | **缺** |

**工作量**：小（Agent 无法运行界面截图 → 按任务书允许的方式放占位 + 截图清单，由人工补图）。

---

## 三、任务书自身错误更正表

| 位置 | 任务书表述 | 源码事实 | 处理 |
|---|---|---|---|
| §0 全局规则 3 | 「只能新增迁移文件（**refinery**，下一个可用版本号）」 | **无 refinery**；是手写阶梯 `db/mod.rs:117-169` | 规则**意图**（只加新文件、不改已发布迁移）保留；**执行步骤**改为四步（见 §1.1） |
| T04 | 「**先审计**：restore.rs 替换数据库文件前是否已关闭 r2d2 连接池或要求重启」 | 恢复走 Online Backup API，**不替换文件** | 审计结论「符合」，该问题在本仓库**不成立** |
| T07 | 「`parallel_4`（现状）」 | 4 个并行 agent kind 提炼 —— **表述正确**；并发上限另有 2 | **已澄清**：两个维度（4 类 Agent / 并发 2）在 PR-10 中写明 |
| T17 | 「当前分别写 ~18MB 与 ~15MB」 | 真实 ~7.4MB / ~7.1MB / ~5.8MB | 两处都要改成实测值，**不是只统一** |
| T05 | 「有测试（可对 keyring 做 trait 抽象后 mock）」 | `keyring.rs` 只有自由函数、无 trait | 需**先做 trait 抽象**才有测试 |
| §2 T01 | 「新增『当前 Schema』章节」 | §五 标题为「初始 V001 DDL 基线」 | 改为「当前 Schema（V001–V00N）」 |
| 全文 | （任务书未提） | `README.md:66` 原宣称「全文搜索替换」但功能不存在，**已随 PR-09 落地并改写为真实描述** | 已闭环 |

---

## 四、再基线后的工作量汇总

| 分级 | 任务 | 说明 |
|---|---|---|
| **已符合，仅需补测试/结论** | T12 | 补越界路径与 CSP 回归测试 |
| **部分符合，缺口明确且有限** | T08、T13、T15、T16、T17、T18 | 见上表逐条「缺口」列 |
| **部分符合 → 已落地** | T03、T04、T05 | PR-04 / PR-05 / PR-06（2026-10-22），均零数据库迁移 |
| **部分符合 → 已落地** | T02、T06、T07、T09 | PR-07（导出，零迁移，2026-10-22）/ PR-08（V004：pinned，2026-10-23）/ PR-09（V005：FTS5，2026-10-23）/ PR-10（V006：疑似同人锚点，2026-10-23） |
| **部分符合，缺口较大** | T10 | 需新增看板与回收章节存储 |
| **完全缺失，全新建设** | T11 | T02 已于 PR-07 落地 |
| **文档同步** | T01 | 护栏脚本 + §五 刷新 |

**建议合并顺序**（依赖硬约束）：

```
PR-01 现状映射表（本文件）
PR-02 T01 文档 schema 护栏脚本 + CI
PR-03 T12 Asset 协议与 CSP 审计（补测试）
PR-04 T03 备份保留策略扩展            ← 已完成（2026-10-22，零迁移）
PR-05 T04 设备标识与跨设备保护        ← 已完成（2026-10-22，零迁移）
PR-06 T05 密钥缺失统一处理          ← 已完成（2026-10-22，零迁移）
PR-07 T02 导出 txt/docx/epub        ← 已完成（2026-10-22，零迁移）
PR-08 T06 上下文装配透明度        ← 已完成（2026-10-23，迁移 V004）
PR-09 T09 全书搜索与替换        ← 已完成（2026-10-23，迁移 V005）
PR-10 T07 提炼成本与去重        ← 已完成（2026-10-23，迁移 V006）
PR-11 T08 冲突检测结构化          ← 依赖 T06 的 pinned
PR-12 T10 伏笔看板                ← 复用 T06 匹配逻辑
PR-13 T11 写作辅助
PR-14 T13 校验和与更新提示
PR-15 T14 打包与签名
PR-16 T15 WebDAV 明文警告
PR-17 T16 平台口径统一
PR-18 T17 体积口径与文档命名
PR-19 T18 README 截图位
```

**迁移编号分配**（每个 PR 内部不合并，编号随合并顺序递增）：

| 迁移 | PR | 内容 |
|---|---|---|
| V004 | PR-08 | `worldview_entry` / `character` / `plot_item` 增 `pinned` —— **已落地** |
| V005 | PR-09 | `chapter_fts` FTS5(trigram) 虚拟表 + 触发器 + 回填；`chapter_revision` 增 `batch_id` —— **已落地** |
| V006 | PR-10 | `extraction_candidate` 增 `possible_duplicate_of` —— **已落地** |
| V007 | PR-11 | 新增 `chapter_conflict_ignore` 表 |
| V008 | PR-12 | `plot_item` 增 `resolved_chapter_id` |
| V009 | PR-13 | 新增 `daily_stat` 表；`chapter` 增 `pov_character_id` / `story_time` |

**新增第三方依赖**（集中在 PR-07）：实际只引入 `tauri-plugin-dialog`（MIT/Apache-2.0）——
原计划的 `docx`（docx-rs）与 `epub-builder` 经评估后**不引入**，DOCX 与 EPUB 改用仓库已有的
`zip` 9 自拼（理由见 §二 T02 落地说明）。**T09 不需要任何新依赖。**

> ✅ **已闭环**：`README.md:66` 的「全文搜索替换」承诺曾先于实现存在；PR-09 落地后已改写为真实描述（含 3 字符回退限制）。

---

## 五、全局实施约束（每个 PR 都要满足）

1. **迁移四步**（§1.1）：加 `.sql` 文件 + `init_db` 加 `if applied < N` 块 + 递 `BASELINE_VERSION` + 扩展 `src-tauri/tests/migration_ladder.rs`。只新增，不改 V001–V003。
2. **迁移测试**：每个新迁移都要有「旧库升级后数据不丢」的用例（`src-tauri/tests/migration_ladder.rs` 已有 `HistoricalState` 夹具可复用：枚举见 `:185`，构造 fixture 见 `:399`）。
3. **IPC 契约三处**（§1.3）：`ipc_argcase.rs` 的 `COMMAND_SOURCES`、`command_key_sweep.rs` 的 handler 列表、`ipc_command_inventory.rs` 的前端对接（暂不接 UI 的命令登记进 `RESERVED_COMMANDS`）。
4. **命令命名**：`#[tauri::command(rename_all = "snake_case")]` 不可省。
5. **测试门**：`pnpm test:frontend` + `pnpm test:scripts` + `cargo test -p easy-writing-core -p easy-writing-frontend` 全绿；不得删除或放宽既有测试。
6. **新依赖**：PR 描述须说明用途、许可证（须与 MIT 兼容）、体积影响。
7. **备份/恢复/批量替换**：先在数据库副本上验证，禁止直接操作用户真实库。
8. **不顺手重构**：发现其他问题记入 PR 的「后续建议」。
9. **双远程提交**：GitHub 在主目录提交（分支 + PR）；CNB 在 `.cnb-worktree` 用 `cherry-pick --no-commit` + 重新 commit 取得 CNB 身份（详见 `Docs/提交流程.md`）。
10. **PR 描述模板**：变更摘要 / 涉及迁移 / 测试清单与结果 / 验收证据 / 风险与回滚方式 / 后续建议。

---

## 六、需要人工提供（Agent 不得编造）

| 事项 | 用在哪 | 状态 |
|---|---|---|
| **本映射表的确认** | 任务书全局规则 1：映射表不确认不进入改动 | 待确认 |
| Apple / Windows 代码签名证书与 CI secrets | PR-15 | 待提供 |
| 真实运行截图 7 张（写作台、设定面板、AI 修饰对比、冲突卡片、提炼面板、关系图谱、备份设置） | PR-19 | 待提供 |
| 可用的大模型测试 Key | PR-08 / PR-10 / PR-11 的端到端验证（单元测试用固定模型返回样例，不强依赖） | 待提供 |

---

## 七、本次核对的未决项

- `Docs/Easy-writing · 实施计划.md` 中「导入 MVP 不含…」等过时描述的逐条位置，PR-02 落笔时核对。
- `plot_item.kind` 的三个 CHECK 值（`mainline`/`conflict`/`foreshadow`）与 T10「伏笔」口径的映射关系，PR-12 落笔时确认。
- ~~`chapter_revision` 是否需要新增「批次 id」列以支撑 T09 的整批撤销~~ —— **需要，已在 V005 落定**（`batch_id TEXT` + `idx_revision_batch`）。
- `restore.rs` 预检已读出 `schema_version` 但未见「高于当前应用即拒绝」的比较，PR-05 落笔时补齐并补测试。

<!-- kimi-badge -->
[![powered by KimiCode](../.github/assets/kimi-badge.svg)](https://www.kimi.com/code)
