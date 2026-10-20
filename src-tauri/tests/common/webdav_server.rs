//! 测试专用的进程内 WebDAV 服务端。
//!
//! ## 为什么不用 mock
//!
//! `WebDavClient` 的行为全部体现在**真实 HTTP 报文**上：PROPFIND 的 Depth 头、
//! MOVE 的 Destination 头、HEAD 的 Content-Length、404 与 5xx 的区别。
//! 把这些替换成 trait mock 之后，测试验证的是 mock 自己的行为，
//! 而不是客户端与服务端之间的契约 —— 而后者恰恰是过去从未被验证过的部分。
//!
//! 这里起一个真实的 `TcpListener`，按 HTTP/1.1 解析请求、按 WebDAV 语义应答，
//! 后端是一个内存文件系统。`WebDavClient` 完全不知道自己在被测试。
//!
//! ## 这不是「真实 WebDAV 实现」
//!
//! 必须说清楚：本服务端只为覆盖 `WebDavClient` 实际发出的那些请求而实现，
//! 它不能证明与 Nextcloud / 坚果云等真实服务的兼容性。
//! 那部分由 `tests/webdav_real_server.rs`（env 门控）与 CI 中的容器 job 负责。
//!
//! ## 支持的方法
//!
//! PROPFIND（Depth 0 / 1）、GET、HEAD、PUT、MOVE、DELETE、MKCOL。
//! 认证方式为 HTTP Basic；每个请求处理完立即关闭连接（`Connection: close`）。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// 受控故障注入。全部默认关闭 —— 服务端默认行为是「正常工作」。
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// MOVE 一律失败（返回 507）。
    pub fail_move: bool,
    /// PUT 一律失败（返回 500）。
    pub fail_put: bool,
    /// 读完 PUT 请求体后直接断开连接、不应答 —— 模拟上传中断。
    pub abort_during_put: bool,
    /// GET 时把响应体截断到指定字节数（Content-Length 同步改小，
    /// 传输本身是干净的，但内容是残缺的）—— 模拟服务端存了半截文件。
    pub truncate_get_to: Option<usize>,
    /// GET 时先发一半响应体再断开 —— 模拟下载中途断线。
    pub abort_during_get: bool,
    /// HEAD 返回 405（服务端不支持 HEAD）。
    pub refuse_head: bool,
    /// 所有请求返回 401 —— 模拟凭据错误或被吊销权限。
    pub unauthorized: bool,
    /// DELETE 返回 500。
    pub fail_delete: bool,
    /// 路径以该后缀结尾的 PUT 返回 500（例如 `.json` = 清单上传失败，
    /// `.png` = 图片上传失败），其余 PUT 正常。
    pub fail_put_suffix: Option<String>,
    /// 对 `.json` 的 GET 返回该状态码（例如 500）。
    ///
    /// 用来验证「清单读取失败」不会被误判成「清单不存在」：
    /// 只有服务端明确返回 404 才允许走历史备份兼容分支。
    pub manifest_get_status: Option<u16>,
    /// HEAD 报告的 Content-Length 强制为该值 —— 模拟服务端返回错误大小。
    pub head_content_length: Option<u64>,
    /// PROPFIND 响应中不含 `getcontentlength`。
    ///
    /// 与 `refuse_head` 组合使用即可模拟「服务端既不支持 HEAD、
    /// 也无法通过 PROPFIND 给出大小」—— 此时客户端必须承认「无法确认」，
    /// 而不是把跳过校验当成校验通过。
    pub propfind_omit_content_length: bool,
}

#[derive(Debug, Default)]
struct Store {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
    faults: Faults,
    /// 服务端真实收到的请求计数，按方法名统计，供测试断言「确实走了 HTTP」。
    hits: BTreeMap<String, usize>,
}

#[derive(Clone)]
pub struct TestWebDav {
    state: Arc<Mutex<Store>>,
    port: u16,
    alive: Arc<AtomicBool>,
}

