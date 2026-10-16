//! IPC 命令清单的自动比对。
//!
//! 【为什么需要这个测试】
//! 删掉前端的 80 个 `#[derive(Serialize)]` 参数结构体后，命令名与参数键名从
//! 「编译器校验的结构体字段」退化为「运行时字符串」——写错命令名不会有任何编译
//! 错误，只会在运行时报 command not found。
//!
//! 原先 `command_key_sweep.rs` 靠**手工维护**的 handler 列表守护这一点，而它
//! 已经失同步过一次：上一轮新增 8 个命令后没人同步该列表，导致这些命令一度无
//! 测试覆盖。人工清单本身就是易腐的，所以这里改为两侧都从源码自动提取：
//!
//! - 后端：扫描 `src/commands/*.rs` 中所有 `#[tauri::command]` 标注的函数名
//! - 前端：扫描 `frontend/src/**/*.rs` 中所有 `invoke_cmd("...")` 与
//!   `invoke_no_args::<T>("...")` 的字面量命令名
//!
//! 然后断言「前端调用的每个命令后端都存在」。新增命令若忘了前端对接（或反之）
//! 会在这里立刻暴露，不需要任何人记得去更新一个手写列表。
//!
//! Run: cargo test -p easy-writing-core --test ipc_command_inventory

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 仓库根目录（本测试文件位于 src-tauri/tests/）。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri 应位于仓库根下")
        .to_path_buf()
}

/// 收集目录下所有 `.rs` 文件（递归）。
fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 后端：从 `#[tauri::command]` 标注后紧跟的 `pub [async] fn NAME` 提取命令名。
///
/// 不依赖注释或人工列表——真正的契约来源就是这些属性标注。
fn backend_commands() -> BTreeSet<String> {
    let mut cmds = BTreeSet::new();
    for file in rs_files(&repo_root().join("src-tauri/src/commands")) {
        let src = std::fs::read_to_string(&file).expect("读取后端命令源文件");
        let mut awaiting_fn = false;
        for line in src.lines() {
            let t = line.trim();
            if t.starts_with("#[tauri::command") {
                awaiting_fn = true;
                continue;
            }
            if awaiting_fn {
                // 跳过其他属性（如 #[allow(...)]），直到遇到函数签名
                if t.starts_with("#[") || t.is_empty() {
                    continue;
                }
                if let Some(name) = parse_fn_name(t) {
                    cmds.insert(name);
                }
                awaiting_fn = false;
            }
        }
    }
    cmds
}

/// 从 `pub fn foo(` / `pub async fn foo(` 提取 `foo`。
fn parse_fn_name(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("pub async fn ")
        .or_else(|| line.strip_prefix("pub fn "))?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// 前端：提取所有 invoke_cmd("name") 与 invoke_no_args::<T>("name") 的命令名。
fn frontend_commands() -> BTreeSet<String> {
    let mut cmds = BTreeSet::new();
    for file in rs_files(&repo_root().join("frontend/src")) {
        let src = std::fs::read_to_string(&file).expect("读取前端源文件");
        for line in src.lines() {
            // 跳过注释行，避免把文档里的示例代码当成真实调用
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            cmds.extend(extract_quoted_after(line, "invoke_cmd("));
            cmds.extend(extract_quoted_after(line, "invoke_no_args::<"));
            // 经 settings_panel/loaders.rs 的封装调用：命令名以 &'static str 传入，
            // 仍是字面量，但不在 invoke_cmd( 之后，需要单独提取。
            cmds.extend(extract_quoted_after(line, "load_for_novel("));
            cmds.extend(extract_quoted_after(line, "load_list_for_novel("));
        }
    }
    cmds
}

/// 在 `marker` 之后找到第一个字符串字面量（形如 "name"）。
///
/// 对 invoke_no_args::<T>("cmd")，marker 之后的第一个字面量就是命令名
/// （泛型参数里不会出现字符串字面量）。
fn extract_quoted_after(line: &str, marker: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(pos) = line[search_from..].find(marker) {
        let abs = search_from + pos + marker.len();
        let tail = &line[abs..];
        if let Some(start) = tail.find('"') {
            if let Some(end_rel) = tail[start + 1..].find('"') {
                let name = &tail[start + 1..start + 1 + end_rel];
                // 命令名一律小写字母/数字/下划线；过滤掉非命令名的杂项
                // 注意：这里**不能**把「不像命令名」的字面量静默丢弃——
                // 那恰好会放过最该报错的情况（大小写笔误、连字符、拼写错误）。
                // 全部收集，由调用方统一判定：合法命令名之外的一律视为可疑。
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        }
        search_from = abs;
    }
    out
}

/// 核心断言：前端调用的每个命令，后端都必须存在。
#[test]
fn every_frontend_invoked_command_exists_in_backend() {
    let backend = backend_commands();
    let frontend = frontend_commands();

    assert!(
        backend.len() > 50,
        "后端命令提取结果异常偏少（{} 个），生成逻辑可能失效",
        backend.len()
    );
    assert!(
        frontend.len() > 50,
        "前端调用提取结果异常偏少（{} 个），生成逻辑可能失效",
        frontend.len()
    );

    let missing: Vec<&String> = frontend.difference(&backend).collect();

    assert!(
        missing.is_empty(),
        "以下命令被前端调用，但后端没有对应的 #[tauri::command]\n\
         （写错命令名不会编译报错，只会在运行时报 command not found）：\n  {}\n\
         若确实是新增命令，请确认后端已用 #[tauri::command] 标注；\n\
         若是笔误，请修正前端调用点。",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// 自检：提取逻辑本身必须能工作，否则上面那个断言会「因为提取不到东西而通过」。
///
/// 这是防「测试静默失效」的元测试——一个什么都不提取的提取器会让
/// `missing.is_empty()` 恒为真。
#[test]
fn extractors_actually_extract() {
    // 必须真的扫到后端命令（而非空集）
    let backend = backend_commands();
    for known in ["create_novel", "save_chapter_content", "get_app_setting"] {
        assert!(
            backend.contains(known),
            "后端提取漏掉了已知命令 {known}；提取逻辑已失效"
        );
    }

    // 必须真的扫到前端调用（含经 loaders 封装的那一类）
    let frontend = frontend_commands();
    for known in [
        "create_novel",          // 直接 invoke_cmd
        "list_novels",           // invoke_no_args
        "list_characters",       // 经 load_list_for_novel 封装
        "list_trash_items",      // 经 load_for_novel 封装
    ] {
        assert!(
            frontend.contains(known),
            "前端提取漏掉了已知调用 {known}；提取逻辑已失效"
        );
    }

    // 已知不存在的名字不应被凭空造出来
    assert!(!frontend.contains("this_command_does_not_exist"));
}

/// 反向检查：后端定义了但前端从未调用的命令。
///
/// 这一项**只告警不失败**——后端保留一些暂未被 UI 使用的命令是合理的
/// （例如仅供测试或预留），硬性失败会制造噪音。但它能回答
/// 「这个命令到底有没有人用」这个常见疑问。
#[test]
fn report_backend_commands_never_invoked_by_frontend() {
    let backend = backend_commands();
    let frontend = frontend_commands();

    let unused: Vec<&String> = backend.difference(&frontend).collect();

    println!(
        "后端共 {} 个命令，其中 {} 个未被前端调用：",
        backend.len(),
        unused.len()
    );
    for c in &unused {
        println!("  - {c}");
    }

    let covered = backend.len() - unused.len();
    println!(
        "覆盖：{}/{}（{:.0}%）",
        covered,
        backend.len(),
        100.0 * covered as f64 / backend.len() as f64
    );
}
