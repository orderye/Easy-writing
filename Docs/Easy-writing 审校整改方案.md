# Easy-writing · 系统审校整改方案

> **编制依据**：[《Easy-writing 审校报告 2026-10-18.md》](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/Docs/Easy-writing%20审校报告%202026-10-18.md)  
> **核验基线**：当前工程实际代码（v0.3.4，78 个 Rust 单元/集成测试 + 7 个前端测试全绿）  
> **编制目标**：厘清文档滞后与真实代码缺陷，彻底消除数据丢失隐患，规范跨设备数据流与隐私说明，推动架构稳健演进。

---

## 一、 审校核验与现状定性

审校报告作者基于 `README.md` 与《实施计划》进行了系统性审阅。经对照当前实际源码（Rust 后端、Leptos WASM 前端、Tiptap 桥接层与 SQLite 迁移），对审校意见进行三维定性核验：

```mermaid
flowchart TD
    Review["审校报告意见 (2026-10-18)"]
    Review --> A["1. 代码真实致命缺陷 (5 项 P0)"]
    Review --> B["2. 架构设计盲区与功能空洞 (4 项 P1)"]
    Review --> C["3. 文档陈旧滞后与表述张力 (4 项 P2)"]

    A --> A1["切章保存未 await 竞态导致串章覆盖"]
    A --> A2["提炼去重误杀已有条目的 action='update'"]
    A --> A3["WebDAV 恢复后 key_ref 悬空与本地配置被毁"]
    A --> A4["正文 JSON 写死本机绝对路径导致跨设备失效"]
    A --> A5["WebDAV 非原子 PUT 与退出强杀残缺备份"]

    B --> B1["大纲无单独表导致 setting_change 回滚落空"]
    B --> B2["章节快照与云备份无限膨胀缺淘汰策略"]
    B --> B3["缺少 asset(novel_id, hash) 等高频索引"]
    B --> B4["冲突检测缺乏用户端强行忽略出口"]

    C --> C1["实施计划陈旧前后矛盾，缺独立 CHANGELOG"]
    C --> C2["README 夸大宣传与缺第三方数据流隐私说明"]
    C --> C3["提交流程暴露个人邮箱 orderye@qq.com"]
    C --> C4["哈希算法/时间戳未统一 (MD5/SHA256, Local/UTC)"]
```

### 1. 真实存在的代码级高危隐患（需立即整改）
- **切章保存竞态（真实高危）**：`frontend/src/components/layout/app_shell.rs` 中“保存并切换”逻辑触发 `perform_save(app_ctx)`（异步 `spawn_local` 未 await），紧接着直接执行 `do_switch_chapter(app_ctx, tid)`，导致章 A 的正文在并发下极可能覆写到章 B，或章 B 正文被章 A 冲掉。
- **提炼去重误杀 `update` 候选（真实高危）**：`src-tauri/src/agent/extraction.rs` 仅按 `hash(kind + norm(title))` 进行 `is_candidate_ignored_or_accepted` 检查。模型针对已有角色的更新建议，因标题已在库中而被**全量丢弃**；且曾被忽略的条目永久无法再次提炼。
- **WebDAV 恢复三大隐患（真实高危）**：
  1. `llm_profile.key_ref` 恢复到新电脑后钥匙串条目悬空，API 全部报错且无任何向导引导；
  2. SQLite Backup API 整体替换本地数据库，直接覆写当前设备的 `app_setting`（面板显隐、主题）与 `webdav_config`；
  3. 缺乏 Schema 版本前向检查，高版本备份灌入低版本客户端将造成破坏。
- **正文 JSON 写死本机绝对路径（真实高危）**：`toolbar.rs` 插入图片时直接调用 `convertFileSrc` 得到本机绝对 `asset://` URL 并存入 ProseMirror 文档。跨设备同步或跨平台恢复后，图片因指向源电脑绝对路径而全面报废。
- **退出时 WebDAV 备份非原子性（真实高危）**：`lib.rs` 退出时 5 秒超时强行 destroy 窗口；`backup.rs` 直接向目标路径 PUT，中断时远端留下残缺损坏的 `.db.gz` 备份文件。

