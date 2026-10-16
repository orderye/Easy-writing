# Easy-writing · 遗留命令处置与图库 UI 实施方案

> 制定日期：2026-10-16
> 基线：`dcbce2f`（工作树干净，无未提交改动）
> 现状数据：后端 **88** 个 `#[tauri::command]`，前端实际调用 **83** 个，未对接 **5** 个，覆盖率 **94%**
> 数据来源：`src-tauri/tests/ipc_command_inventory.rs` 的反向报告（两侧均从源码自动提取，非人工清单）

---

## 一、问题与处置口径

命令清单核查暴露的 5 个未对接命令，恰恰是本次请求列出的 5 个。逐个定性如下：

| 命令 | 未对接原因 | 处置 | 理由 |
|---|---|---|---|
| `get_novel` | 前端从不需要单查作品 | **删除命令**（保留 db 函数） | 前端持有等价数据源，见 §2.1 |
| `append_chat_message` | 落库已在后端内部完成 | **标注预留** | 见 §2.2 |
| `list_chat_sessions` | 多会话切换无 UI 设计 | **标注预留** | 见 §2.2 |
| `delete_chat_session` | 同上 | **标注预留** | 见 §2.2 |
| `list_image_assets` | 图库 UI 未做 | **本轮接入图库 UI** | 唯一「有明确用户价值、只缺界面」的一个 |

**三个可验收目标**

1. 后端命令数由 88 降到 87，未对接由 5 降到 4（`get_novel` 消失）。
2. 4 个未对接命令中，3 个在**测试里被显式登记为「预留」**，反向报告不再把它们与「疑似遗漏」混在同一个列表里。
3. 图库 UI 落地后，未对接降到 **3**（全部为预留），覆盖率 **96.6%**（84/87）；且图库不止能看图——能插回正文、能发现失效图片。

**非目标（明确划界）**

- 不做图片编辑器：裁剪、压缩、滤镜、标签、多选批量、拖拽排序、导出，均不在本轮。
- 不动正文图片的 URL 语义（见 §9.1 的既有缺陷，单独立项）。
- 不删除 `db::novel::get_novel`——它是内部共享函数，删命令不等于删函数，见 §2.1。

---

## 二、事实核对（方案的地基）

方案中的每一条设计都对应下面这些实测事实，不是估算。

### 2.1 `get_novel` 的调用面

| 位置 | 性质 | 处置 |
|---|---|---|
| `src-tauri/src/lib.rs:50` | `invoke_handler` 注册 | 删 |
| `src-tauri/src/commands/mod.rs:30-34` | 命令定义 | 删 |
| `src-tauri/tests/command_key_sweep.rs:34, 119` | handler 列表 + 用例 | 删 |
| `src-tauri/src/db/novel.rs:41` | **db 层函数** | **保留** |
| `src-tauri/src/db/novel.rs:24`（`create_novel` 内）、`ai_cmd.rs:188/202`、`commands/mod.rs:188` 等 | db 函数的内部调用方 | 不受影响 |

结论：**只有「命令」是死代码，「db 函数」有 4+ 处内部调用方**。前端等价数据源为 `list_novels` 的返回列表与 `AppContext.current_novel` / `novels_list`（`frontend/src/state/mod.rs:6-7`），确实无需单查命令。

### 2.2 对话命令的现状

- `ai_chat_stream`（`commands/ai_cmd.rs:122`）在返回回复前已把 user/assistant **两条消息内部落库**。因此 `append_chat_message` 若再由前端调用，会造成同一条消息写两次。
- 前端现有对话 UI（`components/ai/chat_box.rs`）只用了 3 个命令：`get_or_create_chat_session`（:47）、`list_chat_messages`（:58）、`clear_chat_session`（:99）。
- 缺的是**多会话**：`list_chat_sessions` / `delete_chat_session` 服务的是「一个章节下保留多轮独立对话并切换/删除」这个尚未设计的交互。数据层（`db/chat.rs:91-126`，含 message_count 与按活跃度排序）已经备好，属于典型的「后端先行、UI 后置」。

### 2.3 图片资产链路（图库的全部依赖）