impl TestWebDav {
    /// 启动服务端。返回时已可接受连接。
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试端口失败");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(Store {
            dirs: BTreeSet::from(["/".to_string()]),
            ..Default::default()
        }));
        let alive = Arc::new(AtomicBool::new(true));

        let server_state = state.clone();
        let server_alive = alive.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                if !server_alive.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let st = server_state.clone();
                // 每连接一个线程：客户端可能并发发起请求，
                // 串行处理会造成假死锁。
                thread::spawn(move || {
                    let _ = handle(stream, &st);
                });
            }
        });

        TestWebDav { state, port, alive }
    }

    /// WebDAV 根地址（形如 `http://127.0.0.1:PORT/dav`）。
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/dav", self.port)
    }

    /// 把客户端视角的远端路径（相对于 `base_url`）换算成服务端存储键。
    ///
    /// 所有 `put_file` / `get_file` / `remove_file` / `file_names_in`
    /// 都必须用这个函数换算，否则会因为少了 `/dav` 前缀而静默匹配不到任何东西。
    pub fn path_for(&self, remote_path: &str) -> String {
        format!("/dav/{}", remote_path.trim_start_matches('/'))
    }

    pub fn set_faults(&self, f: Faults) {
        self.state.lock().unwrap().faults = f;
    }

    /// 直接往服务端塞一个文件（绕过 HTTP，用于构造初始状态）。
    pub fn put_file(&self, path: &str, bytes: Vec<u8>) {
        let mut st = self.state.lock().unwrap();
        let norm = normalize(path);
        if let Some(parent) = parent_of(&norm) {
            st.dirs.insert(parent);
        }
        st.files.insert(norm, bytes);
    }

    pub fn get_file(&self, path: &str) -> Option<Vec<u8>> {
        self.state
            .lock()
            .unwrap()
            .files
            .get(&normalize(path))
            .cloned()
    }

    /// 直接从服务端移除一个文件（绕过 HTTP，用于构造「远端素材缺失」等状态）。
    pub fn remove_file(&self, path: &str) {
        self.state.lock().unwrap().files.remove(&normalize(path));
    }

    /// 列出某个目录下的文件名（不含目录前缀）。
    ///
    /// 前缀必须带结尾斜杠再切掉，否则返回的名字会多一个 `/`，
    /// 拿去 `remove_file` 时会静默匹配不到任何文件 —— 这个坑真实踩过。
    pub fn file_names_in(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}/", normalize(dir));
        self.state
            .lock()
            .unwrap()
            .files
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(|k| k[prefix.len()..].to_string())
            .collect()
    }

    /// 某方法被真实请求过几次。用来证明测试确实经过了 HTTP 层。
    pub fn hits(&self, method: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .hits
            .get(method)
            .copied()
            .unwrap_or(0)
    }
}