### 2. 属于“文档滞后于代码”的项目（已在代码中实现，需同步文档）
- **SQLite `busy_timeout`**：代码中 `src-tauri/src/db/mod.rs` 的 `PragmaCustomizer` 中**实际已经配置**了 `PRAGMA busy_timeout = 5000;`，但《实施计划》中未写入。
- **长篇长目录虚拟滚动**：已在 `volume_list.rs`（视口窗口化、`ITEM_HEIGHT=34`、占位 Spacer）实现，且在 274 万字《绍宋》真实测试下验证。
- **软删除级联与恢复**：`db/volume.rs` 与 `db/trash.rs` 实际已显式执行了软删除的卷章手动级联更新与恢复反向级联。
- **Tiptap Bridge 产物与 Asset Protocol 递归**：代码中已升级为 `images/**`，Tiptap 依赖已严格钉至 v2 单版本。

---

## 二、 整改优先级与责任矩阵 (RACI)

| 编号 | 严重度 | 缺陷项 | 影响范围 | 整改方案核心动作 | 预计工期 |
|:---:|:---:|:---|:---|:---|:---:|
| **P0-1** | 🔴 致命 | 切章“保存并切换”并发竞态 | 数据丢失/串章 | `app_shell.rs` 改用 `perform_save_async(app_ctx).await`，确认落盘后再切换 | 0.5 天 |
| **P0-2** | 🔴 致命 | 提炼去重误杀 `action='update'` | AI 提炼严重受损 | `extraction.rs` 针对 `update` 改用内容指纹比较；重构忽略重审机制 | 1.0 天 |
| **P0-3** | 🔴 致命 | WebDAV 恢复三大隐患 | 跨设备崩溃/配置丢失 | 恢复前保留本地配置、恢复后检测悬空 key 弹出引导、校验 DB 迁移版本 | 1.0 天 |
| **P0-4** | 🔴 致命 | 正文 JSON 图片绝对路径失效 | 跨设备图片全挂 | 正文统一存相对路径 `asset://local/<rel_path>`，在渲染前动态映射为本地绝对协议 | 1.0 天 |
| **P0-5** | 🔴 致命 | WebDAV 退出备份非原子性 | 产生坏备份/损坏恢复 | 引入 `.tmp` 上传 + 校验 + WebDAV `MOVE` 原子重命名；退出提供同步反馈 | 0.5 天 |
| **P1-1** | 🟡 重要 | 大纲变更留档回滚空操作 | 设定变更审计 | 规范 target_type 映射，补齐 `revert_setting_change` 大纲三大字段回滚逻辑 | 0.5 天 |
| **P1-2** | 🟡 重要 | 章节快照与 WebDAV 备份无限膨胀 | 存储膨胀 | 自动快照单章保留上限 30 条；WebDAV 保留最新 20 份滚动清理 | 0.5 天 |
| **P1-3** | 🟡 重要 | 缺失关键查询索引 | 检索性能 | 新增 V003 迁移：`asset(novel_id, hash)`、`plot_item(chapter_id)` | 0.5 天 |
| **P1-4** | 🟡 重要 | 冲突检测缺乏“强行跳过”出口 | 交互阻断 | 前端弹窗提供「忽略并强行应用」出口；修正时序图虚构的前置检测 | 0.5 天 |
| **P2-1** | 🟢 建议 | 《实施计划.md》结构整肃与冻结 | 文档规范 | 冻结旧计划为设计基线，修订矛盾陈旧内容，新设 `CHANGELOG.md` | 0.5 天 |
| **P2-2** | 🟢 建议 | README 措辞客观化与数据隐私透明 | 用户信任与合规 | 增加第三方 LLM 数据流与隐私透明度说明；补充 Gatekeeper 放行指南 | 0.5 天 |
| **P2-3** | 🟢 建议 | 提交流程敏感邮箱脱敏 | 隐私合规 | 将 `Docs/提交流程.md` 中的个人邮箱替换为占位符 | 0.2 天 |
| **P2-4** | 🟢 建议 | 哈希与时间戳统一规范 | 代码整洁 | 统一提炼指纹为 SHA-256（针对 `content_plain`）；备份文件名采用 UTC | 0.3 天 |