| 环节 | 实现 | 位置 |
|---|---|---|
| 表结构 | `asset(id, novel_id, name, mime, rel_path, size, hash, created_at)`，`novel_id` 外键 **ON DELETE CASCADE**，**无 `deleted_at`** | `migrations/V001__init.sql:76-85` |
| 写入 | base64 → SHA-256 → `images/<hash>.<ext>`，**同内容文件只写一次**，每次上传都插一行（同一 `rel_path` 可对应多行） | `asset.rs:9-61` |
| 列表 | 按 `novel_id` 过滤，`created_at DESC`，无分页 | `asset.rs:64-94` |
| 已具备的命令 | `save_image_asset` / `get_app_data_dir` / `list_image_assets` | `commands/mod.rs:478-514` |
| 上传入口 | 选文件 → FileReader → base64 → `save_image_asset` → `get_app_data_dir` → `bridge/asset.rs` → `tiptap::insert_image` | `components/editor/toolbar.rs:53-132` |
| 路径 → URL | `convertFileSrc(appDataDir + "/" + rel_path)`，失败原样返回 | `frontend/src/bridge/asset.rs:11-38` |
| 资产协议 | `assetProtocol.enable=true`，scope `$APPDATA/images/**` | `src-tauri/tauri.conf.json:26-29` |
| WebDAV | 备份：整目录上传（`sync/backup.rs:24-31`）；恢复：**按 `asset` 表逐行补图**（`sync/restore.rs:90-103`） | 同左 |

两个直接推论：

1. **「一张图」在库里可能有多行**（同内容多次上传），文件按内容哈希去重 → 任何删除逻辑都必须先判「同 `rel_path` 是否还有别的行」。
2. **`asset` 表是 WebDAV 补图的清单**：删掉行，恢复时就少下这张图；正文里则留下一个永远裂开的 `<img>`。

### 2.4 孤儿文件的来源（已确证）

`purge_novel`（`db/trash.rs:80-83`）执行 `DELETE FROM novel`，`asset` 行随外键 CASCADE 消失，但 `$APPDATA/images` 下的**文件不会被删**。即：**每彻底删除一个作品，就留下它的全部图片文件**。这是 §7 孤立对账功能的直接依据，也解释了为什么「图库」应当具备维护能力。

---

## 三、阶段 0 · 删除 `get_novel` 命令（预估 0.5h）

| 文件 | 位置 | 动作 |
|---|---|---|
| `src-tauri/src/commands/mod.rs` | 30-34 | 删除命令定义；**不动** `db/novel.rs::get_novel` |
| `src-tauri/src/lib.rs` | 50 | 删除 `commands::get_novel,` 一行 |
| `src-tauri/tests/command_key_sweep.rs` | 34、119 | 删除 handler 项与 `("get_novel", …)` 用例 |
| `src-tauri/tests/ipc_argcase.rs` | 43-51 | 命令数断言 88 → 87，并追加一行审计注释（沿用该文件既有的「逐条确认」写法） |
| `Docs/代码优化实施与量测报告.md` | §4.6 | 追加一条修订说明（该报告是历史量测，不重算，只标注后续变化） |

验证：`cargo test -p easy-writing-core --test ipc_command_inventory -- --nocapture` 应打印「后端 87 个命令，4 个未被调用」；`ipc_argcase` 与 `command_key_sweep` 全绿。

---

## 四、阶段 1 · 三个对话命令标注为预留（预估 1h）

「标注为预留」如果只写注释，下一个人仍然无从判断这是有意还是遗漏——本仓已经吃过一次这个亏（`command_key_sweep` 的手工清单失同步）。因此做成**测试里可执行的登记**：

**4.1 后端注释**（`commands/ai_cmd.rs:141/163/184`）：说明该命令服务的是尚未设计的「多会话切换/删除」，数据层已备好，前端当前不需要调用。

**4.2 `ipc_command_inventory.rs` 增加显式预留清单与新用例**

```rust
/// 有意的预留命令：后端先行、UI 尚未设计。列入此表即表示「不是遗漏」。
const RESERVED_COMMANDS: &[&str] = &[
    "append_chat_message",
    "list_chat_sessions",
    "delete_chat_session",
];
```

| 新用例 | 断言 | 防的是什么 |
|---|---|---|
| `reserved_commands_exist_in_backend` | 预留清单里每个名字都在后端命令集中 | 清单写错字 / 命令被误删 |
| `reserved_commands_are_not_yet_wired` | 预留清单里每个名字都**不在**前端调用集中 | 一旦真的接上 UI，必须把它从预留清单摘掉，否则清单开始说谎 |

