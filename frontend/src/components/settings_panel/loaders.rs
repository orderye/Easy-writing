//! settings 面板的公共加载助手。
//!
//! 六个设置模块（世界观 / 角色 / 剧情 / 词库 / 回收站 / 设定留档）原先各自写了
//! 形状相同的 `load_X` 闭包：取当前作品 → 无作品则跳过 → spawn_local →
//! `invoke_cmd("list_x").arg("novel_id", ..)` → 写入信号。
//! 这段样板重复 7 次且容易漏更新（例如新增一处忘记在处理里带 novel_id）。
//!
//! `load_for_novel` 把它收敛成一次调用；「作品切换时刷新」的语义也随之单点化。

use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use serde::de::DeserializeOwned;

/// 按当前作品加载一个列表命令的结果，写入 `target`。
///
/// `cmd` 必须是接受单个 `novel_id` 参数、返回 `Vec<T>` 的 Tauri 命令。
/// 当前无选中作品时什么都不做（与原样板一致）。
///
/// `on_done` 在写入结果后调用，用于复位 loading 标记等收尾工作。
pub fn load_for_novel<T, F>(
    app_ctx: AppContext,
    cmd: &'static str,
    target: RwSignal<Vec<T>>,
    on_done: F,
) where
    T: DeserializeOwned + Send + Sync + 'static,
    F: Fn() + Send + Sync + 'static,
{
    if let Some(novel) = app_ctx.current_novel.get() {
        let novel_id = novel.id;
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_cmd(cmd)
                .arg("novel_id", novel_id)
                .send::<Vec<T>>()
                .await
            {
                target.set(list);
            }
            on_done();
        });
    }
}

/// `load_for_novel` 的简写：不需要收尾动作时使用。
pub fn load_list_for_novel<T>(app_ctx: AppContext, cmd: &'static str, target: RwSignal<Vec<T>>)
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    load_for_novel(app_ctx, cmd, target, || {});
}