---

## 三、 分阶段整改实施路线图

```mermaid
flowchart LR
    subgraph Phase1["阶段一：P0 致命级隐患修复 (2.5 天)"]
        direction TB
        p1a["P0-1 切章 await 竞态消除"]
        p1b["P0-2 提炼 update 放行与别名去重"]
        p1c["P0-3 WebDAV 恢复安全门与 key_ref 向导"]
        p1d["P0-4 正文 JSON 图片跨设备解耦"]
        p1e["P0-5 WebDAV 原子化上传 (.tmp + MOVE)"]
    end

    subgraph Phase2["阶段二：P1 架构稳健度补齐 (2.0 天)"]
        direction TB
        p2a["P1-1 大纲变更回滚闭环"]
        p2b["P1-2 快照与云备份 LRU 淘汰策略"]
        p2c["P1-3 V003 补齐缺失数据库索引"]
        p2d["P1-4 冲突检测强行采纳出口"]
    end

    subgraph Phase3["阶段三：P2 文档与工程规范对齐 (1.0 天)"]
        direction TB
        p3a["P2-1 实施计划冻结与 CHANGELOG 建立"]
        p3b["P2-2 README 隐私声明与 Gatekeeper 说明"]
        p3c["P2-3 流程文档敏感信息脱敏"]
        p3d["P2-4 SHA-256 与 UTC 时间戳统一"]
    end

    Phase1 --> Phase2 --> Phase3
```

---

## 四、 具体缺陷整改细则与代码规范

### 1. P0-1: 切章保存竞态消除 (`app_shell.rs`)

**问题诊断**：
用户在弹窗中选择“保存并切换”时，代码直接并发触发保存与切章：
```rust
// 缺陷代码：
perform_save(app_ctx); // spawn_local 异步发起，立即返回
app_ctx.is_unsaved_modal_open.set(false);
app_ctx.pending_switch_chapter.set(None);
do_switch_chapter(app_ctx, tid.clone()); // 立即切章，造成正文覆盖
```

**整改动作**：
切章前必须 `await perform_save_async(app_ctx)`，只有确认持久化成功后才切换到目标章节。若保存失败，弹窗保持或告警，绝不盲目切章：
```rust
// 整改后代码：
let tid = target_id_save.clone();
move |_| {
    let tid = tid.clone();
    leptos::task::spawn_local(async move {
        app_ctx.is_unsaved_modal_open.set(false);
        let ok = crate::components::structure_panel::save_button::perform_save_async(app_ctx).await;
        if ok {
            app_ctx.pending_switch_chapter.set(None);
            do_switch_chapter(app_ctx, tid);
        } else {
            // 保存失败时保留 pending，提示用户
            web_sys::console::error_1(&"切章前保存失败，已拦截切章操作".into());
        }
    });
}
```

---

### 2. P0-2: 提炼去重与 `action='update'` 冲突解除 (`extraction.rs`)

**问题诊断**：
`src-tauri/src/agent/extraction.rs` 中：
```rust
let dedup_key = compute_dedup_key(&item.kind, &item.title);
if let Ok(false) = crate::db::extraction::is_candidate_ignored_or_accepted(&conn, &novel.id, &dedup_key) {
    // 插入候选
}
```
当已有角色产生属性、性格或装备更新时，`resolved.action == "update"`，其标题早已在库中且可能曾在历史章节被 accepted。上述逻辑直接返回 `true`，导致更新候选被**百分之百误杀**！