**4.3 反向报告分组**：`report_backend_commands_never_invoked_by_frontend` 打印时分成「预留（有意保留）」与「待确认（可能遗漏）」两段，让「94% 覆盖率」这个数字始终带上语义。

这样，§1 目标 2 是**被测试守护的**，而不是靠注释维持的。

---

## 五、阶段 2 · 图库 UI（主交付，预估 1.5～2 天）

### 5.1 设计口径：图库回答三个问题

1. 本作品里有哪些图？（列表 + 缩略图 + 统计）
2. 它们还在不在？（文件缺失检测——这是图库最容易被忽略、却最有价值的一点）
3. 怎么把已经入过库的图，再插回正文？（一次插入，而不是重新上传一遍）

**图库的价值不是「相册」，而是「已入库素材的复用与体检」。** 若只做缩略图墙，用户没有理由打开它；插入与体检才是让 `list_image_assets`「真正有用」的两个功能点。

### 5.2 入口决策：放编辑器工具栏，不放左栏页脚

| 方案 | 评估 | 结论 |
|---|---|---|
| 左栏页脚加第 6 个按钮 | 页脚当前已有 设置/留档/作品/回收站/导入 5 键（`settings_panel/mod.rs:171-214`），容器 `w-72`（288px，减 `px-2.5` 后约 268px 可用）。按 `text-[11px]` + 图标 + `px-1.5` 估算，5 键合计已接近该上限，第 6 键会挤压「导入」 | ✗ 需先重构页脚，收益不抵成本 |
| 编辑器工具栏「🖼 图片」右侧加「🗂 图库」 | 与插图动作同一上下文；模态可用 `max-w-4xl` 展示缩略图网格（288px 左栏放网格太窄）；左栏可保留原生 tabs 语义 | ✓ **采用** |

> 页脚宽度是**估算**，实施第一步先量一次实际渲染宽度（开发者工具），若确认还有余量再考虑补一个页脚入口；本方案按「只加工具栏入口」排期。

### 5.3 文件清单

| 文件 | 动作 | 内容 |
|---|---|---|
| `frontend/src/components/editor/asset_gallery.rs` | **新增** | `AssetGalleryModal` 组件 |
| `frontend/src/components/editor/image_upload.rs` | **新增** | 抽取的上传流程（见 5.6） |
| `frontend/src/components/editor/mod.rs` | 改 | 注册两个新模块 |
| `frontend/src/state/mod.rs` | 改 | 新增 `is_asset_gallery_open: RwSignal<bool>`（含 `new()` 初始化） |
| `frontend/src/components/layout/app_shell.rs` | 改 | 挂载 `<AssetGalleryModal app_ctx=app_ctx />` |
| `frontend/src/components/editor/toolbar.rs` | 改 | 加「🗂 图库」按钮；`on_insert_image` 改为调用 `image_upload.rs` |

**不新增任何后端命令**——阶段 2 只消费 `list_image_assets` 与 `get_app_data_dir`，这也是它风险低、可独立交付的原因。

### 5.4 数据流

```
打开（is_open == true）
  └─ Effect：若 current_novel 为空 → 显示「请先选择作品」
     ├─ 并发：get_app_data_dir()  ┐
     └─ 并发：list_image_assets(novel_id) ┘
        └─ 每条 AssetInfo.rel_path → bridge::asset::resolve_asset_url(rel_path, app_dir)
           └─ 网格 <img src=url loading="lazy">
点击「插入到光标处」
  └─ tiptap::insert_image(url, name)
     ├─ app_ctx.save_status.set("dirty")
     └─ 关闭模态（可配置为保持打开以连续插图）
```

切换作品时由 `current_novel` 变更触发重载（与 `settings_panel/loaders.rs` 的「切作品即刷新」约定一致，不要新造一套）。

### 5.5 交互明细与边界

