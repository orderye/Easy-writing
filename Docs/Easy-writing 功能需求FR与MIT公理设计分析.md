# Easy-writing 功能需求（FRs）提取与 MIT 公理设计分析（V0.3.8 实证修订版）

> **依据**：《Easy-writing · 中长篇小说工作台 框架方案与 PRD》（2026-10-14 立项基线）、[CHANGELOG.md](../CHANGELOG.md)、[current-state-map.md](./current-state-map.md) 与 `v0.3.8` 实际源码（数据库迁移 `V001`–`V009`、21 张业务表 + `schema_version`）。  
> **说明**：FR 编号按功能域整理；DP 编号为本文公理设计分析所用。  
> **日期**：2026-11-26（由立项基线版升级为「理论评审 + V0.3.8 演进同步 + 源码解耦实证」完整版）

---

## 摘要（TL;DR）

1. **理论评审与勘误**：修正初版在独立公理（Axiom 1）矩阵方向上的表述倒置，以及「存在上三角闭环却仍称为解耦设计」的概念矛盾——存在上三角闭环即 **耦合设计（Coupled）**，必须通过架构隔离将闭环消除后，才能收敛为 **解耦设计（Decoupled）**。
2. **V0.3.8 演进同步**：从立项基线（~30 条 FR）扩展为 **46 条 FR（6 大功能域）**，完整纳入 V0.2–V0.3.8 交付的 TXT/MD/EPUB 导入导出、6 维角色卡与关系星云图、专属词库、FTS5 全书搜索与节点级批量替换/整批撤销、透明上下文预算清单（`ContextManifest` + `pinned`）、6 层自愈提炼与疑似同人合并、结构化冲突检测与本章忽略、伏笔看板、每日字数/专注模式/POV、备份保留策略与跨设备保护、统一 `KEY_MISSING` 凭据预检、WebDAV 明文警告、命令面板等能力，并重建 **17 × 17 FR–DP 设计矩阵**。
3. **源码审计与下三角解耦落地**：结合源码审计发现 4 处真实缺陷（`novel.description` 错列与 `"novel"` 违反 CHECK、`resolve_plot` 撤回破坏 `plot_item.content`、`chapter_summary` 提炼 `before_content` 为空、提炼/合并 `reason` 硬编码为用户手动编辑），并在不新增数据库迁移、不破坏 IPC 契约的前提下落地三项解耦改造：
   - 新建统一设定变更事务通道 `src-tauri/src/db/setting_tx.rs`（`SAVEPOINT` 原子化写入 + 留档 + 撤回）；
   - 新建设定索引只读导出视图 `src-tauri/src/db/setting_view.rs`（`ai/context.rs` 与 `ai/consistency.rs` 单向消费，消除重复实现与双向约束）；
   - 强化提炼候选池隔离（`commands/ai_cmd.rs::resolve_candidate` 与 `db/extraction.rs::merge_character_candidate` 全部收口至 `setting_tx`）。

---

## 一、初版分析评审与理论勘误

在对照 Suh《Axiomatic Design》及仓库源码复核初版文档后，发现以下 4 处需要修正与深化的点：

