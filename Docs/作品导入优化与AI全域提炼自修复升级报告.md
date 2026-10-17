# 作品导入优化与 AI 全域提炼自修复升级报告

> **编制时间**：2026-10-17  
> **涉及模块**：文件导入解包引擎 (`src-tauri/src/import/`)、AI 提炼与自修复引擎 (`src-tauri/src/agent/extraction.rs`, `src-tauri/src/commands/ai_cmd.rs`)、数据模型与持久层 (`shared-types`, `src-tauri/src/db/`)、前端交互与审核看板 (`frontend/src/components/`)、自动化测试矩阵 (`src-tauri/tests/`)。

---

## 一、 工作背景与解决的核心痛点

在测试百万字历史巨著《绍宋》（9.4MB，8 卷，473 章，274 万字）真实导入以及深度接入大模型 API 进行 AI 辅助创作的过程中，原系统暴露出了以下结构性缺陷与鲁棒性瓶颈：

1. **导入架构职责耦合严重**：原 `import/mod.rs` 单文件堆砌了纯文本正则、EPUB 解包、数据库事务、字数统计与清洗规则，EPUB 与 TXT/Markdown 存在规则重复与分卷分章误判。
2. **长篇大作导入体验缺失**：400+ 章节在预览界面直接平铺展开，DOM 结构过长造成操作卡顿；用户无法在入库前修改带有网络抓取后缀的书名。
3. **AI 提炼与小说工作台全域割裂**：
   - 提炼候选池未独立记录标题，入库时写死为硬编码（如“提炼新角色”、“提炼条目”、“提炼事件”）；
   - 章纲被直接丢弃未更新至章节；
   - 提炼出的专有名词与人物无法联动至**词库**与**编辑器补全**；
   - 角色卡 6 维立体解构（ExplodedProfile）未能与提炼闭环，更新时易粗暴覆盖作者自定义数据；
   - 角色之间的关系未被提取，导致**关系管理**与**星云爆炸图（Nebula Graph）**无法自动成网。
4. **大模型输出脆弱，极易崩溃或数据偏移**：
   - 当大模型因达到 `max_tokens` 发生尾部截断时，`serde_json` 直接报 EOF 失败，导致已生成的有效候选全数丢失；
   - 开源/兼容模型常用的字段别名（如 `name` 代替 `title`、`content` 代替 `payload`、`quote` 代替 `evidence`）造成数据丢失；
   - 复杂的 Markdown 代码块包裹、闲聊前缀、行内注释及尾随逗号导致 JSON 解析失败；
   - 虚构不存在的 `target_id` 或跨类别引用导致外键约束异常或数据重复冗余。

---

## 二、 作品导入架构解耦与长篇交互升级

### 1. 后端模块化解耦架构 (`src-tauri/src/import/`)

将原先庞杂的导入逻辑重构为单一职责模块：

```
src-tauri/src/import/
├── mod.rs        # [Module Facade] 负责向 Tauri 注册对外 IPC 命令，精简 75%
├── common.rs     # [Domain Logic] 全局共享领域规则（LazyLock 预编译正则、字数统计、HTML 反转义与清洗）
├── epub.rs       # [Format Parser] 专职 EPUB/NCX/Spine/Toc 提取，对接 common 规则
├── text.rs       # [Format Parser] 专职 TXT / Markdown 流式解析与智能切分
└── db.rs         # [Persistence]   专职数据库事务持久化，精准维护卷/章 sort_order
```

### 2. 领域规则与边界判定修复
- **CJK 中文字符单词边界失效修复**：Rust `\b` 遇到连续汉字无法匹配，已重构正则模式以适应无空格中文长句与标题。
- **“番外卷”与“番外章节”冲突消除**：严格限定卷模式（`番外卷|番外篇`），避免将正文中的番外章节（如 `番外一 东京梦华`）误判为空分卷。
- **文件名垃圾清洗**：自动去除常见的资源站后缀（如 `(z-library.sk, 1lib.sk)` 等）。
- **统一字数统计与排版**：中英文混合字数精准统计，自动过滤 HTML 标签与多余空行，封装标准段落 `<p>...</p>`。

### 3. 前端大作品预览体验重构 (`import_modal.rs`)
- **分卷手风琴折叠面板 (Volume Accordion)**：
  - 针对多卷长篇自动折叠，分卷标题栏呈现折叠状态、卷名、**本卷章节数**以及**本卷字数汇总统计**；
  - 顶部常驻“**展开全部**”与“**收起全部**”快速切换按钮。
- **书名即时修正**：Step 2 顶部支持实时编辑书名，入库前即可将从网络下载带前缀/后缀的书名快速修改为正式书名。

---

## 三、 AI 全域提炼体系重构（八大领域联动）

提炼流程打通了小说全域数据的有机闭环，采纳即刻落地：