| 区域 | 内容 |
|---|---|
| 头部 | `本作品图片（N）· 合计 X MB`（`size` 求和，本地算，无需后端） |
| 搜索 | 按 `name` 客户端过滤；复用左栏搜索框的视觉样式（`settings_panel/mod.rs:132-157`） |
| 排序 | 默认 `created_at DESC`（后端已排）；可切换「名称 / 大小」，纯客户端 |
| 网格 | 固定比例卡片、`object-cover`、`loading="lazy"`；悬停显示文件名与体积 |
| 详情 | 选中项显示大图 + `name / mime / size / created_at / hash 前 12 位 / rel_path` |
| 动作 | 「插入到光标处」「复制路径」（路径即 `images/<hash>.<ext>`，便于排查与跨端迁移） |
| **失效检测** | `<img on:error>` → 卡片加「⚠ 文件缺失」徽标，顶部汇总「M 条记录找不到文件」。把 WebDAV 恢复补图失败从「正文里默默裂图」变成「图库一眼可见」 |
| 空态 | 「本作品还没有图片」+「上传图片」按钮（复用 5.6 的抽取流程） |
| 降级 | `get_app_data_dir` 失败时仍列出文件名与元信息，预览位显示占位符（`resolve_asset_url` 本身已保证失败时原样返回，不 panic） |
| 大库 | 不分页、不虚拟滚动；记录阈值：**>500 张**时再引入分页（届时后端加 `limit/offset`，见 §9.2） |

### 5.6 顺带消除的重复：抽取上传流程

`toolbar.rs:53-132` 里那段「建 input → 读 base64 → `save_image_asset` → `get_app_data_dir` → `insert_image`」如果图库空态再写一遍，就是第二份实现。本仓已经因「复制两份」付过一次代价（`read_setting/write_setting` 曾在两处各存一份，见优化报告 §4.1）。

因此抽为 `image_upload::pick_and_save_image(app_ctx, on_saved: impl Fn(String, String) + 'static)`：
- 工具栏调用：`on_saved` = 插入光标处；
- 图库调用：`on_saved` = 刷新列表并选中新图。

**关键约束**：行为必须与现状完全一致（同样的 accept、同样的错误文案、同样在失败时不影响正文编辑），这一步是纯搬迁，不是顺手改行为。

---

## 六、阶段 3（可选）· 素材管理命令

阶段 2 让图库「有用」，但还不能重命名/删除。补齐需要新命令，且删除有真实风险，故单独立阶段。

### 6.1 `rename_image_asset(asset_id, name)` —— 低风险

只改 `asset.name`（显示名）。磁盘文件名是内容哈希，**不随重命名变动**，因此不影响正文引用、不影响 WebDAV。

### 6.2 `delete_image_asset` —— 必须先算引用，三道前置

| 前置检查 | SQL 形态 | 不检查的后果 |
|---|---|---|
| 同文件是否还有其他记录 | `SELECT COUNT(*) FROM asset WHERE rel_path = ?1 AND id <> ?2` | 删掉文件，另一条记录变「文件缺失」 |
| 正文是否引用 | `SELECT COUNT(*) FROM chapter WHERE content LIKE '%' \|\| ?1 \|\| '%'`（`?1` 为 hash） | 正文出现永远裂开的图片 |
| 恢复清单影响 | `asset` 行是 WebDAV 补图清单（`sync/restore.rs:90`） | 新设备恢复后少图 |

**处置建议：软删除，而非硬删。**

- `init_db` 增加「为旧库补列」：`ALTER TABLE asset ADD COLUMN deleted_at TEXT`（`db/mod.rs:61-62` 已有同款先例，幂等、失败忽略）。
- `db/trash.rs` 增加 `'asset'` 分支（`restore_trash_item` / `purge_trash_item`），图库列表默认过滤 `deleted_at IS NULL`。
- **物理文件只在「无其他记录且正文无引用」时删除**；否则只摘记录，并在 UI 明确提示「文件仍被引用，已保留」。

理由与 `delete_novel` 完全同源：不可恢复的删除，必然产出两类误解——「以为删了记录、结果正文裂图」和「以为释放了空间、其实没释放」。既然作品删除都做了软删除 + 跨作品回收站，素材删除不该退化成硬删。

**需拍板项**（实施前确认，二选一即可）：

| 选项 | 说明 |
|---|---|
| A（推荐） | 被正文引用时**禁止删除**，只提供「从图库移除记录」，并提示受影响的章节数 |
| B | 允许强制删除，但二次确认里**列出受影响的章节标题** |

---

## 七、阶段 4（可选）· 孤儿与失效对账

§2.4 已确证孤儿文件的来源。本阶段把图库从「本作品相册」升级为「全库素材维护」：

