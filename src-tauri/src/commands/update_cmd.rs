use crate::db::DbPool;
use reqwest::header::USER_AGENT;
use serde::Deserialize;
use shared_types::{AppUpdateAsset, AppUpdateInfo};
use tauri::State;

#[derive(Deserialize)]
struct GithubReleaseAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GithubReleaseResponse {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    prerelease: Option<bool>,
    assets: Option<Vec<GithubReleaseAsset>>,
}

/// 解析类似 "v1.2.3-beta" 或 "0.1.0" 为三元组 (major, minor, patch)
pub fn parse_semver(ver: &str) -> (u64, u64, u64) {    let clean = ver.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let main_part = clean.split('-').next().unwrap_or(clean);
    let mut parts = main_part.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// 比较 latest 版本是否比 current 新
pub fn is_version_newer(latest: &str, current: &str) -> bool {
    let latest_parts = parse_semver(latest);
    let current_parts = parse_semver(current);
    latest_parts > current_parts
}

/// SHA256SUMS.txt 中记录校验和的资产文件名（大小写不敏感）。
pub const SHA256SUMS_NAME: &str = "sha256sums.txt";

/// 解析 `SHA256SUMS.txt` 的内容为「文件名（小写）→ sha256」。
///
/// 行格式与 `sha256sum` / `shasum -a 256` 的默认输出一致：
/// `<64位十六进制> <空格*><可选 '*'><文件名>`，`#` 开头的行是注释。
/// 解析对格式异常宽容：不合式的行直接跳过，绝不 panic，
/// 也不让「校验和文件写得难看」阻断更新检查。
pub fn parse_sha256sums(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // 分割：<hash> 与其余部分；hash 必须是 64 位十六进制才可信
        let Some(space_pos) = line.find(' ') else {
            continue;
        };
        let (hash_raw, rest) = line.split_at(space_pos);
        let hash = hash_raw.trim().to_ascii_lowercase();
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        // rest 形如 "  filename" 或 " *filename"（二进制模式标记）
        let name = rest.trim_start().trim_start_matches('*').trim();
        if name.is_empty() {
            continue;
        }
        map.insert(name.to_ascii_lowercase(), hash);
    }
    map
}

/// 给资产列表附加校验和。匹配按文件名（大小写不敏感）；
/// `SHA256SUMS.txt` 自身与未命中的资产不带哈希（前端对空值不展示）。
pub fn attach_sha256_to_assets(
    mut assets: Vec<AppUpdateAsset>,
    sums: &std::collections::HashMap<String, String>,
) -> Vec<AppUpdateAsset> {
    for asset in assets.iter_mut() {
        if asset.name.to_ascii_lowercase() == SHA256SUMS_NAME {
            continue;
        }
        if let Some(hash) = sums.get(&asset.name.to_ascii_lowercase()) {
            asset.sha256 = hash.clone();
        }
    }
    assets
}