```
                    ┌── 设定库 (Worldview) ── 自动提取地理/势力/时代背景
                    ├── 词库 (Novel Lexicon) ── 专有名词/角色名实时注入，驱动编辑器自动补全
                    ├── 角色库 (Characters) ── 姓名/外号/别名/阵营/重要度
AI 提炼受限并发引擎 ──┼── 角色卡 6 维爆炸图 ── 灵魂核/外貌皮囊/性格言行/专属器物/死穴秘密
(无阻塞异步并发)    ├── 星云爆炸图 & 关系管理 ── 自动推断并写入 character_relation 连线
                    ├── 剧情与伏笔 (Plot) ── 冲突记录、伏笔埋设 (planted) 与回收 (resolved)
                    └── 大纲 (Chapter Summary) ── 核心梗概实时更新 chapter.summary，同步总卷章纲
```

| 业务领域 | 提炼内容与识别机制 | 采纳落地与全域联动效果 |
| :--- | :--- | :--- |
| **设定 (Worldview)** | 时代背景、地理地点、宗门势力、运转规则 | 保存至 `worldview_entry`；自动同步登记至 `novel_lexicon` 词库 |
| **词库 (Lexicon)** | 专有名词、功法典籍、官制兵制 | 自动入库 `novel_lexicon`，编辑器打字时毫秒级自动弹出补全建议 |
| **角色 (Character)** | 角色姓名、外号、别名、阵营归属、重要度 | 保存至 `character` 表；别名自动进入匹配索引消除重复创建 |
| **角色卡 (Exploded Profile)** | 6 维全息解构：灵魂核、外貌、性格口癖、器物资产、死穴弱点 | 更新时支持**无损合并自修复**，填充空缺并以版本线保留自定义修改 |
| **关系管理 (Relations)** | 人物之间的交互与纽带（君臣、同门、宿敌、眷属等） | 自动识别正文交互，存入 `character_relation` 表，记录双向关联与证据 |
| **星云爆炸图 (Nebula Graph)** | 角色节点群与关系引力网 | 角色与关系入库后，`CharacterGraphView` 画布实时渲染粒子群与连线 |
| **剧情与伏笔 (Plot)** | 核心戏剧冲突事件、新埋设伏笔或已知伏笔回收 | 分类标记 `mainline/conflict/foreshadow`，状态标记 `planted/resolved` |
| **大纲章纲 (Outline)** | 提取具备因果脉络的 1-2 句核心章节梗概 | 直接回填 `chapter.summary`，大纲视图与卷章目录树实时同步呈现 |

---

## 四、 AI 提炼 6 层自愈引擎设计 (`self_heal_and_parse_candidates`)

针对 API 输出中常见的各类异常与偏移，实现了完整的自修复流水线：

```
[原始 LLM 返回文本]
       │
       ▼
 [1. Markdown & 闲聊剥离] ── 剔除 ```json 代码块及前置礼貌用语/后置总结闲聊
       │
       ▼
 [2. 数组与单对象自修复] ── 自动兼容单对象 `{}`，动态包装为标准 `[{}]` 数组
       │
       ▼
 [3. 语法杂质自修复] ──── 字符串感知型去除行注释 `//`；智能去除换行尾随逗号 `,\n}`
       │
       ▼
 [4. Token 截断自修复] ─── 逆向搜寻最后一个完整闭合的 `}`，补全 `]`，抢救全部有效候选
       │
       ▼
 [5. 字段别名漂移自适应] ─ 宽松映射：`name`→`title`, `content/desc`→`payload`, `quote`→`evidence`
       │
       ▼
 [6. 动作与引用消解自愈] ─ 虚构 target_id 自动模糊匹配既有角色/别名；找不到自动回退为新建
       │
       ▼
