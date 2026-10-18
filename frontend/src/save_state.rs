//! 保存状态机的纯逻辑部分。
//!
//! 这段逻辑被刻意从组件里抽出来，因为它承载的是一个**容易写错的并发语义**：
//! 保存是异步的，而用户在保存期间可以继续输入。
//! 抽成纯函数后可以用普通 `cargo test` 覆盖，不需要 DOM。

/// 保存执行后的终态判定枚举。
///
/// 消除单纯 `bool` 返回值带来的二义性：
/// - `Saved`：IPC 成功落库且期间没有新增输入，允许安全切章；
/// - `Failed`：IPC 报错或数据库写失败，严禁切章；
/// - `EditedDuringSave`：虽已成功持久化取样时的快照，但等待期间用户又键入了新字符，
///   正文再次处于脏状态，必须继续追平保存，不能直接切章。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    /// 成功保存且正文已完全同步
    Saved,
    /// 保存失败（网络、IPC、SQL 锁等）
    Failed,
    /// 本次快照已保存，但保存期间产生了新编辑，内容依然未完全持久化
    EditedDuringSave,
}

impl SaveOutcome {
    pub fn as_status_str(&self) -> &'static str {
        match self {
            SaveOutcome::Saved => "saved",
            SaveOutcome::Failed => "error",
            SaveOutcome::EditedDuringSave => "dirty",
        }
    }

    #[allow(dead_code)]
    pub fn is_safe_to_switch(&self) -> bool {
        matches!(self, SaveOutcome::Saved)
    }
}

/// 根据 IPC 执行结果与并发输入判定终态。
pub fn resolve_save_outcome(succeeded: bool, edited_since: bool) -> SaveOutcome {
    match (succeeded, edited_since) {
        (false, _) => SaveOutcome::Failed,
        (true, true) => SaveOutcome::EditedDuringSave,
        (true, false) => SaveOutcome::Saved,
    }
}

/// 保存完成后的目标状态字符串（兼容保留）。
#[allow(dead_code)]
pub fn resolve_save_status(succeeded: bool, edited_since: bool) -> &'static str {
    resolve_save_outcome(succeeded, edited_since).as_status_str()
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

/// 切章安全性终态判定。
///
/// 切章需要跨越异步 IPC（get_chapter），在等待目标章节加载期间：
/// - 若用户输入了新字符（`current_edit_seq != start_edit_seq`），正文已落后，严禁覆盖；
/// - 若保存状态处于非 `saved`（如保存失败或中途变脏），严禁覆盖；
/// - 仅在完全干净且无新增编辑时，才允许将目标章节覆盖写入编辑器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchSafetyOutcome {
    /// 安全：期间无任何新输入且处于 saved 状态，允许切章
    SafeToSwitch,
    /// 存在新编辑：读取目标章节期间产生了新击键，放弃切章以保全正文
    AbortedDueToNewEdits,
    /// 处于未保存状态：当前章节未成功保存，放弃切章
    AbortedDueToUnsaved,
}

pub fn check_switch_safety(
    start_edit_seq: u64,
    current_edit_seq: u64,
    current_save_status: &str,
) -> SwitchSafetyOutcome {
    if current_edit_seq != start_edit_seq {
        SwitchSafetyOutcome::AbortedDueToNewEdits
    } else if current_save_status != "saved" {
        SwitchSafetyOutcome::AbortedDueToUnsaved
    } else {
        SwitchSafetyOutcome::SafeToSwitch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_failed_is_always_error() {
        assert_eq!(resolve_save_outcome(false, false), SaveOutcome::Failed);
        assert_eq!(resolve_save_outcome(false, true), SaveOutcome::Failed);
        assert_eq!(resolve_save_status(false, false), "error");
        assert_eq!(resolve_save_status(false, true), "error");
        assert!(!resolve_save_outcome(false, false).is_safe_to_switch());
    }

    #[test]
    fn save_succeeded_with_new_edits_must_stay_dirty() {
        // 这是关键分支：保存成功但磁盘内容仍落后于编辑器，绝不允许直接切章
        let outcome = resolve_save_outcome(true, true);
        assert_eq!(outcome, SaveOutcome::EditedDuringSave);
        assert_eq!(outcome.as_status_str(), "dirty");
        assert_eq!(resolve_save_status(true, true), "dirty");
        assert!(!outcome.is_safe_to_switch());
    }

    #[test]
    fn save_succeeded_without_new_edits_is_saved() {
        let outcome = resolve_save_outcome(true, false);
        assert_eq!(outcome, SaveOutcome::Saved);
        assert_eq!(outcome.as_status_str(), "saved");
        assert_eq!(resolve_save_status(true, false), "saved");
        assert!(outcome.is_safe_to_switch());
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

    #[test]
    fn switch_safety_detects_concurrent_edits() {
        // 等待 IPC 期间序号增加：拒绝覆盖
        assert_eq!(
            check_switch_safety(10, 11, "saved"),
            SwitchSafetyOutcome::AbortedDueToNewEdits
        );
        // 等待 IPC 期间虽未增加序号，但处于未保存状态：拒绝覆盖
        assert_eq!(
            check_switch_safety(10, 10, "dirty"),
            SwitchSafetyOutcome::AbortedDueToUnsaved
        );
        assert_eq!(
            check_switch_safety(10, 10, "error"),
            SwitchSafetyOutcome::AbortedDueToUnsaved
        );
        // 完全无改动且为 saved：安全切换
        assert_eq!(
            check_switch_safety(10, 10, "saved"),
            SwitchSafetyOutcome::SafeToSwitch
        );
    }
}