#[tauri::command(rename_all = "snake_case")]
pub async fn check_app_update(
    pool: State<'_, DbPool>,
    force: bool,
) -> Result<AppUpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let conn = pool.get().map_err(|e| e.to_string())?;
    let custom_url = crate::db::app_setting::get_setting(&conn, "update_check_url").unwrap_or(None);
    let ignored_version = crate::db::app_setting::get_setting(&conn, "ignored_update_version").unwrap_or(None);
    drop(conn);

    let check_url = custom_url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "https://api.github.com/repos/orderye/Easy-writing/releases/latest".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .get(&check_url)
        .header(USER_AGENT, format!("Easy-writing-Desktop/{}", current_version))
        .send()
        .await
        .map_err(|e| format!("无法连接至版本更新源: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("检查更新失败，服务器返回 HTTP {}", resp.status()));
    }

    let release = resp
        .json::<GithubReleaseResponse>()
        .await
        .map_err(|e| format!("解析版本信息失败: {}", e))?;

    let clean_latest = release.tag_name.trim().trim_start_matches(|c| c == 'v' || c == 'V').to_string();
    let is_newer = is_version_newer(&clean_latest, &current_version);

    // 如果非强制检测且用户忽略了此版本，则将 has_update 视为 false
    let has_update = if !force {
        if let Some(ignored) = ignored_version {
            if ignored == clean_latest {
                false
            } else {
                is_newer
            }
        } else {
            is_newer
        }
    } else {
        is_newer
    };

    let release_name = release.name.unwrap_or_else(|| format!("Easy-writing v{}", clean_latest));
    let release_notes = release.body.unwrap_or_else(|| "暂无版本更新说明。".to_string());
    let published_at = release.published_at.unwrap_or_default();
    let is_prerelease = release.prerelease.unwrap_or(false);

    let assets: Vec<AppUpdateAsset> = release
        .assets
        .unwrap_or_default()
        .into_iter()
        .map(|a| AppUpdateAsset {
            name: a.name,
            size: a.size,
            browser_download_url: a.browser_download_url,
            sha256: String::new(),
        })
        .collect();

    // 校验和（任务书 T13）：Release 里有 SHA256SUMS.txt 就拉下来，
    // 按文件名给每个安装包附上 SHA-256。拉取或解析失败都静默降级 ——
    // 哈希展示是附加价值，不能因为它挡住更新检查本身。
    let assets = if assets
        .iter()
        .any(|a| a.name.to_ascii_lowercase() == SHA256SUMS_NAME)
    {
        let sums_asset = assets
            .iter()
            .find(|a| a.name.to_ascii_lowercase() == SHA256SUMS_NAME)
            .cloned()
            .unwrap();
        match client
            .get(&sums_asset.browser_download_url)
            .header(USER_AGENT, format!("Easy-writing-Desktop/{}", current_version))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(content) => attach_sha256_to_assets(assets, &parse_sha256sums(&content)),
                Err(_) => assets,
            },
            _ => assets,
        }
    } else {
        assets
    };

    // 默认下载链接优先取匹配当前操作系统的 asset，若无则使用 release.html_url
    let download_url = assets
        .iter()
        .find(|a| {
            #[cfg(target_os = "macos")]
            {
                a.name.ends_with(".dmg") || a.name.ends_with(".app.tar.gz")
            }
            #[cfg(target_os = "windows")]
            {
                a.name.ends_with(".msi") || a.name.ends_with(".exe")
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                a.name.ends_with(".tar.gz") || a.name.ends_with(".AppImage")
            }
        })
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_else(|| release.html_url.clone());

    // 记录最近检查更新时间
    if let Ok(conn) = pool.get() {
        let now_str = chrono::Utc::now().to_rfc3339();
        let _ = crate::db::app_setting::set_setting(&conn, "last_update_check_time", &now_str);
    }

    Ok(AppUpdateInfo {
        current_version,
        latest_version: clean_latest,
        has_update,
        release_name,
        release_notes,
        published_at,
        html_url: release.html_url,
        download_url,
        assets,
        is_prerelease,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("仅支持打开 http/https 协议的链接".to_string());
    }

    // 协议校验不够：只限制 `http(s)://` 仍允许打开内网地址，
    // 例如云元数据服务 `http://169.254.169.254/...` 或路由器管理页，
    // 等于给前端一个「让系统浏览器访问本机/内网」的入口。
    //
    // 这里额外拒绝明显的本机与私有网段目标。更新链接应当永远指向公网
    // 仓库地址，没有任何正当理由需要跳转到 127.0.0.1 或局域网 IP。
    if let Some(host) = extract_host(trimmed) {
        if is_local_or_private_host(&host) {
            return Err(format!("出于安全考虑，不允许打开指向本机或内网的链接：{host}"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("打开系统浏览器失败: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", trimmed])
            .spawn()
            .map_err(|e| format!("打开系统浏览器失败: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("打开系统浏览器失败: {}", e))?;
    }

    Ok(())
}

/// 从 URL 中取出 host 部分。
fn extract_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    // 去掉 userinfo、path、query、fragment
    let after_auth = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
    let host_part = after_auth
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    // 处理 IPv6 字面量 [::1]:8080
    let host = if let Some(stripped) = host_part.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("").to_string()
    } else {
        host_part
            .split(':')
            .next()
            .unwrap_or("")
            .to_string()
    };
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// 是否指向本机或私有网段。
fn is_local_or_private_host(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") || host == "::1" || host == "0.0.0.0" {
        return true;
    }
    // 云厂商元数据端点
    if host == "169.254.169.254" || host == "metadata.google.internal" {
        return true;
    }
    // 解析失败时保守放过（真正的拦截交给协议与浏览器），
    // 但能解析出的私有网段一律拒绝。
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
                || v4.is_multicast()
        }
        Ok(std::net::IpAddr::V6(v6)) => {
            v6.is_loopback() || v6.is_unspecified() || v6.is_multicast()
        }
        Err(_) => false,
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_ignored_update_version(pool: State<DbPool>, version: String) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    crate::db::app_setting::set_setting(&conn, "ignored_update_version", &version)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparisons() {
        assert!(is_version_newer("0.2.0", "0.1.0"));
        assert!(is_version_newer("1.0.0", "0.1.0"));
        assert!(is_version_newer("v1.0.0", "0.1.0"));
        assert!(is_version_newer("0.1.1", "0.1.0"));
        assert!(is_version_newer("v0.1.1-alpha", "0.1.0"));
        
        assert!(!is_version_newer("0.1.0", "0.1.0"));
        assert!(!is_version_newer("v0.1.0", "0.1.0"));
        assert!(!is_version_newer("0.0.9", "0.1.0"));
        assert!(!is_version_newer("0.0.99", "0.1.0"));
    }

    #[test]
    fn external_url_blocks_local_and_private_targets() {
        // 云元数据端点：最高危的一个
        assert!(is_local_or_private_host("169.254.169.254"));
        assert!(is_local_or_private_host("metadata.google.internal"));
        // 本机与环回
        assert!(is_local_or_private_host("localhost"));
        assert!(is_local_or_private_host("127.0.0.1"));
        assert!(is_local_or_private_host("::1"));
        // 私有网段
        assert!(is_local_or_private_host("192.168.1.1"));
        assert!(is_local_or_private_host("10.0.0.5"));
        assert!(is_local_or_private_host("172.16.0.1"));
        // 正常公网地址必须放行，否则功能会被误伤
        assert!(!is_local_or_private_host("github.com"));
        assert!(!is_local_or_private_host("api.github.com"));
        assert!(!is_local_or_private_host("8.8.8.8"));
    }

    #[test]
    fn extract_host_handles_ports_and_ipv6() {
        assert_eq!(extract_host("https://github.com/x/y").as_deref(), Some("github.com"));
        assert_eq!(extract_host("http://127.0.0.1:8080/a").as_deref(), Some("127.0.0.1"));
        assert_eq!(extract_host("https://user:pw@example.com/p").as_deref(), Some("example.com"));
        assert_eq!(extract_host("http://[::1]:9000/x").as_deref(), Some("::1"));
        assert_eq!(extract_host("not-a-url"), None);
    }

    // ---- SHA256SUMS 解析（任务书 T13）----

    const SUMS_SAMPLE: &str = "\
# generated by release.yml
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  Easy-writing_0.3.8_aarch64.dmg
fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *Easy-writing_0.3.8_windows_x64-setup.exe
bad-line
zz11111111111111111111111111111111111111111111111111111111111111  broken-hash.txt
short  not-enough.txt
";

    #[test]
    fn sha256sums_parses_standard_lines_and_tolerates_noise() {
        let map = parse_sha256sums(SUMS_SAMPLE);
        assert_eq!(map.len(), 2, "只应解析出两行合式记录，实际 {map:?}");
        assert_eq!(
            map.get("easy-writing_0.3.8_aarch64.dmg").map(String::as_str),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        // '*' 二进制模式标记被剥掉
        assert_eq!(
            map.get("easy-writing_0.3.8_windows_x64-setup.exe").map(String::as_str),
            Some("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
        );
    }

    #[test]
    fn sha256sums_matching_is_case_insensitive() {
        let map = parse_sha256sums("abcDEF0123456789abcDEF0123456789abcDEF0123456789abcDEF0123456789  Package.DMG");
        assert!(map.contains_key("package.dmg"));
    }

    #[test]
    fn sha256sums_attach_skips_sums_file_and_misses() {
        let map = parse_sha256sums(SUMS_SAMPLE);
        let assets = vec![
            AppUpdateAsset { name: "Easy-writing_0.3.8_aarch64.dmg".into(), size: 1, browser_download_url: "u".into(), sha256: String::new() },
            AppUpdateAsset { name: "SHA256SUMS.txt".into(), size: 1, browser_download_url: "u".into(), sha256: String::new() },
            AppUpdateAsset { name: "unknown.bin".into(), size: 1, browser_download_url: "u".into(), sha256: String::new() },
        ];
        let attached = attach_sha256_to_assets(assets, &map);
        assert!(!attached[0].sha256.is_empty(), "命中的资产必须拿到哈希");
        assert!(attached[1].sha256.is_empty(), "SHA256SUMS.txt 自身不附加哈希");
        assert!(attached[2].sha256.is_empty(), "未命中的资产保持为空，前端不展示");
    }
}