impl Drop for TestWebDav {
    fn drop(&mut self) {
        // 让 accept 循环退出。监听 socket 随进程结束释放，
        // 这里只需保证不再处理新请求。
        self.alive.store(false, Ordering::SeqCst);
        // 主动连一次，唤醒阻塞在 incoming() 上的线程。
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn normalize(path: &str) -> String {
    let p = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let p = p.trim_end_matches('/');
    if p.is_empty() {
        "/".to_string()
    } else {
        p.to_string()
    }
}

fn parent_of(path: &str) -> Option<String> {
    let idx = path.rfind('/')?;
    let parent = if idx == 0 { "/" } else { &path[..idx] };
    Some(parent.to_string())
}

fn handle(mut stream: TcpStream, state: &Arc<Mutex<Store>>) -> std::io::Result<()> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1];
    // 逐字节读到空行为止：请求头很小，这样实现最不容易出错。
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        head.push(buf[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
        if head.len() > 64 * 1024 {
            return Ok(());
        }
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let mut lines = text.lines();
    let Some(request_line) = lines.next() else {
        return Ok(());
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();

    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    let body_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; body_len];
    if body_len > 0 {
        stream.read_exact(&mut body)?;
    }

    // 只取 URL 的路径部分（MOVE 的 Destination 是完整 URL）。
    let path = path_only(&raw_path);

    let response = {
        let mut st = state.lock().unwrap();
        *st.hits.entry(method.clone()).or_insert(0) += 1;
        let faults = st.faults.clone();

        if faults.unauthorized {
            respond(401, "Unauthorized", Vec::new(), &[])
        } else {
            match method.as_str() {
                "PROPFIND" => {
                    let depth = headers.get("depth").cloned().unwrap_or_else(|| "0".into());
                    propfind(&mut st, &path, &depth, &faults)
                }
                "GET" => get(&mut st, &path, &faults),
                "HEAD" => head_req(&mut st, &path, &faults),
                "PUT" => put(&mut st, &path, body, &faults),
                "MOVE" => {
                    let dest = headers
                        .get("destination")
                        .cloned()
                        .unwrap_or_default();
                    move_file(&mut st, &path, &path_only(&dest), &faults)
                }
                "DELETE" => delete(&mut st, &path, &faults),
                "MKCOL" => mkcol(&mut st, &path),
                other => respond(
                    405,
                    "Method Not Allowed",
                    format!("unsupported method {other}").into_bytes(),
                    &[],
                ),
            }
        }
    };

    stream.write_all(&response)?;
    stream.flush()?;
    // 显式关闭：客户端不应复用连接，避免测试之间的状态串扰。
    let _ = stream.shutdown(std::net::Shutdown::Write);
    Ok(())
}

fn path_only(url: &str) -> String {
    if let Some(rest) = url.split_once("://") {
        let after_host = rest.1.split('/').skip(1).collect::<Vec<_>>().join("/");
        return normalize(&format!("/{after_host}"));
    }
    normalize(url)
}

fn respond(status: u16, reason: &str, body: Vec<u8>, extra: &[(&str, String)]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} {reason}\r\n").into_bytes();
    out.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    out.extend_from_slice(b"Connection: close\r\n");
    for (k, v) in extra {
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&body);
    out
}

const LAST_MODIFIED: &str = "Wed, 01 Jan 2026 00:00:00 GMT";

fn propfind(st: &mut Store, path: &str, depth: &str, faults: &Faults) -> Vec<u8> {
    let norm = normalize(path);
    let is_dir = st.dirs.contains(&norm) || norm == "/";
    let is_file = st.files.contains_key(&norm);
    if !is_dir && !is_file {
        return respond(404, "Not Found", Vec::new(), &[]);
    }

    let mut entries: Vec<(String, Option<usize>)> = Vec::new();
    if is_dir {
        entries.push((format!("{norm}/"), None));
        if depth != "0" {
            let prefix = if norm == "/" {
                "/".to_string()
            } else {
                format!("{norm}/")
            };
            let child_dirs: Vec<String> = st
                .dirs
                .iter()
                .filter(|d| d.starts_with(&prefix) && *d != &norm && d.as_str() != "/")
                .filter(|d| d[prefix.len()..].split('/').count() == 1)
                .cloned()
                .collect();
            for d in child_dirs {
                entries.push((format!("{d}/"), None));
            }
            for (k, v) in st.files.iter() {
                if k.starts_with(&prefix) && k[prefix.len()..].split('/').count() == 1 {
                    entries.push((k.clone(), Some(v.len())));
                }
            }
        }
    } else {
        entries.push((norm.clone(), st.files.get(&norm).map(|v| v.len())));
    }

    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
"#,
    );
    for (href, len) in entries {
        xml.push_str("<D:response>\n");
        xml.push_str(&format!("<D:href>{href}</D:href>\n"));
        xml.push_str("<D:propstat>\n<D:prop>\n");
        let omit = faults.propfind_omit_content_length;
        match len {
            Some(n) => {
                xml.push_str("<D:resourcetype/>\n");
                if !omit {
                    xml.push_str(&format!("<D:getcontentlength>{n}</D:getcontentlength>\n"));
                }
            }
            None => {
                xml.push_str("<D:resourcetype><D:collection/></D:resourcetype>\n");
                if !omit {
                    xml.push_str("<D:getcontentlength>0</D:getcontentlength>\n");
                }
            }
        }
        xml.push_str(&format!(
            "<D:getlastmodified>{LAST_MODIFIED}</D:getlastmodified>\n"
        ));
        xml.push_str("</D:prop>\n<D:status>HTTP/1.1 200 OK</D:status>\n</D:propstat>\n");
        xml.push_str("</D:response>\n");
    }
    xml.push_str("</D:multistatus>\n");

    respond(
        207,
        "Multi-Status",
        xml.into_bytes(),
        &[("Content-Type", "application/xml; charset=utf-8".to_string())],
    )
}