| 命令 | 作用域 | 说明 |
|---|---|---|
| `list_orphan_images` | **全库**（非 novel） | 返回 `$APPDATA/images` 下存在、但 `asset` 表无对应 `rel_path` 的文件 |
| `prune_orphan_images` | 全库 | 删除孤儿文件；**仍需查正文引用**（行被删但正文仍引用的情况真实存在），且排除「仍被其他作品 asset 行引用」的文件 |

**作用域差异必须显式表达**：`list_image_assets` 是「本作品」，对账是「全库」。UI 上放在图库的「维护」折叠区，文案写明「全库范围」，避免用户以为只影响当前作品。

同时可顺带提供「清理失效记录」：`asset` 行存在、文件不在（与阶段 2 的「文件缺失」徽标共用数据）。

---

## 八、测试与验收

### 8.1 守卫更新表（每阶段都要同步，否则数字会失真）

| 守卫 | 阶段 0 | 阶段 1 | 阶段 2 | 阶段 3 |
|---|---|---|---|---|
| `ipc_argcase::every_command_declares_snake_case` 命令数 | 88 → **87** | 87 | 87 | 87 → **88/89** |
| `command_key_sweep` handler + 用例 | −1（`get_novel`） | — | — | +1/+2 |
| `ipc_command_inventory` 预留清单 | — | **+3 条 + 2 个用例** | — | — |
| `ipc_command_inventory` 覆盖数字 | 87 / 83 / 4 未对接 | 同左（分组打印） | 87 / 84 / **3** 未对接 | 88-89 / 86-87 / 3 |

### 8.2 新增后端测试 `src-tauri/tests/asset_gallery.rs`

1. `list_assets_is_novel_scoped_and_newest_first` —— 两个作品互不串台 + 倒序（阶段 2 直接依赖此契约）
2. `save_image_dedupes_file_but_records_each_upload` —— 同字节两次上传：`rel_path` 相同、行数为 2、磁盘文件为 1
3. `purge_novel_leaves_image_file_on_disk` —— **反向复现**孤儿来源（沿用 `novel_trash.rs` / `diff_fixes.rs` 的「把缺陷钉在测试里」写法）
4. `rename_asset_does_not_touch_file`（阶段 3）
5. `delete_asset_keeps_file_when_referenced_by_another_row`（阶段 3）
6. `delete_asset_keeps_file_when_chapter_references_it`（阶段 3）
7. `orphan_scan_detects_files_without_rows`（阶段 4）

### 8.3 现有测试补强

`ipc_argcase::settings_and_assets_write_paths`（:135）**名不副实**：函数体里没有任何一条资产调用，`list_image_assets` 恰好未被覆盖。补一条 `list_image_assets` 调用（它在 MockRuntime 下可注册，见 `command_key_sweep.rs:123` 已有先例）。

### 8.4 手工验收清单（图库，前端无自动化测试设施，只能人工过）

1. 无作品时打开 → 提示选择作品，不报错
2. 空库 → 空态与「上传图片」按钮可用
3. 上传 3 张 → 图库出现 3 张，缩略图可显示
4. 重复上传同一张 → 列表出现 2 条同名记录，磁盘仍只有 1 个文件
5. 搜索框过滤生效
6. 「插入到光标处」→ 正文出现图片、保存状态变 dirty、保存后重开仍在
7. 「复制路径」→ 剪贴板得到 `images/<hash>.<ext>`
8. 手工删掉磁盘文件再打开图库 → 该卡片显示「文件缺失」、顶部汇总计数正确
9. 切换作品 → 图库内容随之切换
10. 深色模式 / 左栏收起 / 右栏收起下布局不错位

### 8.5 验收命令（每个提交前逐条跑，贴输出）

```bash
cargo check --workspace
cargo clippy --workspace --all-targets        # 期望 0 error / 0 warning
cargo test --workspace                        # 全绿
pnpm build:assets && trunk build frontend/index.html
pnpm dev                                      # 手测 §8.4
```

> 注：仓库 CI 仅 `release.yml`（tag 触发），**没有 PR 检查**。因此本轮验收完全依赖上述本地命令，测试输出需逐条留证。

---

## 九、风险与已发现的既有缺陷（不属本轮，但必须在案）

### 9.1 【高】正文图片的 src 是机器绝对路径，与跨设备恢复的设计意图相悖

`bridge/asset.rs:16-20` 把 `appDataDir + "/" + rel_path` 交给 `convertFileSrc`，得到的 URL **内嵌了 URL 编码后的绝对路径**，而 tiptap 把这个 URL 原样存进正文 JSON（`js/src/tiptap-bridge.js:494-496`，无任何反向归一化）。