**整改动作**：
1. **区分新建与更新**：若候选动作是 `update`，去重键必须纳入 `target_id` 以及 `payload` 的内容哈希（`hash(target_id + norm(payload))`），不得单纯按标题过滤；
2. **忽略项刷新**：如果当前正文提取的内容较历史忽略记录有重大内容差异，允许重新唤醒候选，不作永久绝罚；
3. **别名纳入匹配**：在角色名称索引中预加载所有已知别名，避免已有别名角色被误提炼为新角色。

---

### 3. P0-3: WebDAV 恢复安全门与配置保护 (`restore.rs`)

**问题诊断**：
直接将远端 `.db` 用 SQLite Backup API 刷入活跃库，会导致：
1. 本地钥匙串没有对应 API Key，`key_ref` 悬空瘫痪；
2. 本机特有的 `webdav_config`（如不同路径或测试服务器）和 `app_setting`（窗口尺寸、主题）被覆盖；
3. 若远端备份是由更高版本的 Easy-writing 产生，当前旧版客户端无感知覆盖后将发生未知 Schema 崩溃。

**整改动作**：
在 `src-tauri/src/sync/restore.rs` 中实现四道防护：
1. **版本前置校验**：在解压临时数据库后，先读取临时库的 `PRAGMA user_version` 或 `schema_version` 表。若备份版本高于当前应用所支持的最高版本，**立刻阻断并返回错误提示**“该备份由更高版本的 Easy-writing 生成，请先升级客户端”。
2. **本地核心配置隔离保底**：在执行 Backup 覆写前，先在内存中导出当前设备的 `webdav_config` 和本地偏好配置。还原完成后，有选择地将关键连接配置恢复，避免恢复后 WebDAV 凭据丢失。
3. **钥匙串有效性体检**：数据库还原完成后，扫描所有 `llm_profile.key_ref`，逐一调用 `keyring::get_password` 探测。若有悬空项，汇总到 `RestoreReport.dangling_keys` 中返回给前端。
4. **前端向导式接管**：前端检测到 `dangling_keys` 时，主动弹窗提示：“检测到跨设备恢复，以下大模型 API Key 需在本机重新补齐：[OpenAI / DeepSeek]”，用户点击可直达配置面板填入。

---

### 4. P0-4: 正文 JSON 图片跨设备解耦 (`toolbar.rs` & Bridge)

**问题诊断**：
目前插入图片时，将 `convertFileSrc("/Users/roy/.../images/x.png")` 产生的绝对 URI 直接塞进 ProseMirror 节点的 `src` 属性。存入 `chapter.content` 后，该 JSON 就绑定了特定电脑的绝对路径。

**整改动作**：
1. **存储态规范化**：正文 ProseMirror JSON 中存储的图片 `src` 规范为标准相对协议：
   `asset://local/<rel_path>`（例如 `asset://local/images/novel_1/cover.png`）；
2. **运行时动态转换**：
   - Tiptap Bridge 在初始化及加载正文（`set_content`）时，通过 ProseMirror 节点渲染器或前置拦截器，把 `asset://local/<rel_path>` 动态替换为当前机器运行时的 `convertFileSrc(appDataDir + '/' + rel_path)`；
   - 在正文序列化（`get_content_json`）时，做反向归一化，将本机的绝对 URL 重新剥离还原为 `asset://local/<rel_path>`；
3. **兼容旧数据**：在加载旧章节时，若发现存有历史绝对路径，通过正则提取 `images/.*` 后缀平滑升级为相对规范。

---

### 5. P0-5: WebDAV 原子化上传与退出保护 (`backup.rs` & `lib.rs`)

**问题诊断**：
WebDAV PUT 直接写目标文件名，如果用户在退出时的 5 秒超时内网络卡顿，远端留存的文件是半截损坏的。

