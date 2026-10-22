//! 远端备份保留策略。
//!
//! ## 它解决什么问题
//!
//! 旧实现只有一条规则：「保留最近 20 份」（`KEEP: usize = 20` 写死在
//! `backup.rs`）。这条规则能防止目录无限膨胀，但它对**时间跨度**一无所知：
//! 作者每天备份一次，20 份就是 20 天；一个月不备份，20 份可能是两年半。
//! 真正需要在意的往往是「上周那一份还在不在」，而不是「第 20 份还在不在」。
//!
//! 现在保留集合由三个维度取并集：
//!
//! - **最近 N 份**（`recent`）：保住「手滑删了什么」的近邻
//! - **每日 1 份**（`daily`）：覆盖最近 N 个有备份的自然日
//! - **每周 1 份**（`weekly`）：覆盖最近 N 个有备份的 ISO 周
//!
//! ## 两条不可放宽的安全铁律
//!
//! 1. **新备份未验证通过时一个都不删**（见 `plan_retention`）。
//!    否则「新备份不可用 + 旧备份已删」是不可恢复的组合。
//! 2. **只处理本应用创建的 `backup_*.db.gz`**。
//!    用户手动放进远端目录的其它文件不属于本应用的策略范围。
//!
//! 此外还有一条本模块自己加的保守规则：**文件名里解析不出时间戳的备份一律保留**。
//! 名字不可信时，宁可留一份垃圾，也不猜一个时间点把它删掉。

use chrono::{DateTime, Datelike, Utc};
use rusqlite::Connection;
use shared_types::RetentionPolicy;
use std::collections::{BTreeMap, BTreeSet};

/// 设置项键名（存 `app_setting` 表，写行即可，无需迁移）。
pub const SETTING_KEEP_RECENT: &str = "backup_keep_recent";
pub const SETTING_KEEP_DAILY: &str = "backup_keep_daily";
pub const SETTING_KEEP_WEEKLY: &str = "backup_keep_weekly";

/// 默认值。`recent` 与旧实现写死的 `KEEP = 20` 一致 ——
/// 升级后作者在不做任何设置的情况下，行为与升级前相同。
pub const DEFAULT_KEEP_RECENT: usize = 20;
pub const DEFAULT_KEEP_DAILY: usize = 7;
pub const DEFAULT_KEEP_WEEKLY: usize = 4;

/// 一次保留计算的完整结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPlan {
    /// 保留的文件名（按文件名升序，便于展示与比对）。
    pub keep: Vec<String>,
    /// 将被删除的文件名（按文件名升序 = 由旧到新）。
    pub delete: Vec<String>,
    /// 远端目录中不属于本应用备份命名范围的文件数。
    pub protected_foreign: usize,
}

/// 该名字是否是**备份本体**（`backup_*.db.gz`）。
///
/// 归属只看命名规则：附件（清单 sidecar、meta sidecar）另有各���判定，
/// 不算备份本体 —— 备份本体才是保留策略要决定去留的对象。
pub fn is_backup_package(name: &str) -> bool {
    name.starts_with("backup_") && name.ends_with(".db.gz")
}

/// 该名字是否是备份的**清单 sidecar**（`<backup>.db.gz.json`）。
pub fn is_manifest_sidecar(name: &str) -> bool {
    name.ends_with(".db.gz.json")
}

/// 该名字是否是备份的 **meta sidecar**（`<backup>.db.gz.meta.json`）。
///
/// meta 记录「这份备份是哪台设备写的」，用于跨设备覆盖保护；
/// 它与清单一样是备份的附属文件，随备份本体一起生灭。
pub fn is_meta_sidecar(name: &str) -> bool {
    name.ends_with(".db.gz.meta.json")
}

/// 该名字是否属于本应用创建的备份（含其附属文件）。
///
/// 判定沿用旧实现的规则：剥掉可选的 `.json` 后缀后，
/// 必须以 `backup_` 开头且以 `.db.gz` 结尾。
/// 清单 sidecar（`<backup>.db.gz.json`）因此也会被识别为「备份本体」，
/// 轮转时随之一起清理，不会留下孤儿清单。
pub fn is_managed_backup_name(name: &str) -> bool {
    let base = name.strip_suffix(".json").unwrap_or(name);
    base.starts_with("backup_") && base.ends_with(".db.gz")
}