后果：换设备或换用户目录后从 WebDAV 恢复，正文里的图片引用全部失效——**恰是设计文档要避免的情况**（`Docs/Easy-writing · 实施计划.md:187`：「编辑器中引用的图片统一映射为相对路径……保证 WebDAV 整库还原后跨设备可用」）。

**本轮处置**：图库插入**复用现有机制**（保持一致，不引入第二套 URL 语义），同时把「正文 src 相对路径化 + 加载时解析」单列为下一条独立任务。它牵涉 tiptap 的序列化/反序列化与已有正文的兼容，**不能塞进图库这一轮**；但图库的「文件缺失」徽标会让这个问题第一次变得可见，对后续修复有利。

### 9.2 `list_image_assets` 无分页

一次返回作品全部资产。阈值：**>500 张**时前端渲染会明显变慢，届时给后端加 `limit/offset`（属命令签名变更，需同步 4 处守卫）。当前规模下不做。

### 9.3 `asset` 表无 `deleted_at`

阶段 3 才需要补列；SQLite 对 `DROP COLUMN` 支持有限，因此迁移必须**只加列、不改既有列语义**，回滚时保留列不用即可（可空、无副作用）。

### 9.4 左栏页脚 5 键已接近 `w-72` 宽度上限

本方案因此不把图库入口放页脚（§5.2）。若后续仍要加入口，应先重构页脚布局，而不是继续加第 6 键。

---

## 十、工作量与提交计划

| 提交 | 内容 | 预估 | 提交信息（建议） | 依赖 |
|---|---|---|---|---|
| 1 | 阶段 0 + 阶段 1：删 `get_novel`、预留登记、守卫更新 | 1.5h | `chore(ipc): 删除未对接的 get_novel 命令并将对话命令登记为预留` | 无 |
| 2 | 阶段 2：图库 UI + 上传流程去重 | 1.5～2d | `feat(gallery): 接入图库 UI，使 list_image_assets 具备插入与失效体检能力` | 提交 1 |
| 3 | 阶段 3：重命名 / 软删除 / 引用检查（可选，需先拍板 6.2 的 A/B） | 1d | `feat(asset): 素材重命名与软删除（含正文引用检查）` | 提交 2 |
| 4 | 阶段 4：孤儿与失效对账（可选） | 0.5～1d | `feat(asset): 图片文件对账与孤儿清理` | 提交 3 |

**建议节奏**：提交 1 与提交 2 分开。提交 1 让命令数、覆盖率、预留语义当场变干净，半天内可交付且零 UI 风险；图库（提交 2）独立成一个功能提交，便于单独回归与回滚。

每个提交按 `Docs/提交流程.md` 的双远程流程（主目录提交 → `.cnb-worktree` 内 `cherry-pick --no-commit` 后以 CNB 身份重新提交），提交前跑 §8.5 全套命令。

---

## 十一、回滚方案

| 提交 | 回滚方式 | 数据影响 |
|---|---|---|
| 1 | `git revert`，纯删除 + 注释 + 测试 | 无 |
| 2 | 删除 2 个新文件，还原 `state/mod.rs` / `app_shell.rs` / `toolbar.rs` / `editor/mod.rs` 四处 | 无（不碰数据库） |
| 3 | 保留 `deleted_at` 列不使用即可（只加列迁移） | 无（列可空） |
| 4 | `git revert` | 孤儿清理是显式用户操作，不自动执行 |

---

## 十二、结论

1. `get_novel` 的「命令死、db 函数活」是本次最需要说清的一点——删错一层会连带打断 `create_novel` 与 AI 提炼链路。
2. 三个对话命令的「预留」如果不落成测试里的登记，就只是又一次注释，迟早重演 `command_key_sweep` 失同步的老问题。
3. 图库要「真正有用」，关键不在缩略图墙，而在**插回正文**与**失效体检**这两件事；前者复用已有上传/插入链路并顺带消除一处重复，后者让长期潜伏的图片与恢复问题第一次可见。
4. 随之暴露的两条既有缺陷（§9.1 绝对路径、§2.4 孤儿文件）本轮不在图库内顺手修——它们的修复边界远大于图库，混在一起会让这一轮的回滚粒度变得不可控。