[输出洁净标准 Candidate]
```

1. **Token 耗尽截断自修复**：
   - 逆向定位最后一个合法的 `}` 并补全 `]`，将已被完整输出的条目全部抢救出来，避免因尾部截断导致整批提炼失败。
2. **字符串感知型语法清洗**：
   - 引入引号状态机 `in_string`：正文引号内部的双斜杠（如 `"密信 // 证物"`）被完整保留，仅在引号外部剥离 JS 风格行注释。
   - 实现无视空白与换行的尾随逗号自清除算法（如 `,\n  }` 和 `,\n  ]`）。
3. **字段别名漂移容错 (Tolerant Parser)**：
   - 定义 `TolerantRawCandidate`，支持任意第三方模型产生的别名字段（`name` / `content` / `desc` / `quote` / `targetId` 等），平滑映射至规范结构。
4. **悬挂 ID 与采纳冲突自愈**：
   - 模型若给出虚构的 `target_id`，系统根据中文名与历史别名（去空格、标点）模糊命中对应条目；若不存在则自动降级为 `create`，清空 `target_id`；
   - 采纳时若目标已被用户在其他页面删除，系统自动降级为新建，绝不中断或抛出 SQL 外键异常。
5. **数据库外键合规性防护**：
   - 候选入库时根据类型动态匹配合法预设代理 ID（`agent-worldview`, `agent-character`, `agent-plot`, `agent-outline`），杜绝外键约束违规。

---

## 五、 审计留档与回滚机制

所有通过 AI 提炼采纳的设定修改（世界观、角色卡、剧情大纲），均通过 [`record_setting_change`](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/src/db/settings.rs#L333) 统一留档：
- 完整记录修改前内容 (`before_content`)、修改后内容 (`after_content`)、目标实体与关联章节；
- 作者随时可在“**设定变更留档看板**”中查看历史轨迹，并提供一键安全回滚能力。

---

## 六、 验证与自动化测试矩阵

本次升级新增了端到端全量自愈与跨领域联动集成测试，且所有已有测试套件全量通过：

### 1. 新增集成测试：[`ai_extraction_self_healing.rs`](file:///Volumes/WD%20Drive/项目/Easy-writing%20·%20小说工作台/src-tauri/tests/ai_extraction_self_healing.rs)
- `test_self_healing_json_recovery_scenarios`：验证 Markdown 剥离、单对象包装、字段别名漂移、Token 截断抢救、尾随逗号清洗。**（PASSED）**
- `test_conflict_resolution_and_action_resolution`：验证虚构 ID 回落、别名归并去重、跨类别 ID 错用防御。**（PASSED）**
- `test_end_to_end_cross_domain_acceptance_and_self_healing`：端到端验证设定落库、词库自动入库、角色 6 维卡无损合并、关系管理与星云图连线建立、剧情伏笔挂接、大纲梗概回填。**（PASSED）**

### 2. 单元测试与已有测试矩阵
- `agent::extraction` 单元测试：12 个自愈与去重单测全部通过。**（PASSED）**
- `tests/epub_import.rs`：包含 9.4MB《绍宋》真实全书 473 章完整解析，23s 内毫秒级稳定通过。**（PASSED）**
- `tests/migration_ladder.rs`：数据库幂等迁移与列补齐测试全数通过。**（PASSED）**
- `tests/ipc_command_inventory.rs`：前后端 100% IPC 接口对齐测试全数通过。**（PASSED）**
- `frontend cargo check`：Leptos 响应式编译 0 警告、0 错误。**（PASSED）**

---

## 七、 文件变更清单

| 文件路径 | 变更性质 | 核心修改摘要 |
| :--- | :--- | :--- |
| `src-tauri/src/agent/extraction.rs` | 核心升级 | 新增 `self_heal_and_parse_candidates` 6 层自愈引擎、`TolerantRawCandidate` 宽松映射、外键合法映射及 12 个自修复测试用例 |
| `src-tauri/src/commands/ai_cmd.rs` | 核心升级 | 重构 `resolve_candidate`，打通设定、角色卡无损合并、关系管理推断、星云爆炸图连线、剧情伏笔、大纲梗概与词库自动同步 |
| `src-tauri/src/db/extraction.rs` | 升级扩展 | `insert_candidate` 与 `list_pending_candidates` 全面支持 `title` 字段读写 |
| `shared-types/src/lib.rs` | 结构扩展 | `ExtractionCandidate` 增加 `#[serde(default)] pub title: String`，保持前后端双向序列化兼容 |
| `src-tauri/migrations/V001__init.sql` | 架构规范 | `extraction_candidate` 表增加 `title` 字段 |
| `src-tauri/migrations/V002__legacy_safe_migrations.sql` | 幂等迁移 | 增加 `ALTER TABLE extraction_candidate ADD COLUMN title TEXT NOT NULL DEFAULT '';`，确保老数据库平滑升级 |
| `src-tauri/src/import/common.rs` | **新增模块** | 领域规则抽取：预编译正则、字数统计、HTML 反转义、文件名净化、段落排版封装 |
| `src-tauri/src/import/text.rs` | **新增模块** | 专职 TXT / Markdown 流式解析与智能切分，杜绝列表编号与句子引用卷名误判 |
| `src-tauri/src/import/db.rs` | **新增模块** | 专职分卷章节数据库事务持久化与顺序索引维护 |
| `src-tauri/src/import/epub.rs` | 重构精简 | 专职 EPUB 格式解析，移除重复清洗代码，复用 `common.rs` |
| `src-tauri/src/import/mod.rs` | 重构精简 | 精简为清晰的 Module Facade 与 IPC 暴露层，代码行数下降 75% |
| `frontend/src/components/ai/extraction_modal.rs` | 交互升级 | 呈现实体标题、新建/更新徽标与四色分类标签，采纳后触发 `lexicon_version` 实时响应 |
| `frontend/src/components/novel/import_modal.rs` | 交互升级 | 实现分卷手风琴折叠面板（Accordion）、卷章节字数汇总统计与书名即时修正 |
| `src-tauri/tests/ai_extraction_self_healing.rs` | **新增测试** | AI 提炼全域联动与全流程自愈能力端到端集成测试套件 |
| `src-tauri/tests/epub_import.rs` | 测试增强 | 增加纯文本与多格式分卷解析自动化单测，确保无回归 |