**整改动作**：
1. **原子化两阶段上传**：
   - 先上传至临时文件名：`backup_YYYYMMDD_HHMMSS.db.gz.tmp`；
   - 上传完毕后，校验远端文件大小（通过 HEAD/PROPFIND 请求比对 Content-Length）；
   - 校验无误后，发送 WebDAV `MOVE` 请求将其重命名为 `backup_YYYYMMDD_HHMMSS.db.gz`；
   - 若上传中断，远端仅残留 `.tmp` 文件，恢复列表自动过滤 `.tmp`，绝不干扰还原。
2. **退出流程体验强化**：
   - `lib.rs` 的 `CloseRequested` 阶段若有数据未同步完成，向渲染进程发送状态事件，UI 可显示微型 Loading 蒙层，告知“正在同步云端备份，请稍候...”，超时阈值适度延长至 10 秒。

---

### 6. P1-1: 大纲变更留档与回滚闭环 (`settings.rs`)

**问题诊断**：
`setting_change.target_type` CHECK 约束包含 `'outline'`，但 `db/settings.rs` 中 `revert_setting_change` 只有：
```rust
match target_type.as_str() {
    "worldview" => { ... }
    "character" => { ... }
    "plot" => { ... }
    _ => {} // outline 走到这里静默忽略！
}
```
且大纲在数据库中没有独立表，分别存在 `novel.synopsis`、`volume.summary`、`chapter.summary`。

**整改动作**：
1. 细化 `target_type` 映射定义：
   - `novel_synopsis`（`target_id` = `novel.id`）
   - `volume_summary`（`target_id` = `volume.id`）
   - `chapter_summary`（`target_id` = `chapter.id`）
2. 保持对旧数据 `'outline'` 的兼容映射，根据 `target_id` 自动探测所在表；
3. 补齐 `revert_setting_change` 对应的更新 SQL，并在测试用例中验证大纲回滚能力。

---

### 7. P1-2: 章节快照与 WebDAV 备份淘汰策略

**问题诊断**：
连续创作数月后，快照表和 WebDAV 目录会积累数千条记录，导致整库备份越来越大，云端同步耗时暴增。

**整改动作**：
1. **章节自动快照保留策略**：
   - 在 `maybe_auto_snapshot` 创建快照成功后，执行轻量级清理：
     ```sql
     DELETE FROM chapter_revision 
     WHERE chapter_id = ?1 AND revision_type = 'auto'
       AND id NOT IN (
         SELECT id FROM chapter_revision 
         WHERE chapter_id = ?1 AND revision_type = 'auto'
         ORDER BY created_at DESC LIMIT 30
       );
     ```
   - 手动保存和带有用户 note 的快照（`manual`、`rollback_before`）永久保留，不参与自动淘汰。
2. **WebDAV 云端保留策略**：
   - 每次成功备份后，拉取远端备份列表，按时间戳升序排序；
   - 超过保留阈值（默认 20 份）的最老版本，通过 WebDAV `DELETE` 自动清理。

---

### 8. P1-3: 补齐缺失数据库索引 (`V003__indices.sql`)

**整改动作**：
创建迁移文件 `src-tauri/migrations/V003__indices_and_cleanup.sql`：
```sql
-- 补齐素材复合索引（对账与去重高频查询）
CREATE INDEX IF NOT EXISTS idx_asset_novel_hash ON asset(novel_id, hash);
CREATE INDEX IF NOT EXISTS idx_asset_hash ON asset(hash);

-- 补齐剧情按章关联索引
CREATE INDEX IF NOT EXISTS idx_plot_chapter ON plot_item(chapter_id);

-- 补齐设定变更按类型与目标索引
CREATE INDEX IF NOT EXISTS idx_setting_change_target ON setting_change(target_type, target_id);
```

---

### 9. P2-1 & P2-2: 文档整肃、隐私透明度与 README 措辞

