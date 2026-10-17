//! 保存状态机的纯逻辑部分。
//!
//! 这段逻辑被刻意从组件里抽出来，因为它承载的是一个**容易写错的并发语义**：
//! 保存是异步的，而用户在保存期间可以继续输入。
//! 抽成纯函数后可以用普通 `cargo test` 覆盖，不需要 DOM。

/// 保存完成后的目标状态。
///
/// - `succeeded`：IPC 是否成功返回。
/// - `edited_since`：从本次保存取走正文之后，编辑器是否又发生了新的编辑。
///
/// 关键分支是 `(true, true)` —— 保存成功，但保存期间用户又改了内容。
/// 此时磁盘上的内容**早于**编辑器内容，绝不能标记成 "saved"：
/// 一旦标记为已保存，切章逻辑就不会弹未保存提示，那一段输入会被静默丢弃。
pub fn resolve_save_status(succeeded: bool, edited_since: bool) -> &'static str {
    match (succeeded, edited_since) {
        (false, _) => "error",
        (true, true) => "dirty",
        (true, false) => "saved",
    }
}

/// 自动保存失败后的退避等待秒数（指数退避，上限 60 秒）。
///
/// `base` 是用户配置的自动保存间隔。连续失败次数越多等得越久，
/// 避免在磁盘满 / 数据库被锁这类持续故障下疯狂重试。
pub fn auto_save_backoff_secs(base: u32, consecutive_failures: u32) -> u32 {
    let base = base.max(5);
    let shift = consecutive_failures.min(3);
    base.saturating_mul(1u32 << shift).min(60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_failed_is_always_error() {
        assert_eq!(resolve_save_status(false, false), "error");
        // 失败优先于"期间有新编辑"：不能因为又编辑过就把失败掩盖成 dirty
        assert_eq!(resolve_save_status(false, true), "error");
    }

    #[test]
    fn save_succeeded_with_new_edits_must_stay_dirty() {
        // 这是本模块存在的理由：保存成功后磁盘内容仍落后于编辑器
        assert_eq!(resolve_save_status(true, true), "dirty");
    }

    #[test]
    fn save_succeeded_without_new_edits_is_saved() {
        assert_eq!(resolve_save_status(true, false), "saved");
    }

    #[test]
    fn backoff_is_bounded_and_monotonic() {
        assert_eq!(auto_save_backoff_secs(30, 0), 30);
        assert_eq!(auto_save_backoff_secs(30, 1), 60);
        // 上限 60 秒，不会无限增长
        assert_eq!(auto_save_backoff_secs(30, 2), 60);
        assert_eq!(auto_save_backoff_secs(30, 9), 60);
        // 用户把间隔调小也要有个下限，避免打爆 IPC
        assert_eq!(auto_save_backoff_secs(1, 0), 5);
    }
}
