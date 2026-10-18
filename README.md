# Easy-writing · 中长篇小说工作台

<img src="src-tauri/icons/128x128@2x.png" width="88" alt="Easy-writing" align="right" />

> 把**设定、大纲、章节正文**与**多 AI Agent** 放进同一界面的本地写作工作台。
>
> 设定自长成 · 结构清晰 · 素材随手插 · 多 Agent 协作 · 数据完全自主

[![Tauri 2](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust 2021](https://img.shields.io/badge/Rust-2021-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Leptos 0.7](https://img.shields.io/badge/Leptos-0.7_(WASM)-EF3939)](https://leptos.dev/)
[![Tiptap 2](https://img.shields.io/badge/Tiptap-2.x-1C1C1C)](https://tiptap.dev/)
[![SQLite](https://img.shields.io/badge/SQLite-WAL-003B57?logo=sqlite&logoColor=white)](https://www.sqlite.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

![Easy-writing 三栏工作台界面](Docs/images/ui-layout.png)

---

## 🌟 核心特性

### 1. ✍️ 专为百万字长篇打造的结构化写作台
- **三栏沉浸式工作台**：左栏「设定/图库/词库」· 中栏「正文写作台」· 右栏「全书卷章目录树」，各面板均可一键折叠收起。
- **百万字长篇性能保障**：经受 274 万字《绍宋》等全量篇章实测校验，在 300+ 章节大纲与长正文下保持流畅滚动与即时响应。
- **便捷卷章整理**：支持跨卷拖拽移动、上下排序调整，提供智能章节批量导入，自动识别格式拆分卷与章节。

### 2. 📚 看板式多作品管理中心（多书库与全局统计）
- **全库指标概览**：顶部看板直观汇总全库作品总数、累计创作总字数（千分位/万字展示）、总篇章数与总分卷数。
- **精细化作品卡片**：模拟书脊封面与状态徽章，清晰标注当前创作中作品、各作品独立字数、卷数、篇章数与最后更新时间。
- **作品元数据维护**：支持在弹窗内直接编辑作品名称与详细简介（题材、核心概要），工作台顶栏与设定面板实时联动响应。
- **实时搜索与平滑切换**：支持按作品名与简介实时模糊检索；在当前章节有未保存修改时智能拦截防丢，确认后一键秒级平滑切换不同作品工作区。
- **全生命周期回收站**：作品连同所有卷章支持安全软删除，本地数据完整留存，可在底部随时一键撤销恢复。

### 3. 📥 全格式长篇智能导入（TXT / Markdown / EPUB 电子书）
- **全格式支持**：全面支持 TXT / Markdown 纯文本文档以及标准 `.epub` 电子书整书导入。
- **纯 MIT 原生 EPUB 解析引擎**：内置自主实现的轻量级安全解包引擎，自动读取 OPF 包元数据、提取作品书名与封面简介；完美兼容 **EPUB 2**（`toc.ncx`）与 **EPUB 3**（`nav.xhtml`）双标准目录树。
- **智能阅读顺序清洗**：按 Spine 顺序提取正文 HTML/XHTML，智能剥离冗余标签，纯净转换为标准 `<p>` 段落并精确计算汉字字数。
- **交互式拆卷拆章预览**：内置强大的正则拆卷拆章识别器，导入前提供完整树状目录与章节段落预览，支持在预览窗口实时调优、重命名卷章、删减并一键落库。

### 4. 🎨 本地图片素材与插图图库（全生命周期管理）
- **作品级素材库**：集中管理本作品的所有人物立绘、场景概念图与设定线稿，支持四列响应式网格与懒加载浏览。
- **一键光标插入**：在图库中点击「插入到光标处」，即刻无缝以富文本图片节点嵌入当前正文段落。
- **素材路径复制**：支持一键将本地素材相对路径复制到系统剪贴板（兼容现代化 Clipboard API 与浏览器降级方案）。
- **文件缺失体检与告警徽标**：物理文件移动或丢失时，自动高亮琥珀色警示边框、标记缺失徽标，并在顶栏汇总体检状态。
- **全生命周期运维**：支持素材重命名、软删除、回收站恢复与彻底清除；删除前自动统计正文与历史版本的引用规模。
- **存储对账与孤儿清理**：全库扫描物理磁盘孤儿图片与数据库记录对账，严格校验引用状态，安全清理未被引用的磁盘孤儿文件。

### 5. 🎭 深度角色档案与动态关系拓扑
- **六维结构化角色爆破卡片**：涵盖核心动机（渴望/所需/心魔/底线）、外貌特征、言谈声线口头禅、重要资产信物、致命秘密死穴与阵营主配分级。
- **物理引力关系图谱（Force-Graph）**：基于物理引擎动态渲染全书人物社交网络，支持节点拖拽、高亮关联路径与关系纽带双向即时编辑。

### 6. 🤖 设定自长成与多 Agent 智能协同
- **设定初始可留空**：先写正文，后台 4 个专注的 AI Agent（世界观提炼、角色建模、剧情推演、细纲梳理）异步提炼并沉淀设定。
- **冲突裁决与留档审计**：写作助手严格遵循已有设定，检测到正文逻辑矛盾时即刻弹窗提醒裁决，改动全程留档防吃书。
- **行内快捷 AI 浮标**：正文中选中任意文本即刻呼出 AI 气泡（润色 / 扩写 / 缩写 / 续写），支持并排 Diff 差异对比与一键采纳替换。
- **灵活的接口与模型适配**：全面兼容各大模型协议（OpenAI 兼容接口、商汤 SenseNova、DeepSeek、Moonshot、通义千问、硅基流动、本地 Ollama 等）；支持实时拉取远端模型列表，细粒度调节 Temperature 与最大 Tokens。

### 7. 📖 专属小说词库与智能语料提取
- **专有术语管理**：全书专属词典统一收录术语、门派法宝、地理风貌，正文输入时智能识别。
- **双向提取入库**：世界观设定与角色条目一键提取入词库，正文高频词扫描与词频热度追踪。

### 8. 📝 专业富文本编辑与版本时间轴
- **Tiptap 2 + ProseMirror 核心**：结构化文档模型，切章不销毁编辑器实例，零延迟切换与即时脏检查保存。
- **全文搜索替换**：支持全局快速定位与批量替换（<kbd>⌘F</kbd> / <kbd>Ctrl+F</kbd>）。
- **版本历史时间轴**：自动定时快照与手动保存历史版本，支持逐行差异对比与一键秒级回退，防止误删断稿。
- **精准字数统计**：专业汉字/标点/词数精细化计算，预估全篇阅读时长。

### 9. 🔒 数据绝对自主与原生钥匙串安全
- **单机 SQLite ACID 存储**：所有作品、设定、章节均保存在本地单文件 SQLite（开启 WAL 模式），断电防崩溃。
- **原生系统钥匙串安全存储**：API Key 与 WebDAV 密码绝不落入明文数据库，全部交由各平台原生安全子系统（macOS Keychain、Windows 凭据管理器、Linux Secret Service）加密持久化。
- **软删除作品回收站**：作品、卷、章节支持软删除机制，误删可一键完好恢复，亦可彻底清空释放空间。

### 10. 🔄 WebDAV 云端备份与软件在线更新
- **标准 WebDAV 快照备份**：支持坚果云、Nextcloud、群晖等任意标准 WebDAV 服务端，原子级打包上传与跨设备无缝恢复。
- **双平台在线检查更新**：内置跨平台版本检测器，联动 GitHub 与 CNB Releases API，发现新版本一键直达安装包下载。

---

## 🛠️ 技术栈

| 模块 | 技术选型 | 说明 |
| :--- | :--- | :--- |
| **桌面运行时** | **Tauri 2.x** (Rust) | 原生安全跨平台，打包体积仅 ~18MB，极低内存占用 |
| **前端框架** | **Leptos 0.7** (CSR / WASM) | Rust 细粒度响应式驱动，编译为 WASM，零虚拟 DOM 开销 |
| **富文本核心** | **Tiptap 2 + ProseMirror** | 结构化文档模型，切章不销毁实例，无缝脏检查与光标恢复 |
| **本地数据** | **SQLite 3** (`rusqlite` + WAL) | 单文件 ACID，支持一致性快照与安全热备份 |
| **安全密钥** | **系统钥匙串** (`keyring`) | macOS Keychain / Windows 凭据管理器 / Linux Secret Service，凭据永不落库 |
| **自动化测试** | **Node.js Test + Rust cargo test** | 覆盖前端 DOM 桥接、IPC 命令比对、UI 契约、作品级联删除与图库存储对账 |

---

## 🚀 快速开始

### 准备环境
- **Node.js** ≥ 18 & **pnpm** ≥ 9
- **Rust** (stable) 并添加 WASM 目标：`rustup target add wasm32-unknown-unknown`
- **Trunk**：`cargo install trunk --locked`

### 本地开发
```bash
# 1. 安装依赖
pnpm install

# 2. 启动开发（支持前端热重载与 Tauri 原生调试）
pnpm dev:frontend   # 终端 A: 启动 Trunk WASM 服务 (http://127.0.0.1:8080)
pnpm tauri dev      # 终端 B: 启动 Tauri 桌面应用
```

### 运行全量测试
```bash
pnpm test:frontend  # 运行前端与 Tiptap 桥接自动化测试
pnpm test:backend   # 运行后端 10 大测试套件（命令自省、契约检查、数据库级联等）
```

### 生产打包
```bash
pnpm build          # 自动编译静态资源、WASM 并生成当前平台桌面安装包（DMG / 安装包）
```
macOS 构建产物输出于 `target/release/bundle/dmg/`，Windows 可使用 `cargo-xwin` 原生交叉编译。

---

## 📦 下载与安装

已提供预编译正式发行版：

| 平台 | 格式 | 下载渠道 |
| :--- | :--- | :--- |
| **macOS (Apple Silicon)** | `.dmg`（安装镜像）/ `.zip`（免安装） | [GitHub Releases](https://github.com/orderye/Easy-writing/releases) · [CNB Releases](https://cnb.cool/toroyxxx/Easy-writing/-/releases) |
| **Windows (x64)** | `.msi`（安装向导）/ `.zip`（便携免安装） | [GitHub Releases](https://github.com/orderye/Easy-writing/releases) · [CNB Releases](https://cnb.cool/toroyxxx/Easy-writing/-/releases) |

> 💡 **macOS 首次打开提示处理**：  
> 若下载打开时系统提示“无法打开，因为无法验证开发者”或“已损坏”，系未签名个人开源应用的隔离属性所致。请在终端执行以下命令清除隔离属性即可正常运行：
> ```bash
> xattr -cr /Applications/Easy-writing.app
> ```
> 或在访达（Finder）的「应用程序」目录中，按住 <kbd>Control</kbd> 键右键点击应用图标并选择「打开」。

---

## 🛡️ 数据流与第三方 AI 隐私透明度

- **100% 本地优先与数据自主**：所有的作品元数据、章节正文、大纲、世界观条目与插图素材均持久化在本地 SQLite 数据库（`easy-writing.db`）与本地磁盘目录中。应用内**不包含任何远程遥测（Telemetry）、埋点统计或用户追踪代码**。
- **第三方 AI 交互透明度**：
  - 仅在创作者**主动点击或触发** AI 功能（如润色、对话、一致性检测、设定提取）时，才会将当前操作上下文或选中段落文本通过直接 HTTPS 请求发送给创作者**自行配置的 AI 提供商**（如 OpenAI、Anthropic、DeepSeek、Moonshot 或本地 Ollama）。
  - 应用不设立任何第三方云端中转服务器，直接由客户端点对点连接模型厂商官方 API 接口。
- **凭据硬件级隔离**：大模型 API Key 与 WebDAV 凭据**绝不以明文或密文写入本地 SQLite 数据库**，完全托管在操作系统底层受保护的凭据系统（macOS Keychain / Windows 凭据管理器 / Linux Secret Service），防范窃取。

---

## 📚 详细设计与规范文档

- [更新日志 (Changelog)](CHANGELOG.md) —— 版本演进记录与重要修复详情
- [审校整改方案](Docs/Easy-writing%20审校整改方案.md) —— 外部审校 13 项重点缺陷整改全记录
- [框架方案与 PRD](<Docs/Easy-writing · 中长篇小说工作台 框架方案与 PRD.md>) —— 完整产品定位、核心场景、AI 协同模型与交互规格
- [实施计划与数据设计](<Docs/Easy-writing · 实施计划.md>) —— 数据库 DDL、IPC 架构、时序图与工程设计（[基线归档]）
- [双远程 Release 发布流程参考规范](Docs/双远程发布流程.md) —— GitHub 与 CNB 双平台 Release 标准发布 6 步 SOP 与校验清单
- [双远程提交流程规范](Docs/提交流程.md) —— 主目录与 `.cnb-worktree` 隔离身份双端提交流程

---

## 📄 开源协议

本项目基于 [MIT License](LICENSE) 开源协议发布。