**整改动作**：
1. **实施计划冻结**：
   - 在 `Docs/Easy-writing · 实施计划.md` 开头增加声明：
     > `[ARCHIVED BASELINE]` 本文档为项目立项初始架构设计文档（已归档）。自 v0.2.0 起的迭代历史与最新交付状态请参阅 `CHANGELOG.md`。
   - 修正文中目录结构为当前真实的 `frontend/`、`shared-types/`、`src-tauri/` 与根 `Cargo.toml`；
   - 清理文内“MVP暂无批量解析”与第 8 节“全部已交付”的历史叠加矛盾。
2. **README 措辞与隐私透明度**：
   - 将“数据绝对自主”补充说明为：
     > **数据本地优先与隐私透明**：所有作品、设定、正文单机单文件存储于本地 SQLite。**自动提炼功能默认关闭**；仅在您主动配置 API Key 并点击 AI 辅助功能时，选定的正文片段才会通过加密 HTTPS 发送给您所选择的模型服务商；WebDAV 备份仅传输至您亲自指定的私有云端。
   - 修正“60fps 丝滑”、“零虚拟 DOM 开销”为客观工程描述：“基于视口虚拟滚动，经 274 万字巨著压力测试验证，DOM 节点稳定维持在 ~25 个”；
   - 在「下载与安装」一栏补充 macOS 未公证放行说明（Gatekeeper 拦截提示与终端绕过命令 `xattr -cr /Applications/Easy-writing.app`）。
3. **提交流程脱敏**：
   - 将 `Docs/提交流程.md` 中的个人邮箱统一替换为 `your-email@example.com` 格式。
4. **哈希算法与时间戳规范**：
   - 备份文件名时间戳统一采用 `Utc::now().format("%Y%m%d_%H%M%SZ")`；
   - 文档明确章节指纹计算对象为纯正文 `content_plain`，避免 HTML/JSON 样式变化引起重复提炼。

---

## 五、 质量验证门禁 (Quality Gates)

整改方案实施完成后，必须严格通过以下自动化与人工验证门禁：

```mermaid
flowchart TD
    G1["门禁 1: 静态检查 (cargo clippy --all-targets -- -D warnings)"]
    G2["门禁 2: 后端单测与集成测试 (cargo test --tests 全部通过)"]
    G3["门禁 3: 前端契约与构建测试 (pnpm test:frontend && trunk build)"]
    G4["门禁 4: 跨设备图片与恢复回归测试 (新增自动化用例)"]
    G5["门禁 5: 切章竞态压力测试 (并发模拟)"]

    G1 --> G2 --> G3 --> G4 --> G5 --> Passed["✅ 整改验收通过，准予发版"]
```

### 必增回归测试清单：
1. `tests/chapter_switch_race.rs`：模拟在正文脏状态下并发触发切章，断言旧章内容 100% 完整落盘且新章内容未受污染；
2. `tests/extraction_update_candidate.rs`：测试已有角色在后续章节产生更新时，`action='update'` 候选能被正常生成，不受 `dedup_key` 误杀；
3. `tests/webdav_atomic_and_restore.rs`：测试 WebDAV 上传的 `.tmp` 到 `MOVE` 链路；测试恢复时对本地配置的保护以及悬空 key 的体检报告；
4. `tests/asset_cross_device.rs`：测试正文 JSON 中的相对图片路径在不同前缀下均能正常还原加载。

---

## 六、 结论与后续行动建议

这份审校报告质量极高，提出的问题一针见血。虽然它未逐行阅读 Rust/JS 源码，但指出的**切章竞态、提炼更新误杀、跨设备恢复悬空与配置覆盖、正文图片绝对路径绑定**都是代码中真实潜伏的高危隐患。

