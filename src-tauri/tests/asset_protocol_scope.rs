//! T12 · Asset 协议与 CSP：把审计结论固化成回归测试。
//!
//! 升级任务书对 T12 的怀疑是「scope 只匹配一层目录、CSP 可能为 null」。
//! 源码级审计的结论是**符合**：`tauri.conf.json` 的 scope 已是 `$APPDATA/images/**`
//! （递归通配，覆盖任意嵌套层级），CSP 非 null 且含 asset 必需项。
//!
//! 但「审计结论符合」本身没有牙齿 —— 配置哪天被改窄（`*` 替掉 `**`，嵌套图片集体裂图）
//! 或改宽（`$APPDATA/**`，整个应用数据目录暴露给 asset 协议），
//! 都不会有任何报错。所以本模块用 **Tauri 自己的判定逻辑**把这些结论钉住：
//!
//! - `tauri::scope::fs::Scope::is_allowed` —— scope 到底允不允许某个真实路径；
//! - `tauri::path::SafePathBuf` —— 含 `../` 的路径在进入 asset 协议前就被拒。
//!
//! 用的都是 Tauri 自己的实现，不是我们复刻一遍的近似逻辑：
//! 复刻的断言只能证明「我们写的正则符合我们的预期」，证明不了 Tauri 的真实行为。
//!
//! Run: cargo test -p easy-writing-core --test asset_protocol_scope

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::path::SafePathBuf;
use tauri::scope::fs::Scope;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::utils::config::FsScope;

/// 读取仓库里的 tauri.conf.json（就是打包时真正生效的那份配置）。
fn tauri_conf() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let raw = fs::read_to_string(&path).expect("读取 tauri.conf.json 失败");
    serde_json::from_str(&raw).expect("解析 tauri.conf.json 失败")
}

/// 从配置里取出 assetProtocol.scope（FsScope 是 untagged 枚举，路径列表即 AllowedPaths）。
fn asset_scope() -> FsScope {
    let scope = tauri_conf()["app"]["security"]["assetProtocol"]["scope"].clone();
    serde_json::from_value(scope).expect("assetProtocol.scope 应当是路径列表")
}

