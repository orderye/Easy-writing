//! 图库 UI 与桥接能力的契约测试。
//!
//! 验证 5 项核心能力在前后端契约与组件源码中的落地情况：
//! 1. 缩略图渲染（resolve_asset_url 桥接与 assetProtocol 协议范围）
//! 2. 插入到光标处（insert_image 富文本桥接）
//! 3. 复制路径（copy_asset_path 剪贴板桥接）
//! 4. 文件缺失徽标（broken 响应式集合与 on:error 告警徽标）
//! 5. 深色模式与面板收起布局（dark: 样式类与 fixed 居中浮层）

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri 应位于仓库根目录下")
        .to_path_buf()
}

#[test]
fn test_gallery_thumbnail_rendering_contract() {
    let root = repo_root();
    let asset_bridge = fs::read_to_string(root.join("frontend/src/bridge/asset.rs"))
        .expect("读取 frontend/src/bridge/asset.rs 失败");
    let tauri_conf = fs::read_to_string(root.join("src-tauri/tauri.conf.json"))
        .expect("读取 src-tauri/tauri.conf.json 失败");
    let gallery_src = fs::read_to_string(root.join("frontend/src/components/editor/asset_gallery.rs"))
        .expect("读取 frontend/src/components/editor/asset_gallery.rs 失败");

    // 1. 必须配置 assetProtocol 并且 scope 包含 $APPDATA/images/**
    assert!(tauri_conf.contains("\"assetProtocol\""), "tauri.conf.json 缺少 assetProtocol");
    assert!(tauri_conf.contains("\"$APPDATA/images/**\""), "assetProtocol scope 未包含 $APPDATA/images/**");

    // 2. 必须存在 resolve_asset_url 转换函数
    assert!(asset_bridge.contains("pub fn resolve_asset_url"), "缺少 resolve_asset_url 导出");
    assert!(asset_bridge.contains("convertFileSrc"), "缺少 convertFileSrc 调用");

    // 3. 图库渲染缩略图时必须调用 resolve_asset_url
    assert!(gallery_src.contains("resolve_asset_url"), "图库未调用 resolve_asset_url 进行缩略图转换");
    assert!(gallery_src.contains("loading=\"lazy\""), "缩略图应启用 lazy loading 以保证流畅性");
}

#[test]
fn test_gallery_insert_to_cursor_contract() {
    let root = repo_root();
    let tiptap_bridge = fs::read_to_string(root.join("frontend/src/bridge/tiptap.rs"))
        .expect("读取 frontend/src/bridge/tiptap.rs 失败");
    let gallery_src = fs::read_to_string(root.join("frontend/src/components/editor/asset_gallery.rs"))
        .expect("读取 frontend/src/components/editor/asset_gallery.rs 失败");

    // 1. tiptap bridge 必须声明 insert_image
    assert!(tiptap_bridge.contains("pub fn insert_image"), "tiptap.rs 缺少 insert_image 声明");

    // 2. 图库 on_insert 动作必须触发 insert_image 并将 save_status 标记为 dirty
    assert!(gallery_src.contains("crate::bridge::tiptap::insert_image"), "图库未接入 insert_image");
    assert!(gallery_src.contains("app_ctx.save_status.set(\"dirty\".to_string())"), "插入图片后未标记 dirty 状态");
    assert!(gallery_src.contains("\"插入到光标处\""), "图库缺少「插入到光标处」用户交互按钮");
}

#[test]
fn test_gallery_copy_path_contract() {
    let root = repo_root();
    let asset_bridge = fs::read_to_string(root.join("frontend/src/bridge/asset.rs"))
        .expect("读取 frontend/src/bridge/asset.rs 失败");
    let gallery_src = fs::read_to_string(root.join("frontend/src/components/editor/asset_gallery.rs"))
        .expect("读取 frontend/src/components/editor/asset_gallery.rs 失败");

    // 1. 剪贴板复制桥接
    assert!(asset_bridge.contains("pub fn copy_asset_path"), "缺少 copy_asset_path 导出");
    assert!(asset_bridge.contains("clipboard.writeText"), "缺少 clipboard.writeText");

    // 2. 图库 UI 操作接入
    assert!(gallery_src.contains("copy_asset_path"), "图库未调用 copy_asset_path");
    assert!(gallery_src.contains("\"复制路径\""), "图库缺少「复制路径」按钮");
    assert!(gallery_src.contains("✓ 已复制路径"), "图库缺少复制成功状态提示反馈");
}

#[test]
fn test_gallery_missing_file_badge_contract() {
    let root = repo_root();
    let gallery_src = fs::read_to_string(root.join("frontend/src/components/editor/asset_gallery.rs"))
        .expect("读取 frontend/src/components/editor/asset_gallery.rs 失败");

    // 1. 缩略图必须捕获 on:error
    assert!(gallery_src.contains("on:error="), "缩略图 img 标签必须绑定 on:error 事件监听");
    assert!(gallery_src.contains("broken.update("), "加载错误时必须更新 broken 集合");

    // 2. 存在黄色警告徽标与提示
    assert!(gallery_src.contains("⚠ 缺失"), "缩略图卡片上缺少「⚠ 缺失」徽标");
    assert!(gallery_src.contains("条记录找不到文件"), "标题栏缺少缺失统计汇总徽标");
    assert!(gallery_src.contains("border-amber-300"), "缺失卡片缺少琥珀色高亮警示边框");
}

#[test]
fn test_gallery_dark_mode_and_panel_collapse_layout() {
    let root = repo_root();
    let gallery_src = fs::read_to_string(root.join("frontend/src/components/editor/asset_gallery.rs"))
        .expect("读取 frontend/src/components/editor/asset_gallery.rs 失败");

    // 1. 全局深色模式类适配
    assert!(gallery_src.contains("dark:bg-slate-800"), "缺少暗色背景适配");
    assert!(gallery_src.contains("dark:border-slate-700"), "缺少暗色边框适配");
    assert!(gallery_src.contains("dark:text-slate-100"), "缺少暗色文本适配");

    // 2. 独立于左右折叠面板的悬浮模态框布局（不会被折叠挤压或遮盖）
    assert!(gallery_src.contains("fixed inset-0"), "必须是 fixed inset-0 模态视口");
    assert!(gallery_src.contains("z-50"), "模态框必须具备最高层级 z-50");
    assert!(gallery_src.contains("max-w-4xl"), "模态框容器最大宽度");
    assert!(gallery_src.contains("overflow-y-auto"), "内容区域必须支持纵向滚动");
}