fn get(st: &mut Store, path: &str, faults: &Faults) -> Vec<u8> {
    let norm = normalize(path);
    if let Some(code) = faults.manifest_get_status {
        if norm.ends_with(".json") {
            return respond(code, "Server Error", b"manifest unavailable".to_vec(), &[]);
        }
    }
    let Some(bytes) = st.files.get(&norm).cloned() else {
        return respond(404, "Not Found", Vec::new(), &[]);
    };
    if faults.abort_during_get {
        // 只发响应头与前一半内容，然后断开：客户端必须报传输错误，
        // 不能把半截内容当成完整文件。
        let half = bytes.len() / 2;
        let mut out = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len())
            .into_bytes();
        out.extend_from_slice(&bytes[..half]);
        return out;
    }
    let body = match faults.truncate_get_to {
        Some(n) if n < bytes.len() => bytes[..n].to_vec(),
        _ => bytes,
    };
    respond(200, "OK", body, &[])
}

fn head_req(st: &mut Store, path: &str, faults: &Faults) -> Vec<u8> {
    if faults.refuse_head {
        // 服务端不支持 HEAD 是真实存在的部署形态（部分反向代理会拦掉）。
        // 客户端此时必须走 PROPFIND 回退，且不能把「没校验」当成「校验通过」。
        return respond(405, "Method Not Allowed", Vec::new(), &[]);
    }
    let norm = normalize(path);
    let Some(bytes) = st.files.get(&norm) else {
        return respond(404, "Not Found", Vec::new(), &[]);
    };
    if let Some(bogus) = faults.head_content_length {
        let mut out = b"HTTP/1.1 200 OK\r\n".to_vec();
        out.extend_from_slice(format!("Content-Length: {bogus}\r\n").as_bytes());
        out.extend_from_slice(b"Connection: close\r\n\r\n");
        return out;
    }
    // HEAD 的响应体为空，但 Content-Length 必须报告**目标资源的真实大小** ——
    // 这正是客户端用来校验上传结果的值。不能复用 respond()，
    // 否则会出现两个互相矛盾的 Content-Length 头。
    let mut out = b"HTTP/1.1 200 OK\r\n".to_vec();
    out.extend_from_slice(format!("Content-Length: {}\r\n", bytes.len()).as_bytes());
    out.extend_from_slice(b"Connection: close\r\n\r\n");
    out
}

fn put(st: &mut Store, path: &str, body: Vec<u8>, faults: &Faults) -> Vec<u8> {
    if faults.abort_during_put {
        // 请求体已读完但不应答就断开 —— 模拟上传在最后一公里中断。
        // 关键点是服务端**不能留下一个被当成完整备份的半成品**。
        return Vec::new();
    }
    if faults.fail_put {
        return respond(500, "Internal Server Error", b"put failed".to_vec(), &[]);
    }
    if let Some(suffix) = &faults.fail_put_suffix {
        if path.ends_with(suffix) {
            return respond(
                500,
                "Internal Server Error",
                format!("rejected by fault: {suffix}").into_bytes(),
                &[],
            );
        }
    }
    let norm = normalize(path);
    if let Some(parent) = parent_of(&norm) {
        st.dirs.insert(parent);
    }
    st.files.insert(norm, body);
    respond(201, "Created", Vec::new(), &[])
}

fn move_file(st: &mut Store, src: &str, dest: &str, faults: &Faults) -> Vec<u8> {
    if faults.fail_move {
        return respond(
            507,
            "Insufficient Storage",
            b"move refused".to_vec(),
            &[],
        );
    }
    let s = normalize(src);
    let d = normalize(dest);
    let Some(bytes) = st.files.remove(&s) else {
        return respond(404, "Not Found", Vec::new(), &[]);
    };
    if let Some(parent) = parent_of(&d) {
        st.dirs.insert(parent);
    }
    st.files.insert(d, bytes);
    respond(201, "Created", Vec::new(), &[])
}

fn delete(st: &mut Store, path: &str, faults: &Faults) -> Vec<u8> {
    if faults.fail_delete {
        return respond(500, "Internal Server Error", b"delete failed".to_vec(), &[]);
    }
    let norm = normalize(path);
    st.files.remove(&norm);
    respond(204, "No Content", Vec::new(), &[])
}

fn mkcol(st: &mut Store, path: &str) -> Vec<u8> {
    let norm = normalize(path);
    if st.dirs.contains(&norm) {
        // 405 = 已存在。WebDavClient::ensure_dir 显式容忍这个状态码。
        return respond(405, "Method Not Allowed", Vec::new(), &[]);
    }
    st.dirs.insert(norm);
    respond(201, "Created", Vec::new(), &[])
}