/// scope 里配置的原始模式列表。
fn scope_patterns() -> Vec<String> {
    asset_scope()
        .allowed_paths()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// 从 CSP 字符串里取出某条指令的取值。
fn csp_directive(csp: &str, name: &str) -> String {
    csp.split(';')
        .map(str::trim)
        .find(|part| part.starts_with(&format!("{name} ")) || *part == name)
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// 配置面
// ---------------------------------------------------------------------------

#[test]
fn asset_protocol_is_enabled() {
    let conf = tauri_conf();
    assert_eq!(
        conf["app"]["security"]["assetProtocol"]["enable"],
        serde_json::json!(true),
        "asset 协议必须启用，否则图片全部无法显示"
    );
}

#[test]
fn scope_uses_recursive_glob_covering_nested_paths() {
    let patterns = scope_patterns();
    assert!(
        patterns.iter().any(|p| p == "$APPDATA/images/**"),
        "scope 必须用 ** 递归通配 —— 单层 * 只匹配 images/ 下一层，\
         放进子目录的图片会全部裂图。当前 scope：{patterns:?}"
    );
    assert!(
        !patterns.iter().any(|p| p == "$APPDATA/images/*"),
        "scope 不得退回单层 * —— 那正是任务书怀疑的「只匹配一层」：{patterns:?}"
    );
}

#[test]
fn scope_does_not_widen_beyond_the_images_directory() {
    for pattern in scope_patterns() {
        assert!(
            !pattern.starts_with("$APPDATA/**") && pattern != "$APPDATA",
            "scope 不得放宽到 images 目录之外：{pattern}"
        );
        assert!(
            !pattern.starts_with("$APPDATA/*"),
            "scope 不得放宽到 images 目录之外：{pattern}"
        );
    }
}

// ---------------------------------------------------------------------------
// 行为面：用 Tauri 自己的 scope 判定
// ---------------------------------------------------------------------------

/// 造一个临时根目录，作为 scope 里 `$APPDATA` 的替身。
///
/// **为什么不用 mock app 的 `app_data_dir()`**：mock_context 用的 identifier 是真实
/// 应用的 identifier，它解析出的 app_data_dir 就落在 `~/Library/Application Support/`
/// 底下 —— 测试往那里写文件，等于往用户的真实数据目录里写垃圾
/// （第一版就踩了这个坑，测试直接覆盖了那个目录下的同名文件）。
/// 换成临时根目录后，glob 模式与真实配置完全同构，只是根不同。
fn temp_root(tag: &str) -> PathBuf {
    let raw = std::env::temp_dir().join(format!("ew_scope_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&raw);
    fs::create_dir_all(&raw).unwrap();
    // canonicalize：is_allowed 会把被测路径 canonicalize 后再匹配，
    // 模式侧必须用同一个真实路径，否则 macOS 上 /tmp → /private/tmp 的软链会让匹配落空。
    fs::canonicalize(&raw).unwrap()
}

/// 用临时根目录构造与真实配置同构的 scope 模式。
fn scoped_fs_scope(root: &Path) -> FsScope {
    let patterns: Vec<PathBuf> = asset_scope()
        .allowed_paths()
        .iter()
        .map(|p| {
            let s = p.to_string_lossy();
            PathBuf::from(s.replace("$APPDATA", &root.to_string_lossy()))
        })
        .collect();
    FsScope::AllowedPaths(patterns)
}

#[test]
fn real_scope_allows_images_at_any_nesting_depth() {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app 构建失败");
    let root = temp_root("allow");
    let scope = Scope::new(&app, &scoped_fs_scope(&root)).expect("Scope 构建失败");

    let nested = root.join("images").join("2026").join("ch01").join("pic.png");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::write(&nested, b"png").unwrap();

    assert!(
        scope.is_allowed(&nested),
        "scope 必须允许 images 下的嵌套图片：{}",
        nested.display()
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn real_scope_rejects_files_outside_the_images_directory() {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app 构建失败");
    let root = temp_root("outside");
    let scope = Scope::new(&app, &scoped_fs_scope(&root)).expect("Scope 构建失败");

    let outside = root.join("easy-writing.db");
    fs::write(&outside, b"db").unwrap();

    assert!(
        !scope.is_allowed(&outside),
        "images 之外的路径不得被 asset 协议放行：{}",
        outside.display()
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn traversal_out_of_images_dir_is_rejected_by_scope() {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app 构建失败");
    let root = temp_root("traversal");
    let scope = Scope::new(&app, &scoped_fs_scope(&root)).expect("Scope 构建失败");

    let secret = root.join("secret.txt");
    fs::write(&secret, b"secret").unwrap();
    let images_dir = root.join("images");
    fs::create_dir_all(&images_dir).unwrap();

    // images/../secret.txt 规范化后落在 images 之外，必须被拒。
    let traversal = images_dir.join("..").join("secret.txt");
    assert!(
        !scope.is_allowed(&traversal),
        "images/../secret.txt 规范化后已越出 images，必须被拒：{}",
        traversal.display()
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tauri_rejects_parent_dir_components_in_asset_paths() {
    // asset 协议在解析请求路径时先过 SafePathBuf：含 ParentDir 分量直接拒绝。
    let traversal = PathBuf::from("/tmp/appdata/images/../../etc/passwd");
    assert!(
        SafePathBuf::new(traversal).is_err(),
        "含 ../ 的路径必须在进入 asset 协议之前就被拒绝"
    );
}

// ---------------------------------------------------------------------------
// CSP
// ---------------------------------------------------------------------------

#[test]
fn csp_is_declared_and_allows_asset_images() {
    let conf = tauri_conf();
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("CSP 不得为 null —— 任务书怀疑的第二个点");

    assert!(
        csp_directive(csp, "default-src").contains("'self'"),
        "default-src 必须含 'self'：{csp}"
    );

    let img_src = csp_directive(csp, "img-src");
    assert!(
        img_src.contains("asset:"),
        "img-src 必须放行 asset:，否则正文插图全部裂图：{img_src}"
    );
    assert!(
        img_src.contains("http://asset.localhost"),
        "img-src 必须放行 http://asset.localhost —— Windows 下 asset:// 走这个源：{img_src}"
    );

    assert!(
        csp_directive(csp, "connect-src").contains("ipc:"),
        "connect-src 必须放行 ipc:，否则前端调不动后端命令：{csp}"
    );
}

#[test]
fn dev_csp_is_declared_and_allows_asset_images() {
    let conf = tauri_conf();
    let csp = conf["app"]["security"]["devCsp"]
        .as_str()
        .expect("devCsp 不得为 null");

    let img_src = csp_directive(csp, "img-src");
    assert!(
        img_src.contains("asset:") && img_src.contains("http://asset.localhost"),
        "devCsp 的 img-src 同样必须放行 asset 源：{img_src}"
    );
}

// ---------------------------------------------------------------------------
// 落盘面：应用自己生成的路径必须始终落在 scope 之内
// ---------------------------------------------------------------------------

#[test]
fn saved_images_use_flat_hashed_paths_inside_the_scope() {
    use easy_writing_lib::{asset, db};

    let dir = std::env::temp_dir().join(format!("ew_asset_scope_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "A", "").unwrap().id;

    let rel = asset::save_image(&conn, &dir, &novel, "pic.png", "image/png", "aGVsbG8=")
        .expect("保存图片失败");

    assert!(
        rel.starts_with("images/"),
        "落盘相对路径必须落在 images/ 下：{rel}"
    );
    assert!(
        !rel.contains(".."),
        "落盘相对路径不得含 ../ —— 那会指向 scope 之外：{rel}"
    );

    let file_name = rel.strip_prefix("images/").unwrap();
    let stem = file_name.split('.').next().unwrap();
    assert_eq!(
        stem.len(),
        64,
        "文件名应是内容哈希（sha256 十六进制 64 位）：{file_name}"
    );
    assert!(
        stem.chars().all(|c| c.is_ascii_hexdigit()),
        "文件名应是内容哈希（sha256 十六进制 64 位）：{file_name}"
    );

    assert!(
        dir.join(&rel).exists(),
        "相对路径必须真实落在 app_data_dir 之下：{rel}"
    );

    let _ = fs::remove_dir_all(&dir);
}