| # | 原文问题 | 评审结论与修正 |
|---|---|---|
| 1 | **2.1 节下三角矩阵方向表述倒置**：原文写「下三角矩阵：DP 只影响自身及*之前*的 FR」 | **勘误**：设计方程为 $\{FR\} = [A]\{DP\}$。矩阵 $[A]$ 为下三角意味着 $A_{ij} = 0\ (j > i)$，即第 $i$ 行的 $FR_i$ 只受 $DP_1 \dots DP_i$ 影响；等价于第 $j$ 列的 $DP_j$ 只影响 $FR_j$ 及其**之后**（$i \ge j$）的 FR，而非「之前」。 |
| 2 | **三、五节「耦合却称解耦」的判定矛盾**：原文指出存在 3 处上三角耦合，但结论仍称「当前属于解耦设计（Decoupled）」 | **勘误**：按公理定义，只要设计矩阵存在无法通过行列重排消除的**上三角非零项与闭环**，该设计即属于**耦合设计（Coupled Design）**，违反独立公理；只有在通过中间层/导出视图切断反向依赖、使矩阵收敛为下三角之后，才是**解耦设计（Decoupled Design）**。 |
| 3 | **14 × 14 矩阵漏标了部分真实依赖与回写边** | **补全**：`DP1`（钥匙串）实际同时影响 `FR-E1/E2`（API Key 引用）与 `FR-E4`（WebDAV 密码引用）；`DP2`（接口与 Agent）同时影响 `FR-D4`（冲突检测）；`DP4`（作品/卷/章表）被 `FR-B` 设定表的外键（`source_chapter_id`、`chapter_id`、`resolved_chapter_id`）前向依赖；而 `DP11`（提炼采纳）与 `DP12`（冲突裁决）在未收口前均直接向 `FR-B` 设定表写入，属于**上三角回写边**。 |
| 4 | **信息公理（Axiom 2）分析偏浅** | **深化**：V0.3.8 已在工程上系统践行了信息公理（最小化信息量以降低实现复杂度与认知/Token 成本），包括：FTS5 trigram 索引 + `<3` 字符自动降级 `LIKE`、单次提炼 `single_call` 与超长分块、上下文预算制 `ContextManifest` + `pinned` 分级裁剪、冲突检测置信度阈值闸门与 `chapter_conflict_ignore` 本章忽略、以及清单校验 + 图片 SHA-256 增量备份。 |

---

## 二、功能需求（FRs）提取（V0.3.8 现状基线，共 46 条）

> 标记说明：`[基线]` 来自 2026-10-14 PRD；`[演进]` 为 v0.2–v0.3.8 新增或深化能力。

### FR 域 A：作品与结构管理（7 条）

| 编号 | 功能需求 | 来源 | 优先级 |
|---|---|---|---|
| FR-A1 | 作品（小说）新建、改名、删除与多作品看板切换 | 基线/演进 | P0 |
| FR-A2 | 卷的新建、重命名、删除、拖拽排序、折叠；卷简介编辑（卷纲） | 基线 | P0 |
| FR-A3 | 章节在卷下新建、重命名、删除、拖拽排序、跨卷移动 | 基线 | P0 |
| FR-A4 | 章节状态管理（草稿/修改中/定稿）、字符数统计、标题搜索与状态筛选 | 基线 | P0/P1 |
| FR-A5 | 删除作品/卷/章/图片进回收站并可恢复（`deleted_at` 软删除） | 基线/演进 | P1 |
| FR-A6 | 作品批量导入：TXT / MD / EPUB 自动识别卷章结构并落库 | 演进 | P1 |
| FR-A7 | 作品导出：TXT / DOCX / EPUB（分卷结构、图片嵌入、自定义标题模板、进度与取消） | 演进(T02) | P1 |

### FR 域 B：设定、关系与伏笔管理（7 条）

| 编号 | 功能需求 | 来源 | 优先级 |
|---|---|---|---|
| FR-B1 | 世界观条目增删改、拖拽排序、分组折叠、搜索 | 基线 | P0/P1 |
| FR-B2 | 角色卡增删改与 6 维全息解构（身份/外貌/性格/阵营/背景/目标/关系/`raw_notes`） | 基线/演进 | P0 |
| FR-B3 | 剧情条目增删改、类型（主线/冲突/伏笔）、关联埋设章 | 基线 | P0 |
| FR-B4 | 大纲三层编辑：全篇梗概（`novel.synopsis`）、卷纲、章纲与右栏同步 | 基线 | P0 |
| FR-B5 | 人物关系网络管理与力导向关系星云图可视化 | 演进 | P1 |
| FR-B6 | 专属小说词库管理（世界观/角色术语自动联动）与编辑器联想补全 | 演进 | P1 |
| FR-B7 | 伏笔看板：回收章节存储、跨卷「距今章数」、超期高亮、关联角色与标记回收 | 演进(T10) | P1 |

### FR 域 C：写作台与全局编辑（9 条）