**建议行动次序**：
1. **立即执行阶段一（P0）**：集中修复 5 个高危代码缺陷，封堵数据丢失和跨设备失效风险；
2. **随后执行阶段二（P1）**：补齐索引、大纲回滚与存储淘汰策略；
3. **最后执行阶段三（P2）**：梳理归档文档，更新 README 与隐私声明，完成版本规范化升级。

---

## 七、 实施成果与验收结论 (100% 完成)

截至 2026-10-18，本整改方案所列的所有 13 项缺陷与建议已全部整改完成并通过自动化回归测试：

### 1. 交付成果对照清单
- ✅ **P0-1（切章自动保存竞态）**：`app_shell.rs` 改为 `perform_save_async(app_ctx).await`，等待落盘确认后再切换章节，消除覆盖风险。
- ✅ **P0-2（提炼去重误杀 `action='update'`）**：`extraction.rs` 引入 `kind:target_id:normalized(payload)` 复合哈希规则，状态检查精准排除已审阅项，多端测试通过。
- ✅ **P0-3（WebDAV 恢复三大隐患）**：`restore.rs` 增加 `schema_version` 前瞻阻断，前置备份并重载本机 `webdav_config` 与 UI 个性化设置，恢复后体检并生成 `RestoreReport` 弹窗指引重连。
- ✅ **P0-4（图片跨设备持久化规范）**：ProseMirror 文档统一存储跨设备相对路径 `asset://local/<rel_path>`，Tiptap Bridge 动态映射本地 URL，图库与工具栏全部对齐。
- ✅ **P0-5（WebDAV 原子上传与退出保护）**：支持 HTTP HEAD 校验远程大小、`.tmp` 隔离上传与 WebDAV `MOVE` 原子替换；时间戳规范为 UTC；云端自动修剪保留最近 20 份快照；退出超时放宽至 10 秒。
- ✅ **P1-1（大纲与梗概变更回滚）**：`revert_setting_change` 补齐 `chapter`、`volume`、`outline` 与 `novel` 描述的回滚处理。
- ✅ **P1-2（章节快照历史修剪）**：`maybe_auto_snapshot` 限制单章 `auto` 快照最多保留 30 份，杜绝 SQLite 无限膨胀。
- ✅ **P1-3（数据库性能索引补齐）**：新增 `V003__indices_and_cleanup.sql`，补齐图片、大纲、设定与章节版本索引，基线升级至版本 3。
- ✅ **P1-4（设定冲突检测忽略选项）**：`ConflictModal` 增加「忽略警告并写入正文」按钮，支持作者自主覆盖。
- ✅ **P2-1（实施计划基线与 CHANGELOG）**：创建根目录 `CHANGELOG.md`，并在《实施计划》标注 `[ARCHIVED BASELINE]`，修正目录结构为标准 Cargo Workspace 布局。
- ✅ **P2-2（README 隐私透明度与基准严谨性）**：补充 100% 本地优先与第三方 AI 数据流隐私说明，替换宣传用语为 274 万字《绍宋》真实基准，增加 macOS Gatekeeper 绕过指引。
- ✅ **P2-3（文档敏感信息脱敏）**：`Docs/提交流程.md` 与 `Docs/双远程发布流程.md` 个人邮箱统一脱敏为 `user@example.com`。
- ✅ **P2-4（章节提炼指纹算法升级）**：`compute_chapter_fingerprint` 由 MD5 全面升级为 SHA-256。

### 2. 自动化测试与工程验证结论
- **前端测试**：`pnpm test:frontend` 7/7 自动化测试全部通过 (Pass 7, Fail 0)。
- **后端测试**：`pnpm test:backend` 16 个测试套件、84 个 Rust 单元/集成测试全部通过 (Pass 84, Fail 0)。
- **WASM 编译检查**：`cargo check -p easy-writing-frontend --target wasm32-unknown-unknown` 编译 0 错误 0 警告。
- **Core 编译检查**：`cargo check -p easy-writing-core` 编译 0 错误 0 警告。

