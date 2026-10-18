# 更新日志 (Changelog)

所有 Easy-writing 的重要更新与架构变更均记录于此。  
本项目遵循 [Semantic Versioning (语义化版本 2.0.0)](https://semver.org/lang/zh-CN/) 规范。

---

## [0.3.4] - 2026-10-18

### 安全性与一致性重大修复 (Security & Consistency)
- **修复章节切换自动保存竞态 (P0-1)**：在前端切换章节时增加异步等待确认机制，必须等待当前章节正文入库并落盘完毕后方可切换 `current_chapter`，彻底消除了未保存正文被目标章节空模板冲刷覆盖的数据丢失风险。
- **修复 AI 提炼去重误杀 `action='update'` (P0-2)**：将提炼去重哈希从朴素的 `kind + title` 升级为支持条目更新的复合哈希规则（`kind + target_id + normalized(payload)`），并在判定忽略/采纳状态时仅排除已审阅项，修复既有设定无法被 AI 提取更新的致命缺陷。
- **加固 WebDAV 云端全库恢复安全性 (P0-3)**：
  - 增加数据库版本前瞻校验（若备份文件的 `schema_version` 高于本机支持版本则强制阻断）。
  - 在全库覆盖前后增加本地状态保护：自动备份并重新合并本机独有的 `webdav_config` 与个性化 UI 设置（主题、面板折叠），防止跨设备覆盖。
  - 恢复完成后自动扫描全部模型配置的系统钥匙串（Keychain）凭据，发现断链 API Key 及缺失图片时生成完整的 `RestoreReport` 并弹窗指引创作者重新配置。
- **正文图片跨设备持久化规范 (P0-4)**：ProseMirror 文档 JSON 中图片 `src` 统一存储为规范化的跨设备相对协议 `asset://local/<rel_path>`，在运行时由 Tiptap Bridge 动态映射为本机真实文件 URL，彻底解决多端同步后绝对路径失效导致“图裂”的问题。
- **WebDAV 原子上传与退出保护 (P0-5)**：
  - 备份上传采用临时文件隔离模式（先上传至 `.tmp`，通过 HTTP HEAD 校验远程大小无误后，再利用 WebDAV `MOVE` 原子替换为目标文件）。
  - 备份时间戳全面采用 UTC 格式（`YYYYMMDD_HHMMSSZ`）。
  - 云端备份自动轮转修剪，默认保留最近 20 份完整快照。
  - 窗口关闭退出时的静默同步超时保护由 5 秒放宽至 10 秒。

### 架构健壮性与功能完善 (Robustness & Features)
- **大纲与章节梗概设定回滚支持 (P1-1)**：在 `revert_setting_change` 与 `apply_conflict_setting_update` 中补齐对 `chapter`、`volume`、`outline` 及 `novel` 描述的回滚更新逻辑。
- **章节历史版本自动保存修剪 (P1-2)**：在 `maybe_auto_snapshot` 中引入自动保存快照修剪机制，每章节默认保留最新的 30 份 `auto` 快照，手动快照与回滚前快照永久保留，杜绝长篇创作导致的数据库无限膨胀。
- **数据库关键性能索引与迁移阶梯 V003 (P1-3)**：新增 `V003__indices_and_cleanup.sql` 迁移，为 `asset(novel_id, hash)`、`asset(hash)`、`plot_item(chapter_id)`、`setting_change(target_type, target_id)`、`chapter_revision(chapter_id, revision_type, created_at DESC)` 补齐高性能索引。
- **设定冲突检测“忽略警告并写入正文”按钮 (P1-4)**：在 `ConflictModal` 中增加明确的“忽略警告并写入正文”操作，允许作者在遭遇 AI 假阳性误判或特殊剧情反转时一键采纳正文而不修改既有设定库。

### 工程规范与文档校准 (Standards & Docs)
- **文档架构基线修正与归档 (P2-1)**：在《实施计划》与《PRD》头部添加 `[ARCHIVED BASELINE]` 警示标头，同步修正目录树为标准 Cargo Workspace 布局（`src-tauri`、`frontend`、`shared-types`），嵌入真实架构图与界面截图。
- **Docs 目录文档体系全量对齐与索引建设**：重命名修复带有冒号 `:` 的审校报告文件名以防 Windows 检出失败；消除历史遗留的实施计划目录与结语矛盾；新建 `Docs/README.md` 全景导航图与速查矩阵。
- **README 隐私透明度与基准严谨性 (P2-2)**：补充第三方 AI 数据流隐私说明与开源隐私承诺，增加 macOS Gatekeeper 绕过隔离属性指引。
- **文档隐私脱敏 (P2-3)**：对提交流程与双远程发布规范中的个人邮箱进行泛化脱敏。
- **算法安全升级 (P2-4)**：将章节提炼指纹计算从 MD5 升级为 SHA-256。

---

## [0.3.3] - 2026-10-17
- 交付双远程发布流程（GitHub + CNB 镜像），支持自动挂载 Release 安装包。
- 引入前端 TipTap Bridge 资产加载桥接。

## [0.3.0] - 2026-10-15
- 交付多 Agent 剧情与角色一致性审校引擎。
- 引入 WebDAV 云端全库备份与还原系统。
- 接入系统钥匙串安全存储模型 API 凭据。

## [0.2.0] - 2026-10-10
- 完成 Leptos 0.7 CSR 前端与 Tauri 2.0 桌面端混合渲染架构。
- 实现章节目录树拖拽排序与实时字数统计。

## [0.1.0] - 2026-10-01
- 项目立项与核心数据模型设计。