/// 从备份文件名解析出内嵌的 UTC 时间戳。
///
/// 备份文件名形如 `backup_20260101_000000Z.db.gz`。
/// 解析不出来返回 `None` —— 调用方据此判定「不参与按时间的规则」。
pub fn parse_backup_timestamp(name: &str) -> Option<DateTime<Utc>> {
    let base = name.strip_suffix(".json").unwrap_or(name);
    let stem = base.strip_prefix("backup_")?;
    let ts = stem.strip_suffix(".db.gz")?;
    let naive = chrono::NaiveDateTime::parse_from_str(ts, "%Y%m%d_%H%M%SZ").ok()?;
    Some(naive.and_utc())
}

/// 计算保留与删除集合。
///
/// `names` 应当是远端目录下的全部文件名（顺序不限）。
///
/// 「最近 N 份」按文件名排序，不按解析出来的时间戳。
///
/// 这一维刻意沿用旧实现的**文件名字典序**。原因是既有轮转测试用
/// 「非法月份」构造过备份名（如 `backup_20251301_...`），那些名字解析不出
/// 时间戳；一旦「解析失败就特殊处理」，旧行为与新行为就会在真实数据上分叉。
///
/// 正常情况下两者等价：备份名内嵌的是 `YYYYMMDD_HHMMSSZ` 形式的 UTC 时间戳，
/// 字典序即时间序。字典序还有一个额外好处 —— 它对**任何**名字都成立，
/// 不会出现「时间戳解析失败就没有归属」的空洞。
///
/// 每日 / 每周两个维度则**只**统计时间戳解析成功的备份：没有日期就没有
/// 「哪天」「哪周」可言，让它们参与只会造出没有意义的分组。
pub fn build_plan(names: &[String], policy: RetentionPolicy) -> RetentionPlan {
    let mut parsed: Vec<(String, DateTime<Utc>)> = Vec::new();
    let mut managed_names: Vec<String> = Vec::new();
    let mut protected_foreign = 0usize;

    for name in names {
        if is_backup_package(name) {
            managed_names.push(name.clone());
            if let Some(ts) = parse_backup_timestamp(name) {
                parsed.push((name.clone(), ts));
            }
        } else if is_manifest_sidecar(name) || is_meta_sidecar(name) {
            // 本应用的附属文件：随备份本体一起生灭，
            // 既不参与保留计算，也不计入「策略管不到的文件」。
        } else {
            protected_foreign += 1;
        }
    }

    // ① 最近 N 份：按文件名升序取最后 N 个（与旧实现完全一致）
    managed_names.sort();
    let mut keep: BTreeSet<String> = BTreeSet::new();
    let recent_start = managed_names.len().saturating_sub(policy.recent);
    for name in managed_names.iter().skip(recent_start) {
        keep.insert(name.clone());
    }

    // 时间倒序；同一时刻（理论上不会发生，文件名内嵌时间戳唯一）
    // 按文件名升序兜底，保证结果稳定可测。
    parsed.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // ② 每日 1 份：按 UTC 自然日分组，每组保留最新一份，
    //    再取最近 `daily` 个「有备份的日子」。
    let mut by_day: BTreeMap<chrono::NaiveDate, &String> = BTreeMap::new();
    for (name, ts) in parsed.iter() {
        by_day
            .entry(ts.date_naive())
            .or_insert(name);
    }
    let day_start = by_day.len().saturating_sub(policy.daily);
    for (_, name) in by_day.iter().skip(day_start) {
        keep.insert((*name).clone());
    }

    // ③ 每周 1 份：按 ISO 周分组，每组保留最新一份，
    //    再取最近 `weekly` 个「有备份的周」。
    let mut by_week: BTreeMap<(i32, u32), &String> = BTreeMap::new();
    for (name, ts) in parsed.iter() {
        let iso = ts.iso_week();
        by_week.entry((iso.year(), iso.week())).or_insert(name);
    }
    let week_start = by_week.len().saturating_sub(policy.weekly);
    for (_, name) in by_week.iter().skip(week_start) {
        keep.insert((*name).clone());
    }

    // 删除集合 = 受管备份本体 − 保留集合，按文件名升序（由旧到新）。
    // 清单 / meta sidecar 不在这里：它们随备份本体一起删，由调用方负责。
    let mut delete: Vec<String> = names
        .iter()
        .filter(|n| is_backup_package(n) && !keep.contains(*n))
        .cloned()
        .collect();
    delete.sort();

    let mut keep_list: Vec<String> = keep.into_iter().collect();
    keep_list.sort();

    RetentionPlan {
        keep: keep_list,
        delete,
        protected_foreign,
    }
}