| 编号 | 功能需求 | 来源 | 优先级 |
|---|---|---|---|
| FR-C1 | Tiptap 富文本编辑：标题、样式、对齐、引用、分隔线、列表、图片、撤销重做 | 基线 | P0 |
| FR-C2 | 卷章条：位置显示、快速切换、上一章/下一章、触发「提炼本章」 | 基线 | P0 |
| FR-C3 | 左右栏隐藏/展开状态记忆与顶部栏自适应重排 | 基线 | P0 |
| FR-C4 | 手动保存（Cmd/Ctrl+S）与可调间隔自动保存 | 基线 | P0 |
| FR-C5 | 章节历史快照（自动/手动/AI 前后/回滚前）与历史版本回滚 | 基线 | P1 |
| FR-C6 | 选中文字局部操作：续写、润色、扩写、缩写 | 基线 | P1 |
| FR-C7 | 全书搜索（FTS5 trigram + 不足 3 字符回退 `LIKE`）与节点级批量替换/整批撤销 | 演进(T09) | P1 |
| FR-C8 | 写作辅助：每日净增字数统计、每日目标进度、专注/打字机模式、章节 POV 与故事时间 | 演进(T11) | P1 |
| FR-C9 | 全局命令面板（Cmd/Ctrl+K）快速跳转与触发核心操作 | 演进 | P2 |

### FR 域 D：AI 能力与设定一致性（11 条）

| 编号 | 功能需求 | 来源 | 优先级 |
|---|---|---|---|
| FR-D1 | 写作助手对话框：SSE 流式输出、可中断、失败分类提示与重试 | 基线/演进 | P0 |
| FR-D2 | 上下文装配透明度：按预算分级装配 + `pinned` 恒入 + `ContextManifest` 可视与单条排除 | 基线/演进(T06) | P0 |
| FR-D3 | 严格按设定写作，修改/重写/直接创作只作用于正文，先预览后写入 | 基线 | P0 |
| FR-D4 | 结构化冲突检测（关联设定校验/证据/置信度阈值闸门）+ 冲突提示卡 + 本章忽略 | 基线/演进(T08) | P0 |
| FR-D5 | 「AI 修饰」输入框润色补全 + 对比采用/重试/放弃 | 基线 | P0 |
| FR-D6 | 从正文自动提炼四类设定（世界观/角色/剧情/章摘要），候选逐条采纳/编辑/忽略/全部采纳 | 基线 | P0 |
| FR-D7 | 提炼成本与去重：`parallel_4` / `single_call` 模式、超长分块、正文指纹、疑似同人标记与合并 | 基线/演进(T07) | P0 |
| FR-D8 | 6 层自愈 JSON 解析引擎容错（代码块/闲聊/别名字段/注释/尾随逗号/截断修复）与单 Agent 隔离 | 演进 | P0 |
| FR-D9 | 设定改动留档（`setting_change`）与一键撤销（`revert_setting_change`） | 基线 | P1 |
| FR-D10 | 对话记录随作品保存、按章节筛选；提示词模板自定义与恢复默认 | 基线 | P1 |
| FR-D11 | 图片素材图库管理与失效/缺失引用检测 | 演进 | P1 |

### FR 域 E：设置、安全与备份恢复（8 条）

| 编号 | 功能需求 | 来源 | 优先级 |
|---|---|---|---|
| FR-E1 | 大模型 API 配置（OpenAI/Anthropic 兼容、Base URL 安全校验、`context_window`、多套切换） | 基线/演进 | P0 |
| FR-E2 | 六预设 Agent 各自绑定接口、系统提示词、参数与启用开关 | 基线 | P0/P1 |
| FR-E3 | 密钥存系统钥匙串（`KeyringStore`），不入库不上传；启动与恢复后统一 `KEY_MISSING` 体检 | 基线/演进(T05) | P0 |
| FR-E4 | WebDAV 配置、测试连接与 `http://` 明文传输拦截警告/常驻标识 | 基线/演进(T15) | P0 |
| FR-E5 | 立即备份：图片哈希增量上传 + DB 一致性快照 + manifest 校验清单 + 远端保留策略 | 基线/演进(T03) | P0 |
| FR-E6 | 从云端恢复：清单与 SHA-256 强制校验、替换前本地保底快照、Online Backup API 原子替换、缺失补图 | 基线/演进 | P0 |
| FR-E7 | 多设备覆盖保护：库外 `sync_state.json`（`device_id` + `last_sync_at`）与 sidecar 元数据比对 | 演进(T04) | P0 |
| FR-E8 | 退出异步自动备份（冲突不阻塞）、写作偏好配置与更新校验和（`SHA256SUMS.txt`）展示 | 基线/演进(T13) | P1 |

