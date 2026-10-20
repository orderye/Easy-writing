//! 恢复确认流程的前后端契约测试（V0.3.8 补充任务书 P0-3）。
//!
//! ## 为什么需要这类测试
//!
//! P0-3 的保障是「覆盖数据库之前必须让作者确认」。这条保障由三部分共同成立：
//! 后端的两阶段命令、前端的调用顺序、以及**确认弹窗真的显示在最上层**。
//! 前两者有 IPC 契约测试兜底，第三者没有 —— 而它恰恰最容易静默失效。
//!
//! 本项目已经真实踩过一次：确认弹窗写的是 `z-60`，但 Tailwind v3 的
//! 默认 z-index 刻度只到 50，`z-60` 根本不会生成任何 CSS。
//! 类名写在那里、代码看着完全正常，弹窗却拿不到预期的层级。
//! 这类失效不会报错、不会被任何现有测试发现，只会让「确认」在最需要它的时候
//! 被父级模态盖住。
//!
//! ## 数据安全
//!
//! 纯源码与编译产物扫描，不启动应用、不触碰任何数据库。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri 应位于仓库根目录下")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", p.display()))
}

fn frontend_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            frontend_rs_files(&p, out);
        } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// Tailwind 会把任意值类名里的方括号转义后写进选择器：
/// `z-[60]` → `.z-\[60\]{...}`。
fn css_selector_for(class: &str) -> String {
    format!(".{}{{", class.replace('[', "\\[").replace(']', "\\]"))
}

/// 从源码里取出所有 `z-<n>` / `z-[<n>]` 类名。
///
/// 全程按字节扫描、只切 ASCII 片段：源码里有大量中文注释，
/// 用 `&src[i..]` 逐字节切片会落在多字节字符中间直接 panic。
fn z_classes_in(src: &str) -> BTreeSet<String> {
    let b = src.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'z' && b[i + 1] == b'-' {
            // 前一个字节必须是分隔符，避免匹配到 `maximize-` 这类词的尾巴
            let boundary_ok = i == 0
                || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'-' || b[i - 1] == b'_');
            let mut j = i + 2;
            let mut digits = 0usize;
            while j < b.len()
                && (b[j].is_ascii_digit() || b[j] == b'[' || b[j] == b']')
            {
                if b[j].is_ascii_digit() {
                    digits += 1;
                }
                j += 1;
            }
            if boundary_ok && digits > 0 {
                // 这一段全是 ASCII，切片安全
                out.insert(format!("z-{}", &src[i + 2..j]));
            }
        }
        i += 1;
    }
    out
}

fn z_index_of(class: &str) -> i64 {
    class
        .trim_start_matches("z-")
        .trim_matches(|c| c == '[' || c == ']')
        .parse()
        .unwrap_or(-1)
}

// ===========================================================================
// 层级：确认弹窗必须真的盖在父级模态之上
// ===========================================================================