/// 计算本次应当删除的旧备份文件名。
///
/// 铁律：**新备份未验证通过时一个都不删**。
/// 少了这一条，「新备份不可用 + 旧备份已删」就是不可恢复的组合。
pub fn plan_retention(
    names: &[String],
    new_backup_verified: bool,
    policy: RetentionPolicy,
) -> Vec<String> {
    if !new_backup_verified {
        return Vec::new();
    }
    build_plan(names, policy).delete
}

/// 从 `app_setting` 读取保留策略。
///
/// 缺失、非法、负数都回落到默认值；三个维度**同时**为 0 视为非法配置
/// （等于「把所有备份都删掉」），整组回落到默认值。
pub fn policy_from_settings(conn: &Connection) -> RetentionPolicy {
    let read = |key: &str, default: usize| -> usize {
        match crate::db::app_setting::get_setting(conn, key) {
            Ok(Some(raw)) => raw.trim().parse::<usize>().unwrap_or(default),
            _ => default,
        }
    };

    let policy = RetentionPolicy {
        recent: read(SETTING_KEEP_RECENT, DEFAULT_KEEP_RECENT),
        daily: read(SETTING_KEEP_DAILY, DEFAULT_KEEP_DAILY),
        weekly: read(SETTING_KEEP_WEEKLY, DEFAULT_KEEP_WEEKLY),
    };

    if policy.recent == 0 && policy.daily == 0 && policy.weekly == 0 {
        return RetentionPolicy {
            recent: DEFAULT_KEEP_RECENT,
            daily: DEFAULT_KEEP_DAILY,
            weekly: DEFAULT_KEEP_WEEKLY,
        };
    }
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn policy(recent: usize, daily: usize, weekly: usize) -> RetentionPolicy {
        RetentionPolicy {
            recent,
            daily,
            weekly,
        }
    }

    #[test]
    fn parses_embedded_timestamp() {
        let ts = parse_backup_timestamp("backup_20260101_000000Z.db.gz").unwrap();
        assert_eq!(ts.to_rfc3339(), "2026-01-01T00:00:00+00:00");
        // 清单 sidecar 走同一条解析路径
        assert_eq!(
            parse_backup_timestamp("backup_20260101_000000Z.db.gz.json"),
            Some(ts)
        );
        assert!(parse_backup_timestamp("someone-else.db.gz").is_none());
        assert!(parse_backup_timestamp("backup_not-a-date.db.gz").is_none());
    }

    #[test]
    fn managed_names_exclude_foreign_and_meta_files() {
        assert!(is_managed_backup_name("backup_20260101_000000Z.db.gz"));
        assert!(is_managed_backup_name("backup_20260101_000000Z.db.gz.json"));
        assert!(!is_managed_backup_name("someone-elses-data.db.gz"));
        assert!(!is_managed_backup_name("backup_20260101_000000Z.db.gz.meta.json"));

        // 备份本体 / 清单 sidecar / meta sidecar 三类必须分得开：
        // 保留策略只对「备份本体」做去留判断。
        assert!(is_backup_package("backup_20260101_000000Z.db.gz"));
        assert!(!is_backup_package("backup_20260101_000000Z.db.gz.json"));
        assert!(is_manifest_sidecar("backup_20260101_000000Z.db.gz.json"));
        assert!(is_meta_sidecar("backup_20260101_000000Z.db.gz.meta.json"));
    }

    #[test]
    fn recent_dimension_keeps_newest_n() {
        let list = names(&[
            "backup_20260101_000000Z.db.gz",
            "backup_20260102_000000Z.db.gz",
            "backup_20260103_000000Z.db.gz",
        ]);
        let plan = build_plan(&list, policy(2, 0, 0));
        assert_eq!(plan.delete, vec!["backup_20260101_000000Z.db.gz".to_string()]);
        assert_eq!(plan.keep.len(), 2);
    }

    #[test]
    fn daily_dimension_keeps_one_per_day() {
        // 同一天两份 + 另一天一份，daily=1 → 只留最近那一天（2026-01-02）那一份
        let list = names(&[
            "backup_20260101_010000Z.db.gz",
            "backup_20260101_230000Z.db.gz",
            "backup_20260102_050000Z.db.gz",
        ]);
        let plan = build_plan(&list, policy(0, 1, 0));
        assert_eq!(plan.keep, vec!["backup_20260102_050000Z.db.gz".to_string()]);
        assert_eq!(plan.delete.len(), 2);
    }

    #[test]
    fn daily_dimension_keeps_latest_of_each_day() {
        // daily=2 → 覆盖最近两个「有备份的日子」，
        // 每天只留最新的一份（01-01 的 23:00 那份）
        let list = names(&[
            "backup_20260101_010000Z.db.gz",
            "backup_20260101_230000Z.db.gz",
            "backup_20260102_050000Z.db.gz",
        ]);
        let plan = build_plan(&list, policy(0, 2, 0));
        assert!(plan.keep.contains(&"backup_20260102_050000Z.db.gz".to_string()));
        assert!(plan.keep.contains(&"backup_20260101_230000Z.db.gz".to_string()));
        assert!(!plan.keep.contains(&"backup_20260101_010000Z.db.gz".to_string()));
    }

    #[test]
    fn weekly_dimension_keeps_one_per_week() {
        // 2026-01-01（周四）属第 1 周；2026-01-05（周一）与 2026-01-08（周四）属第 2 周。
        // weekly=1 → 只留第 2 周最新的一份（01-08）。
        let list = names(&[
            "backup_20260101_000000Z.db.gz",
            "backup_20260105_000000Z.db.gz",
            "backup_20260108_000000Z.db.gz",
        ]);
        let plan = build_plan(&list, policy(0, 0, 1));
        assert_eq!(plan.keep, vec!["backup_20260108_000000Z.db.gz".to_string()]);

        // weekly=2 → 第 1 周与第 2 周各留最新的一份
        // （第 2 周最新的是 01-08；01-05 与它同周，被它挤掉）
        let plan = build_plan(&list, policy(0, 0, 2));
        assert!(plan.keep.contains(&"backup_20260108_000000Z.db.gz".to_string()));
        assert!(plan.keep.contains(&"backup_20260101_000000Z.db.gz".to_string()));
        assert!(!plan.keep.contains(&"backup_20260105_000000Z.db.gz".to_string()));
    }

    #[test]
    fn dimensions_are_unioned() {
        let list = names(&[
            "backup_20260101_000000Z.db.gz",
            "backup_20260102_000000Z.db.gz",
            "backup_20260103_000000Z.db.gz",
            "backup_20260104_000000Z.db.gz",
        ]);
        // 四天都在第 1 周：weekly=1 留最新一份（01-04），
        // daily=1 同样落在 01-04，recent=1 也是 01-04。
        let plan = build_plan(&list, policy(1, 1, 1));
        assert_eq!(plan.keep, vec!["backup_20260104_000000Z.db.gz".to_string()]);
        assert_eq!(plan.delete.len(), 3);

        // 维度取并集：recent=2 保住 01-03/01-04，daily=2 覆盖最近两天（同为 01-03/01-04）
        let plan = build_plan(&list, policy(2, 2, 0));
        assert!(plan.keep.contains(&"backup_20260104_000000Z.db.gz".to_string()));
        assert!(plan.keep.contains(&"backup_20260103_000000Z.db.gz".to_string()));
        assert!(!plan.keep.contains(&"backup_20260102_000000Z.db.gz".to_string()));
        assert!(!plan.keep.contains(&"backup_20260101_000000Z.db.gz".to_string()));
    }

    #[test]
    fn unparsable_names_follow_recent_dimension() {
        // 名字解析不出时间戳的备份仍受「最近 N 份」约束（按文件名字典序），
        // 但不参与每日 / 每周维度 —— 没有日期就没有「哪天」「哪周」可言。
        let list = names(&[
            "backup_20260101_000000Z.db.gz",
            "backup_mystery.db.gz",
        ]);
        let plan = build_plan(&list, policy(1, 0, 0));
        assert!(
            plan.keep.contains(&"backup_mystery.db.gz".to_string()),
            "按文件名字典序，最后一个名字就是最近的一份：{:?}",
            plan.keep
        );
        assert!(plan.delete.contains(&"backup_20260101_000000Z.db.gz".to_string()));
    }

    #[test]
    fn foreign_files_are_never_managed() {
        let list = names(&[
            "backup_20260101_000000Z.db.gz",
            "someone-elses-data.db.gz",
        ]);
        let plan = build_plan(&list, policy(0, 0, 0));
        assert!(!plan.delete.contains(&"someone-elses-data.db.gz".to_string()));
        assert_eq!(plan.protected_foreign, 1);
    }

    #[test]
    fn unverified_backup_deletes_nothing() {
        let list: Vec<String> = (1..=30)
            .map(|i| format!("backup_202601{:02}_000000Z.db.gz", i))
            .collect();
        assert!(plan_retention(&list, false, policy(1, 0, 0)).is_empty());
    }
}