### FR 域 F：非功能/质量需求（4 条）

| 编号 | 功能需求 | 来源 |
|---|---|---|
| FR-F1 | 流畅：切换章节/保存/600 章搜索与导出不卡顿，只加载当前章，长列表虚拟滚动 | 基线/演进 |
| FR-F2 | 可靠：异常关闭恢复最近自动保存；AI 失败与写盘故障不丢稿、不坏库 | 基线 |
| FR-F3 | 安全与兼容：Windows + macOS（arm64/x64）本地桌面应用；严格 CSP、`asset` 协议越界防护与外部 URL 校验 | 基线/演进(T12) |
| FR-F4 | 可扩展：设定模块、模型协议与导入导出格式可独立扩展而不改其他模块 | 基线 |

---

## 三、V0.3.8 MIT 公理设计：FR–DP 映射与设计矩阵

### 3.1 建立 FR–DP 映射（按依赖从底层到上层排序，17 组）

| 序号 | FR 组 | DP（设计参数） |
|---|---|---|
| 1 | FR-E3 密钥安全与缺失体检 | DP1：`KeyringStore` trait + `key_ref`/`pwd_ref` + `KEY_MISSING` 统一错误码 |
| 2 | FR-E1/E2 接口与 Agent 配置 | DP2：`llm_profile` 表（含 `context_window`）+ `agent` 表 + `validate_base_url` |
| 3 | FR-F3/F4 桌面壳与安全边界 | DP3：Tauri 2 壳 + Leptos/Tiptap + CSP/`asset` 协议/`open_external_url` |
| 4 | FR-A1~A4/C9 结构与命令面板 | DP4：`novel`/`volume`/`chapter` 表（`sort_order`/`pov_character_id`）+ 命令面板 |
| 5 | FR-B1~B7 设定/关系/词库/伏笔 | DP5：`worldview_entry`/`character`/`plot_item`/`character_relation`/`novel_lexicon` 表 |
| 6 | FR-C1~C3/C8/D11 编辑与写作辅助 | DP6：Tiptap 编辑器 + `daily_stat` 表 + `asset` 图库与引用检查 |
| 7 | FR-C4/E8 保存、偏好与更新校验 | DP7：自动保存定时器 + `app_setting` 键值表 + `SHA256SUMS` 更新解析 |
| 8 | FR-A6/A7 导入与导出 | DP8：`import/`（TXT/MD/EPUB）+ `export/`（`zip` 自拼 DOCX/EPUB + 流式进度） |
| 9 | FR-C7 全书搜索与批量替换 | DP9：`chapter_fts`（FTS5 trigram + 触发器）+ `replace.rs` 节点级替换 + `batch_id` 快照 |
| 10 | FR-D2 上下文装配透明化 | DP10：`setting_view` 只读导出视图 + `ContextManifest` 预算装配器 + `pinned` |
| 11 | FR-D1/C6/D10 流式对话与模板 | DP11：`AiGateway`（SSE 流式转发）+ `chat_session`/`chat_message`/`prompt_template` |
| 12 | FR-D5 AI 修饰 | DP12：修饰提示词模板 + 对比采用 UI |
| 13 | FR-D6~D8 设定提炼与去重 | DP13：`extraction` 6 层自愈 + `single_call`/分块 + `extraction_candidate` 候选池 |
| 14 | FR-D3/D4 冲突检测与提示卡 | DP14：`consistency` 结构化判定 + 阈值闸门 + `chapter_conflict_ignore` 本章忽略 |
| 15 | **FR-D9 设定变更事务与留档** | **DP15：`setting_tx` 统一 `SAVEPOINT` 事务通道 + `setting_change` 留档/撤回** |
| 16 | FR-A5/C5 回收站与历史快照 | DP16：`deleted_at` 软删除 + `chapter_revision`（含 `batch_id`） |
| 17 | FR-E4~E7 WebDAV 备份与跨设备保护 | DP17：`sync/`（`manifest` 校验 + 图片增量 + `retention` + `sync_state.json` + Online Backup） |

### 3.2 改造后的完整设计矩阵（17 × 17，严格下三角解耦设计）

