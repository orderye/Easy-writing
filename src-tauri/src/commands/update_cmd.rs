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
pub fn parse_semver(ver: &str) -> (u64, u64, u64) {
    let clean = ver.trim().trim_start_matches(|c| c == 'v' || c == 'V');
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
        })
        .collect();

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
}
