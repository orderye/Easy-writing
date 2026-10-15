# Easy-writing · 中长篇小说工作台

<img src="src-tauri/icons/128x128@2x.png" width="88" alt="Easy-writing" align="right" />

> 把**设定、大纲、章节正文**与**多 AI Agent** 放进同一界面的本地写作工作台。
>
> 设定自长成 · 结构清晰 · 多 Agent 协作 · 数据完全自主

[![Tauri 2](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust 2021](https://img.shields.io/badge/Rust-2021-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Leptos 0.7](https://img.shields.io/badge/Leptos-0.7_(WASM)-EF3939)](https://leptos.dev/)
[![Tiptap 2](https://img.shields.io/badge/Tiptap-2.x-1C1C1C)](https://tiptap.dev/)
[![SQLite](https://img.shields.io/badge/SQLite-WAL-003B57?logo=sqlite&logoColor=white)](https://www.sqlite.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

![Easy-writing 三栏工作台界面](Docs/images/ui-layout.png)

---

## 🌟 核心特性

- ✍️ **面向百万字长篇设计**：三栏沉浸布局（设定 / 写作台 / 目录），右栏长目录视口虚拟滚动（300+ 章节丝滑无卡顿），支持跨卷移动、拖拽排序与文稿批量导入智能拆卷拆章。
- 🎭 **深度角色档案与互动拓扑**：
  - **角色爆破卡片**：结构化立绘维度，涵盖核心动机（渴望/所需/心魔/底线）、外貌特征、言谈声线口头禅、重要资产信物、致命秘密死穴与阵营主配分级。
  - **关系拓扑图谱**：基于引力物理引擎（Force-Graph）动态渲染人物关系网，支持节点拖拽、高亮关联路径与纽带快速编辑。
- 📖 **专属小说词库与语料提取**：全书专有术语词典管理，世界观与角色条目自动双向提取入库，正文高频词智能扫描与词频热度追踪。
- 🤖 **多 Agent 协同与设定自长成**：
  - 设定初始可留空，正文保存后 4 个 Agent 后台异步提炼世界观、角色、剧情和细纲；
  - 正文写作助手严格遵循设定，遭遇矛盾即刻弹出冲突裁决与改动留档审计；
  - 选中文本即刻弹出快捷 AI 浮标（润色 / 扩写 / 缩写 / 续写 + 并排 Diff 对比采纳）。
- 📝 **专业富文本与编辑体验**：基于 Tiptap 2 + ProseMirror，支持全文查找替换（<kbd>⌘F</kbd> / <kbd>Ctrl+F</kbd>）、版本历史时间轴一键回滚、精细化汉字/词数统计与预计阅读时长。
- 🔒 **数据绝对自主与安全**：单机 SQLite 本地存储，API Key 与 WebDAV 密码存入系统钥匙串；支持原子级 WebDAV 快照备份与软删除回收站。
- 🚀 **软件在线自动更新**：集成 GitHub / CNB Releases API 语义化版本检测，跨平台安装包智能匹配与一键更新。

---

## 🛠️ 技术栈

| 模块 | 技术选型 | 说明 |
| :--- | :--- | :--- |
| **桌面运行时** | **Tauri 2.x** (Rust) | 原生安全跨平台，打包仅 ~18MB，极低内存占用 |
| **前端框架** | **Leptos 0.7** (CSR / WASM) | 细粒度响应式驱动，零虚拟 DOM 开销 |
| **富文本核心** | **Tiptap 2 + ProseMirror** | 结构化文档模型，切章不销毁实例，无缝脏检查 |
| **本地数据** | **SQLite 3** (`rusqlite` + WAL) | 单文件 ACID，支持一致性快照与安全热备份 |
| **安全密钥** | **系统钥匙串** (`keyring`) | macOS Keychain / Windows 凭据管理器，密钥永不入库 |

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

### 生产打包
```bash
pnpm build          # 自动编译静态资源、WASM 并生成当前平台桌面安装包
```
构建产物输出于 `src-tauri/target/release/bundle/`。

---

## 📦 下载与安装

已提供预编译正式发行版：

| 平台 | 格式 | 下载渠道 |
| :--- | :--- | :--- |
| **macOS (Apple Silicon)** | `.dmg` / `.zip` | [GitHub Releases](https://github.com/orderye/Easy-writing/releases) · [CNB Releases](https://cnb.cool/toroyxxx/Easy-writing/-/releases) |
| **Windows (x64)** | `.msi` / `.zip` | [GitHub Releases](https://github.com/orderye/Easy-writing/releases) · [CNB Releases](https://cnb.cool/toroyxxx/Easy-writing/-/releases) |

---

## 📚 详细设计文档

- [框架方案与 PRD](<Docs/Easy-writing · 中长篇小说工作台 框架方案与 PRD.md>) —— 完整产品定位、核心场景、AI 协同模型与交互规格
- [实施计划与数据设计](<Docs/Easy-writing · 实施计划.md>) —— 数据库 DDL、IPC 架构、时序图与四阶段工程设计
- [演进交付计划](<Docs/Easy-writing · 小说工作台 全面后续研发与演进计划.md>) —— 里程碑交付记录与技术指标追踪

---

## 📄 开源协议

本项目基于 [MIT License](LICENSE) 开源协议发布。