行 = FR（按依赖顺序），列 = DP；`●` 表示该 DP 影响该 FR；空位均为 0。

| FR \ DP | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| **1. FR-E3 密钥** | ● | | | | | | | | | | | | | | | | |
| **2. FR-E1/E2 配置** | ● | ● | | | | | | | | | | | | | | | |
| **3. FR-F3/F4 架构** | | | ● | | | | | | | | | | | | | | |
| **4. FR-A1~4 结构** | | | | ● | | | | | | | | | | | | | |
| **5. FR-B1~7 设定** | | | | ● | ● | | | | | | | | | | | | |
| **6. FR-C1~3/8 编辑** | | | ● | ● | ● | ● | | | | | | | | | | | |
| **7. FR-C4/E8 保存** | | | | | | ● | ● | | | | | | | | | | |
| **8. FR-A6/A7 导入导出** | | | | ● | | ● | | ● | | | | | | | | | |
| **9. FR-C7 搜索替换** | | | | ● | | ● | | | ● | | | | | | | | |
| **10. FR-D2 上下文** | | ● | | ● | ● | ● | | | | ● | | | | | | | |
| **11. FR-D1/C6/D10 对话** | ● | ● | | | | ● | | | | ● | ● | | | | | | |
| **12. FR-D5 修饰** | ● | ● | | | ● | | | | | ● | ● | ● | | | | | |
| **13. FR-D6~8 提炼** | ● | ● | | | ● | ● | | | | | ● | | ● | | | | |
| **14. FR-D3/D4 冲突** | ● | ● | | | ● | ● | ● | | | ● | ● | | | ● | | | |
| **15. FR-D9 设定事务** | | | | ● | ● | | | | | | | | ● | ● | ● | | |
| **16. FR-A5/C5 回收/快照** | | | | ● | ● | ● | | | ● | | | | | | | ● | |
| **17. FR-E4~7 备份恢复** | ● | | | ● | ● | ● | ● | | | | | | | | ● | ● | ● |

> **关键变化说明**：
> - **消除上三角闭环 #1/#3（`DP5 ↔ DP10/DP14`）**：通过 `db/setting_view.rs` 将设定索引固化为 DP5 的只读导出视图，上下文装配（DP10）与冲突检测（DP14）单向依赖 DP5，不再反向约束设定表实现，也不再各自重复拷贝 `list_*` 与别名匹配逻辑。
> - **消除上三角闭环 #2（`DP13/DP14 → FR-B`）**：将设定变更事务（`DP15: setting_tx`）独立置于第 15 层，提炼候选池（DP13）和冲突检测（DP14）只产出「候选/裁决意图」，在用户确认采纳或修改设定时统一经 `DP15` 写入并留档，所有 `●` 严格落在主对角线及其左下方。

---

## 四、源码审计发现与三项解耦改造实录

### 4.1 源码审计发现的 4 处真实耦合/缺陷

1. **大纲冲突更新写错列且违反 CHECK 约束**（已修复）：  
   原 `db/settings.rs::apply_conflict_setting_update` 在 `setting_module == "outline"` 且无 `chapter_id` 时，查询并更新了不存在的列 `novel.description`（真实列名为 `novel.synopsis`），且记录 `setting_change.target_type = "novel"`（违反 `V001__init.sql:195` 的 `CHECK(target_type IN ('worldview', 'character', 'plot', 'outline', 'volume', 'chapter'))`），导致全篇大纲冲突落库与撤回在运行时必然失败。
2. **`resolve_plot` 撤回会破坏 `plot_item.content`**（已修复）：  
   原 `db/foreshadow.rs::resolve_plot` 在 `setting_change` 中记录 `before_content = "status=planted"`，而 `db/settings.rs::revert_setting_change` 对 `"plot"` 一律执行 `UPDATE plot_item SET content = ?1`，导致撤回伏笔回收时把剧情正文直接覆写成 `"status=planted"`，且未清空 `resolved_chapter_id`。
3. **提炼采纳与同人合并绕过统一留档语义**（已修复）：  
   - `commands/ai_cmd.rs::resolve_candidate` 在 `"chapter_summary"` 分支直接裸写 `UPDATE chapter SET summary = ...` 并传 `before_content = ""`，丢失旧章纲导致无法真正回滚；  
   - 采纳世界观/角色/剧情与 `db/extraction.rs::merge_character_candidate` 均调用 `db/settings.rs::save_*`，导致 `setting_change.reason` 被硬编码为 `"用户手动编辑"`；且多处使用 `let _ = record_setting_change(...)` 静默吞掉审计错误。
