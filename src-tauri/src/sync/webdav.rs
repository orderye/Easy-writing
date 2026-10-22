use reqwest::{Client, Method, StatusCode};
use shared_types::BackupFileInfo;
use std::time::Duration;

/// 一次「允许文件不存在」的远端读取结果。
///
/// 三态而不是 `Result<Option<Vec<u8>>>`：后者无法区分
/// 「服务端说不存在」与「我们没能问到」，而这个区别决定了
/// 恢复流程能否走历史备份兼容分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteFetch {
    Found(Vec<u8>),
    NotFound,
}

#[derive(Clone, Debug)]
pub struct WebDavClient {
    client: Client,
    base_url: String,
    username: String,
    password: String,
}

impl WebDavClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        let trimmed_url = base_url.trim_end_matches('/').to_string();

        Self {
            client,
            base_url: trimmed_url,
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    fn build_url(&self, remote_path: &str) -> String {
        let clean_path = remote_path.trim_start_matches('/');
        if clean_path.is_empty() {
            self.base_url.clone()
        } else {
            format!("{}/{}", self.base_url, clean_path)
        }
    }

    pub async fn test_connection(&self) -> Result<(), String> {
        let url = self.build_url("");
        let propfind_method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;

        let res = self
            .client
            .request(propfind_method, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "0")
            .send()
            .await
            .map_err(|e| format!("无法连接至 WebDAV 服务器: {}", e))?;

        let status = res.status();
        if status.is_success() || status == StatusCode::MULTI_STATUS || status == StatusCode::METHOD_NOT_ALLOWED {
            Ok(())
        } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            Err("WebDAV 鉴权失败：用户名或密码错误".to_string())
        } else {
            Err(format!("WebDAV 服务器响应异常 (HTTP {})", status))
        }
    }

    pub async fn ensure_dir(&self, remote_dir: &str) -> Result<(), String> {
        let clean_dir = remote_dir.trim_matches('/');
        if clean_dir.is_empty() {
            return Ok(());
        }

        // Recursively or iteratively create directories
        let parts: Vec<&str> = clean_dir.split('/').collect();
        let mut current_path = String::new();

        for part in parts {
            if current_path.is_empty() {
                current_path = part.to_string();
            } else {
                current_path = format!("{}/{}", current_path, part);
            }

            let url = self.build_url(&current_path);
            let mkcol_method = Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;

            let res = self
                .client
                .request(mkcol_method, &url)
                .basic_auth(&self.username, Some(&self.password))
                .send()
                .await
                .map_err(|e| format!("创建远程目录失败 ({}): {}", current_path, e))?;

            let status = res.status();
            // 201 Created, 405 Method Not Allowed (already exists), 301/200 OK
            if !status.is_success() && status != StatusCode::METHOD_NOT_ALLOWED {
                // If it already exists, PROPFIND check
                if !self.file_exists(&current_path).await.unwrap_or(false) {
                    return Err(format!("无法创建远程目录 {}: HTTP {}", current_path, status));
                }
            }
        }

        Ok(())
    }

    pub async fn file_exists(&self, remote_path: &str) -> Result<bool, String> {
        let url = self.build_url(remote_path);
        let propfind_method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;

        let res = self
            .client
            .request(propfind_method, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "0")
            .send()
            .await;

        match res {
            Ok(r) => {
                let status = r.status();
                if status.is_success() || status == StatusCode::MULTI_STATUS {
                    Ok(true)
                } else if status == StatusCode::NOT_FOUND {
                    Ok(false)
                } else {
                    // Try HEAD request fallback
                    let head_res = self
                        .client
                        .head(&url)
                        .basic_auth(&self.username, Some(&self.password))
                        .send()
                        .await;
                    if let Ok(hr) = head_res {
                        Ok(hr.status().is_success())
                    } else {
                        Ok(false)
                    }
                }
            }
            Err(_) => Ok(false),
        }
    }

    pub async fn upload_file(&self, remote_path: &str, data: Vec<u8>) -> Result<(), String> {
        let url = self.build_url(remote_path);

        let res = self
            .client
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .body(data)
            .send()
            .await
            .map_err(|e| format!("上传文件至 WebDAV 失败 ({}): {}", remote_path, e))?;

        let status = res.status();
        if status.is_success() || status == StatusCode::NO_CONTENT || status == StatusCode::CREATED {
            Ok(())
        } else {
            let err_text = res.text().await.unwrap_or_default();
            Err(format!("WebDAV 上传失败 (HTTP {}): {}", status, err_text))
        }
    }

    pub async fn download_file(&self, remote_path: &str) -> Result<Vec<u8>, String> {
        let url = self.build_url(remote_path);

        let res = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| format!("从 WebDAV 下载文件失败 ({}): {}", remote_path, e))?;

        let status = res.status();
        if !status.is_success() {
            return Err(format!("WebDAV 下载失败: HTTP {}", status));
        }

        let bytes = res
            .bytes()
            .await
            .map_err(|e| format!("读取文件内容流失败: {}", e))?;

        Ok(bytes.to_vec())
    }

    /// 下载一个**允许不存在**的文件。
    ///
    /// ## 为什么不能直接用 `download_file` + `file_exists`
    ///
    /// 恢复流程需要区分三种情况，而 `download_file` 只能表达两种：
    ///
    /// 1. 文件存在且下载成功 → `Found`
    /// 2. 服务端明确说「不存在」（404）→ `NotFound`，这是历史备份的兼容路径
    /// 3. 网络中断、401/403、5xx、超时 → `Err`
    ///
    /// 把 3 折叠进 2 是危险的：一次网络抖动会让「有清单但没读到」的备份
    /// 被当成「历史无清单备份」，从而跳过全部清单校验直接覆盖数据库。
    /// `file_exists` 也帮不上忙 —— 它在网络错误时返回 `Ok(false)`，
    /// 同样是把「不知道」当成「不存在」。
    pub async fn download_file_optional(&self, remote_path: &str) -> Result<RemoteFetch, String> {
        let url = self.build_url(remote_path);

        let res = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| format!("从 WebDAV 下载文件失败 ({}): {}", remote_path, e))?;

        let status = res.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(RemoteFetch::NotFound);
        }
        if !status.is_success() {
            // 401 / 403 / 405 / 5xx 一律不是「文件不存在」。
            return Err(format!(
                "WebDAV 下载失败 ({}): HTTP {}（这不是「文件不存在」，不能据此判定为历史备份）",
                remote_path, status
            ));
        }

        let bytes = res
            .bytes()
            .await
            .map_err(|e| format!("读取文件内容流失败: {}", e))?;

        Ok(RemoteFetch::Found(bytes.to_vec()))
    }

    /// 列出远端目录下的**全部**文件（含清单 sidecar、meta sidecar 等）。
    ///
    /// 与 [`WebDavClient::list_files`] 的区别：后者只返回 `.db.gz` 备份本体，
    /// 而跨设备保护需要看到备份旁边的 `.meta.json`（这份备份是谁写的），
    /// 保留策略预览需要知道目录里还有多少「不属于本应用策略范围」的文件。
    pub async fn list_all_files(&self, remote_dir: &str) -> Result<Vec<BackupFileInfo>, String> {
        let url = self.build_url(remote_dir);
        let propfind_method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;

        let res = self
            .client
            .request(propfind_method, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(
                r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getcontentlength/>
    <D:getlastmodified/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#,
            )
            .send()
            .await
            .map_err(|e| format!("拉取远程文件列表失败: {}", e))?;

        let status = res.status();
        if !status.is_success() && status != StatusCode::MULTI_STATUS {
            return Err(format!("WebDAV 列举文件响应失败: HTTP {}", status));
        }

        let xml_text = res.text().await.unwrap_or_default();
        let items = parse_webdav_multistatus(&xml_text);
        Ok(items)
    }

    /// 列出远端目录下的备份本体（`.db.gz`），按文件名倒序（最新在前）。
    pub async fn list_files(&self, remote_dir: &str) -> Result<Vec<BackupFileInfo>, String> {
        Ok(self
            .list_all_files(remote_dir)
            .await?
            .into_iter()
            .filter(|b| b.filename.ends_with(".db.gz"))
            .collect())
    }

    pub async fn get_file_size(&self, remote_path: &str) -> Result<u64, String> {
        let url = self.build_url(remote_path);
        if let Ok(res) = self
            .client
            .head(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
        {
            if res.status().is_success() {
                if let Some(cl) = res.headers().get(reqwest::header::CONTENT_LENGTH) {
                    if let Ok(s) = cl.to_str() {
                        if let Ok(len) = s.parse::<u64>() {
                            return Ok(len);
                        }
                    }
                }
            }
        }

        // Fallback: PROPFIND Depth: 0
        let propfind_method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
        let res = self
            .client
            .request(propfind_method, &url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "0")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(r#"<?xml version="1.0" encoding="utf-8" ?><D:propfind xmlns:D="DAV:"><D:prop><D:getcontentlength/></D:prop></D:propfind>"#)
            .send()
            .await
            .map_err(|e| format!("PROPFIND 请求失败 ({}): {}", remote_path, e))?;

        let text = res.text().await.unwrap_or_default();
        let length_str = extract_tag_value(&text, "getcontentlength");
        length_str
            .trim()
            .parse::<u64>()
            .map_err(|_| "无法获取远程文件大小".to_string())
    }

    pub async fn move_file(&self, src_path: &str, dest_path: &str) -> Result<(), String> {
        let src_url = self.build_url(src_path);
        let dest_url = self.build_url(dest_path);
        let move_method = Method::from_bytes(b"MOVE").map_err(|e| e.to_string())?;

        let res = self
            .client
            .request(move_method, &src_url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Destination", dest_url)
            .header("Overwrite", "T")
            .send()
            .await
            .map_err(|e| format!("移动 WebDAV 文件失败 ({} -> {}): {}", src_path, dest_path, e))?;

        let status = res.status();
        if status.is_success() || status == StatusCode::CREATED || status == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            let err_text = res.text().await.unwrap_or_default();
            Err(format!("WebDAV MOVE 失败 (HTTP {}): {}", status, err_text))
        }
    }

    pub async fn delete_file(&self, remote_path: &str) -> Result<(), String> {
        let url = self.build_url(remote_path);

        let res = self
            .client
            .delete(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| format!("删除 WebDAV 文件失败 ({}): {}", remote_path, e))?;

        let status = res.status();
        if status.is_success() || status == StatusCode::NO_CONTENT || status == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            let err_text = res.text().await.unwrap_or_default();
            Err(format!("WebDAV DELETE 失败 (HTTP {}): {}", status, err_text))
        }
    }
}

/// Parse WebDAV XML Multi-Status response
fn parse_webdav_multistatus(xml: &str) -> Vec<BackupFileInfo> {
    let mut results = Vec::new();

    for block in xml.split("</D:response>").flat_map(|s| s.split("</d:response>")).flat_map(|s| s.split("</response>")) {
        // Is collection / directory?
        if block.contains("<D:collection") || block.contains("<d:collection") || block.contains("<collection") {
            continue;
        }

        let href = extract_tag_value(block, "href");
        if href.is_empty() {
            continue;
        }

        // Filename from href
        let filename = href
            .trim_end_matches('/')
            .split('/')
            .last()
            .unwrap_or_default()
            .to_string();

        if filename.is_empty() {
            continue;
        }

        let length_str = extract_tag_value(block, "getcontentlength");
        let size_bytes: u64 = length_str.trim().parse().unwrap_or(0);

        let last_modified = extract_tag_value(block, "getlastmodified");

        results.push(BackupFileInfo {
            filename,
            size_bytes,
            modified_at: last_modified,
        });
    }

    // Sort newest first
    results.sort_by(|a, b| b.filename.cmp(&a.filename));
    results
}

fn extract_tag_value(block: &str, tag: &str) -> String {
    let patterns = [
        format!("<{}>", tag),
        format!("<D:{}>", tag),
        format!("<d:{}>", tag),
    ];

    for open_tag in &patterns {
        if let Some(start_idx) = block.find(open_tag) {
            let content_start = start_idx + open_tag.len();
            if let Some(end_rel) = block[content_start..].find('<') {
                return block[content_start..content_start + end_rel].trim().to_string();
            }
        }
    }

    String::new()
}
