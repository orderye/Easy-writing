//! 集成测试共享脚手架。
//!
//! 放在 `tests/common/` 下：Cargo 只会把 `tests/` 顶层的 `.rs` 当作独立测试二进制，
//! 子目录里的模块需要通过 `mod common;` 显式引入，因此这里不会产生额外的空测试目标。

pub mod webdav_server;

use std::path::PathBuf;

/// 每个测试独占一个临时目录。
///
/// 名字里带 pid 与纳秒时间戳，因此并行运行不会互相踩踏；
/// 全部数据都在系统临时目录下，**绝不接触用户真实数据库与真实 WebDAV 目录**。
pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ew_it_{}_{}_{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