4. **`ai/context.rs` 与 `ai/consistency.rs` 重复实现设定读取与处理**：  
   两处各自调用 `list_worldview` / `list_characters` / `list_plots`，各自维护 `pinned` 分组、字符串截断与别名处理逻辑。

### 4.2 本次落地的三项模块级改造（零迁移、零 IPC 破坏）

| 改造项 | 落地文件 | 核心机制 |
|---|---|---|
| **1. 统一设定变更事务通道** | [setting_tx.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/setting_tx.rs)、[settings.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/settings.rs)、[foreshadow.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/foreshadow.rs)、[novel.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/novel.rs)、[volume.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/volume.rs)、[chapter.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/chapter.rs) | 引入 `with_savepoint`（SQLite `SAVEPOINT` 支持内外嵌套事务），提供 `save_*_with_reason`、`update_chapter_summary_with_reason`、`update_volume_summary_with_reason`、`update_novel_synopsis_with_reason`、`resolve_plot_with_reason`、`apply_conflict_setting_update`、`revert_setting_change`；并将三层大纲手动编辑（全篇梗概/卷纲/章纲）全部纳入留档与撤回通道。 |
| **2. 设定索引只读导出视图** | [setting_view.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/setting_view.rs)、[context.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/ai/context.rs)、[consistency.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/ai/consistency.rs)、[extraction.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/agent/extraction.rs)、[foreshadow.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/foreshadow.rs) | 提供 `load_setting_index`（`pinned` 优先排序）、`truncate_chars`、`parse_aliases`、`match_character_presence`，由上下文装配、冲突检测、提炼索引与伏笔看板单向消费。 |
| **3. 提炼候选池写入收口 & 前端闭环** | [ai_cmd.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/commands/ai_cmd.rs)、[extraction.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/extraction.rs)、[status_bar.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/frontend/src/components/editor/status_bar.rs)、[foreshadow_board.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/frontend/src/components/settings_panel/foreshadow_board.rs) | `resolve_candidate` 全部分支与 `merge_character_candidate` 均接入 `setting_tx`；前端接入 `AppContext.foreshadow_board` 共享信号实现状态栏伏笔未回收/超期只读计数，并补齐 `Cmd/Ctrl+K` 命令面板契约测试。 |

### 4.3 实证验证结果

- **新增专项集成测试** [setting_decoupling.rs](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/tests/setting_decoupling.rs)：**7/7 通过**（含三层大纲手动修改留档撤回与提炼索引复用 `setting_view`）。
  - **独立复跑证据（2026-11-26）**：`cargo test --test setting_decoupling`（于 `src-tauri/` 下执行）→ `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`，7 个测试全部通过，耗时约 0.13s（编译缓存后）。
- **全量 Rust 测试**：`cargo test -p easy-writing-core -p easy-writing-frontend`（覆盖 `easy-writing-core` 121 个单测 + 全部 36 个集成测试文件 + `easy-writing-frontend` 5 个单测）：**全部通过**（`backup_retention` 等本地 mock WebDAV 测试需在允许回环端口绑定的环境运行）。
- **Clippy 静态检查**：`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`：**通过，零警告**。
- **前端契约与脚本测试**：`node --test tests/frontend/*.test.js tests/scripts/*.test.js`（含新增 [command_palette.test.js](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/tests/frontend/command_palette.test.js)）：**110/110 通过**（含 `check-docs-schema` 文档 Schema 护栏）。

### 4.4 仍需外部环境/人工提供的遗留项

- **发布与签名**：macOS / Windows 代码签名证书与 CI secrets、未签名/签名产物的真机安装冒烟验收；
- **CI 证据与门禁**：CI 钉住 `ghcr.io/hacdias/webdav:v5.17.3` 后的真实 WebDAV 集成测试重跑证据、性能门禁阈值收口；
- **人工补图与提交**：README 7 张真实功能截图替换占位图，以及当前工作区改动的 Git 提交与双远程推送。