#[test]
fn every_z_index_class_in_frontend_actually_generates_css() {
    // 全仓护栏。这条用例存在的唯一理由，就是 `z-60` 曾经静默失效过：
    // Tailwind v3 默认刻度只到 50，写 `z-60` 不会报错，只是什么都不生成。
    let css = read("frontend/public/styles.css");
    assert!(
        !css.is_empty(),
        "frontend/public/styles.css 为空 —— 请先运行 pnpm build:css"
    );

    let mut files = Vec::new();
    frontend_rs_files(&repo_root().join("frontend/src"), &mut files);
    assert!(!files.is_empty(), "没有扫描到任何前端源文件");

    let mut missing: Vec<String> = Vec::new();
    let mut checked = BTreeSet::new();
    for f in &files {
        let src = fs::read_to_string(f).unwrap();
        for class in z_classes_in(&src) {
            if !checked.insert(class.clone()) {
                continue;
            }
            if !css.contains(&css_selector_for(&class)) {
                missing.push(format!(
                    "{}: {class}",
                    f.strip_prefix(repo_root()).unwrap_or(f).display()
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "以下 z-index 类名不会生成任何 CSS（写上去等于没写，弹窗会失去预期层级）：\n  {}\n\
         Tailwind v3 的默认 z 轴刻度只到 50；超出时请用任意值写法 z-[60]，\n\
         改完记得重新运行 pnpm build:css 并提交 frontend/public/styles.css。",
        missing.join("\n  ")
    );
    assert!(
        checked.len() >= 4,
        "扫描到的 z-index 类名过少（{}），扫描逻辑可能已失效",
        checked.len()
    );
}

#[test]
fn z_class_guard_is_not_vacuous() {
    // 护栏自检。
    //
    // 一条「扫描源码里的类名，再看 CSS 里有没有」的用例，最容易以两种方式空转：
    // 扫描逻辑匹配不到任何东西，或者 CSS 查找恒为真。两者都会让它永远绿。
    // 这里用一段合成字符串证明检测逻辑真的能区分「生成」与「不生成」——
    // 无需改动任何源文件。
    let css = read("frontend/public/styles.css");

    let found = z_classes_in(r#"<div class="fixed z-60 also z-[60] and z-50 not maximize-3">"#);
    assert_eq!(
        found.iter().cloned().collect::<Vec<_>>(),
        vec!["z-50".to_string(), "z-60".to_string(), "z-[60]".to_string()],
        "扫描必须取出全部 z-index 类名，且不能误吞 maximize-3 的尾巴"
    );

    // z-60 在 Tailwind v3 下确实不生成任何规则（默认刻度只到 50）。
    // 如果哪天它开始生成了，说明项目升级了 Tailwind 或扩展了 theme，
    // 那时这条断言会提醒回来复核护栏的前提。
    assert!(
        !css.contains(&css_selector_for("z-60")),
        "z-60 竟然生成了 CSS —— 护栏的前提已变化，请复核"
    );
    assert!(
        css.contains(&css_selector_for("z-[60]")),
        "任意值写法 z-[60] 必须生成 CSS，否则正确写法也被判为失效"
    );
    assert!(
        css.contains(&css_selector_for("z-50")),
        "刻度内的 z-50 必须生成 CSS"
    );
    assert_eq!(z_index_of("z-[60]"), 60);
    assert_eq!(z_index_of("z-50"), 50);
}

#[test]
fn restore_confirm_dialog_stacks_above_config_modal() {
    let modal_src = read("frontend/src/components/config/config_modal.rs");
    let tab_src = read("frontend/src/components/config/webdav_tab.rs");

    let modal_z = z_classes_in(&modal_src)
        .iter()
        .map(|c| z_index_of(c))
        .max()
        .expect("ConfigModal 应当声明 z-index");
    let dialog_z = z_classes_in(&tab_src)
        .iter()
        .map(|c| z_index_of(c))
        .max()
        .expect("恢复确认弹窗应当声明 z-index");

    assert!(
        dialog_z > modal_z,
        "WebdavTab 渲染在 ConfigModal（z-{modal_z}）内部，\
         确认弹窗必须高于它才能真正出现在作者面前，实际 z-{dialog_z}"
    );

    // 光有类名不够，还要确认它真的生成了 CSS
    let css = read("frontend/public/styles.css");
    for class in z_classes_in(&tab_src) {
        if z_index_of(&class) == dialog_z {
            assert!(
                css.contains(&css_selector_for(&class)),
                "确认弹窗的 {class} 没有对应的 CSS 规则"
            );
        }
    }
}

// ===========================================================================
// 调用顺序：预检必须在恢复之前
// ===========================================================================

#[test]
fn restore_button_runs_preflight_and_only_then_restores() {
    let src = read("frontend/src/components/config/webdav_tab.rs");

    assert!(
        src.contains("\"webdav_restore_preflight\""),
        "前端必须调用预检命令，否则拿不到可用于确认的事实"
    );
    assert!(
        src.contains("\"webdav_restore_backup\""),
        "前端必须调用恢复命令"
    );

    // 列表里的按钮必须触发预检，而不是直接恢复。
    // 直接恢复就等于把「确认」放回了覆盖之后 —— 正是要修的缺陷。
    let button = src
        .lines()
        .find(|l| l.contains("on_inspect_backup"))
        .expect("备份列表按钮应绑定预检动作");
    assert!(
        !button.contains("webdav_restore_backup"),
        "备份列表按钮不得直接调用恢复命令：{button}"
    );

    // 恢复命令只能在确认处理函数里被调用
    let confirm_fn_start = src
        .find("let on_confirm_restore")
        .expect("应存在确认后才执行的处理函数");
    let restore_call = src
        .find("\"webdav_restore_backup\"")
        .expect("应存在恢复命令调用");
    assert!(
        restore_call > confirm_fn_start,
        "webdav_restore_backup 必须位于 on_confirm_restore 之内（确认之后才执行）"
    );
    let preflight_call = src
        .find("\"webdav_restore_preflight\"")
        .expect("应存在预检命令调用");
    assert!(
        preflight_call < restore_call,
        "源码顺序上预检应先于恢复"
    );
}

#[test]
fn restore_invocation_passes_token_and_confirmation_flag() {
    let src = read("frontend/src/components/config/webdav_tab.rs");

    // 后端 webdav_restore_backup 的签名要求这两个参数；
    // 少了任何一个，Tauri 会以 missing required key 拒绝调用。
    for key in ["filename", "preflight_token", "confirm_empty_overwrite"] {
        assert!(
            src.contains(&format!("\"{key}\"")),
            "恢复调用必须传 {key}（后端命令签名要求 snake_case 键名）"
        );
    }

    // 确认标志必须来自预检结论，而不是硬编码 true。
    // 硬编码 true 等于取消确认：弹窗还在，但取消按钮不再有任何作用。
    assert!(
        src.contains("pf.requires_empty_overwrite_confirm"),
        "confirm_empty_overwrite 必须取自预检返回的 requires_empty_overwrite_confirm"
    );
    assert!(
        !src.contains(".arg(\"confirm_empty_overwrite\", true)"),
        "确认标志不得硬编码为 true"
    );
}

#[test]
fn dangerous_confirm_label_states_the_consequence() {
    let src = read("frontend/src/components/config/webdav_tab.rs");

    // 任务书 P0-3 C：文案必须明确表达「将用空库替换当前已有作品数据」，
    // 不得使用含糊的「继续操作」。
    assert!(
        src.contains("空库替换"),
        "空库覆盖的确认文案必须说明「用空库替换」这一后果"
    );
    assert!(
        src.contains("local_novel_count"),
        "确认文案必须带上本机现有作品数，作者才知道会失去多少"
    );
    assert!(
        src.contains("将用空库替换当前作品数据"),
        "弹窗标题必须直白说明后果"
    );

    // 取消路径必须存在，且必须真的清空待确认状态
    assert!(src.contains("\"取消\""), "必须有取消按钮");
    assert!(
        src.contains("pending_restore.set(None)"),
        "取消必须清空待确认状态，否则弹窗关不掉"
    );
    assert!(
        src.contains("当前数据未做任何改动"),
        "取消后必须明确告知作者数据没有被动过"
    );
}

// ===========================================================================
// 结果呈现：四类结局不得被折叠成一行「还原成功」
// ===========================================================================

#[test]
fn four_restore_outcomes_have_distinct_visual_treatment() {
    let src = read("frontend/src/components/config/webdav_tab.rs");

    for variant in ["Success", "PartialAssets", "RolledBack", "RollbackFailed"] {
        assert!(
            src.contains(&format!("RestoreOutcome::{variant}")),
            "前端必须单独处理 RestoreOutcome::{variant}"
        );
    }

    // 「回滚失败」是最需要作者立刻行动的状态，绝不能和「完全成功」用同一套样式
    let classes_fn = src
        .split("fn outcome_classes")
        .nth(1)
        .expect("应存在按结局区分样式的函数");
    let body = classes_fn.split("\n}").next().unwrap_or(classes_fn);
    let mut palettes = BTreeSet::new();
    for token in ["emerald", "amber", "orange", "red"] {
        if body.contains(token) {
            palettes.insert(token);
        }
    }
    assert_eq!(
        palettes.len(),
        4,
        "四类结局应使用四种可区分的视觉等级，实际只有 {palettes:?}"
    );
}

#[test]
fn restore_result_is_rendered_structurally_not_folded_into_a_count() {
    let src = read("frontend/src/components/config/webdav_tab.rs");

    // 旧实现把结论塞进 issues[0]，前端再折叠成「另有 N 项素材补齐异常」。
    // 那样作者永远看不到清单是否校验、配置是否写回、是否已回滚。
    assert!(
        !src.contains("项素材补齐异常"),
        "不得再把恢复结论折叠成一个计数"
    );

    // 必须逐项展示关键状态
    for needle in [
        "manifest_label",        // 清单校验状态（含「历史无清单」的区分）
        "database_restored",     // 数据库是否恢复
        "config_restored",       // 本地配置是否写回
        "config_error",          // 配置失败的具体阶段
        "rollback_error",        // 回滚失败原因
        "snapshot_path",         // 安全快照位置
        "snapshot_retained",     // 快照是否保留
        "is_empty_library",      // 空库警告
        "asset_failed",          // 素材失败数
    ] {
        assert!(
            src.contains(needle),
            "恢复结果必须展示 {needle}，否则作者看不到关键状态"
        );
    }

    // 「历史无清单备份」必须与「清单校验通过」区分展示
    assert!(
        src.contains("ManifestCheck::Absent") && src.contains("ManifestCheck::Verified"),
        "必须区分无清单的历史备份与已通过校验的清单"
    );
}

#[test]
fn backup_gaps_are_surfaced_to_the_author() {
    let src = read("frontend/src/components/config/webdav_tab.rs");

    // 备份侧同理：清单没传上去、图片没同步、素材本地就缺，
    // 都不能被一句「✓ 备份成功」盖过去。
    assert!(
        src.contains("BackupReport"),
        "备份命令必须消费结构化的 BackupReport，而不是只拿一个文件名"
    );
    for needle in [
        "manifest_uploaded",
        "image_upload_failures",
        "missing_assets",
        "verified",
    ] {
        assert!(
            src.contains(needle),
            "备份结果必须展示 {needle}"
        );
    }
}

// ===========================================================================
// 后端契约
// ===========================================================================

#[test]
fn restore_commands_are_registered_and_gated() {
    let lib = read("src-tauri/src/lib.rs");
    assert!(
        lib.contains("commands::sync_cmd::webdav_restore_preflight"),
        "预检命令必须注册进 invoke_handler，否则前端调用会被拒绝"
    );
    assert!(
        lib.contains("commands::sync_cmd::webdav_restore_backup"),
        "恢复命令必须注册进 invoke_handler"
    );

    let cmd = read("src-tauri/src/commands/sync_cmd.rs");
    // 两个命令都接受 filename，因此都必须做路径穿越校验
    let preflight_start = cmd
        .find("pub async fn webdav_restore_preflight")
        .expect("应存在预检命令");
    let restore_start = cmd
        .find("pub async fn webdav_restore_backup")
        .expect("应存在恢复命令");
    let after_preflight = &cmd[preflight_start..restore_start];
    let after_restore = &cmd[restore_start..];
    assert!(
        after_preflight.contains("validate_backup_filename"),
        "预检命令必须校验备份文件名（filename 会被拼进 WebDAV URL）"
    );
    assert!(
        after_restore.contains("validate_backup_filename"),
        "恢复命令必须校验备份文件名"
    );

    // 生产路径不得启用故障注入
    assert!(
        after_restore.contains("RestoreHooks::default()"),
        "生产命令必须使用默认（无故障注入）的 RestoreHooks"
    );
}
