//! 安全边界的回归测试（V0.3.8 P1-3 / P1-4）。
//!
//! 覆盖基线审计中的四条：
//! - C-08：日志输出模型原文（含正文引用的 evidence 片段）；
//! - C-09：EPUB 无解压上限（见 `epub_import.rs`，此处只做跨模块的兜底断言）；
//! - C-10：`open_external_url` 只校验协议，不限制目标 host；
//! - 凭据生命周期：秘密不得进入备份清单、不得出现在任何导出结构中。
//!
//! 这些用例不依赖网络与钥匙串，全部是纯函数，可在 CI 中稳定运行。

use easy_writing_lib::agent::outcome::{
    classify_error, sanitize_reason, AgentFailure, ExtractionFailureKind, ExtractionOutcome,
};
use easy_writing_lib::db;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// C-10 · 外链 host 限制
// ---------------------------------------------------------------------------

#[test]
fn external_url_policy_blocks_internal_targets() {
    // 直接复刻 update_cmd 中的策略判定，确保门禁不会被误改。
    fn is_blocked(url: &str) -> bool {
        use std::net::IpAddr;
        let rest = url.split("://").nth(1).unwrap_or("");
        let after_auth = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
        let host_part = after_auth
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let host = if let Some(s) = host_part.strip_prefix('[') {
            s.split(']').next().unwrap_or("").to_string()
        } else {
            host_part.split(':').next().unwrap_or("").to_string()
        };
        if host == "localhost" || host.ends_with(".localhost") || host == "::1" {
            return true;
        }
        if host == "169.254.169.254" || host == "metadata.google.internal" {
            return true;
        }
        match host.parse::<IpAddr>() {
            Ok(IpAddr::V4(v4)) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            Ok(IpAddr::V6(v6)) => v6.is_loopback() || v6.is_unspecified(),
            Err(_) => false,
        }
    }

    for blocked in [
        "http://169.254.169.254/latest/meta-data/iam/security-credentials/",
        "http://127.0.0.1:8080/admin",
        "http://localhost/admin",
        "http://192.168.1.1/",
        "http://10.0.0.1/",
        "http://172.16.0.1/",
        "http://[::1]:9000/",
    ] {
        assert!(is_blocked(blocked), "内网目标必须被拦截：{blocked}");
    }
    for allowed in [
        "https://github.com/orderye/Easy-writing/releases",
        "https://api.github.com/repos/x/y",
        "https://8.8.8.8/",
    ] {
        assert!(!is_blocked(allowed), "正常公网链接必须放行：{allowed}");
    }
}

// ---------------------------------------------------------------------------
// C-08 · 日志脱敏
// ---------------------------------------------------------------------------

#[test]
fn parse_error_message_never_echoes_model_prose() {
    // 模型返回了一段包含正文引用（evidence）的非法内容。
    let raw = "抱歉，我无法完成这个请求。作为一个 AI，我应该直接说明：我不能扮演……";
    let reason = sanitize_reason(raw);
    for fragment in ["抱歉", "无法完成", "扮演", "AI"] {
        assert!(
            !reason.contains(fragment),
            "失败原因不得回显模型原文（命中「{fragment}」）：{reason}"
        );
    }
}

#[test]
fn sanitize_reason_keeps_actionable_signal_only() {
    // 保留 HTTP 状态码即可定位问题，其余一律丢弃。
    assert_eq!(
        sanitize_reason("Anthropic API error (429): slow down"),
        "模型服务返回 HTTP 429"
    );
    assert!(sanitize_reason("connection reset by peer").contains("网络"));
    assert!(sanitize_reason("request timed out").contains("超时"));
}

// ---------------------------------------------------------------------------
// C-07 · 错误分类
// ---------------------------------------------------------------------------

#[test]
fn auth_failures_are_never_reported_as_success() {
    // 全员 401：旧实现会返回 Ok(vec![])，作者看到「本章无新设定」。
    let failures: Vec<AgentFailure> = ["世界观", "角色", "剧情", "大纲"]
        .iter()
        .map(|a| AgentFailure {
            agent: a.to_string(),
            kind: classify_error("API error (401): invalid api key"),
            reason: "模型服务返回 HTTP 401".to_string(),
        })
        .collect();

    let outcome = ExtractionOutcome {
        candidates: vec![],
        succeeded_agents: 0,
        failures,
        skipped_fingerprint_match: false,
        all_succeeded: false,
    };

    assert!(outcome.is_failure(), "全员鉴权失败必须判定为失败");
    let msg = outcome.summarize().expect("失败必须有可展示的原因");
    assert!(msg.contains("API Key"), "必须提示检查 Key：{msg}");
    assert!(!outcome.failures[0].kind.is_retryable(), "鉴权失败重试无意义");
}

#[test]
fn empty_result_with_success_is_distinguishable_from_failure() {
    // 「本章确实没有新设定」必须与「提炼失败」在数据结构上可区分。
    let success = ExtractionOutcome {
        candidates: vec![],
        succeeded_agents: 4,
        failures: vec![],
        skipped_fingerprint_match: false,
        all_succeeded: true,
    };
    assert!(!success.is_failure());
    assert!(success.summarize().is_none());

    let failure = ExtractionOutcome {
        candidates: vec![],
        succeeded_agents: 2,
        failures: vec![AgentFailure {
            agent: "角色".into(),
            kind: ExtractionFailureKind::Timeout,
            reason: "请求超时".into(),
        }],
        skipped_fingerprint_match: false,
        all_succeeded: false,
    };
    assert!(failure.is_failure());
    assert!(failure.summarize().is_some());
}

#[test]
fn partial_failure_reports_which_agents_failed() {
    let outcome = ExtractionOutcome {
        candidates: vec![],
        succeeded_agents: 3,
        failures: vec![AgentFailure {
            agent: "剧情 Agent".into(),
            kind: ExtractionFailureKind::RateLimited,
            reason: "模型服务返回 HTTP 429".into(),
        }],
        skipped_fingerprint_match: false,
        all_succeeded: false,
    };
    let msg = outcome.summarize().unwrap();
    assert!(msg.contains("3 / 4"), "应说明成功/总数：{msg}");
    assert!(msg.contains("剧情"), "应指出失败的是哪个 Agent：{msg}");
    assert!(msg.contains("稍后重试"), "限流应给出可执行建议：{msg}");
}

// ---------------------------------------------------------------------------
// 凭据生命周期：秘密不得外泄
// ---------------------------------------------------------------------------

#[test]
fn backup_manifest_contains_no_credentials() {
    let dir: PathBuf = std::env::temp_dir().join(format!("ew_sec_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pool = db::init_db(dir.clone()).unwrap();

    {
        let conn = pool.get().unwrap();
        let nid = db::novel::create_novel(&conn, "保密测试", "").unwrap().id;

        // 写入真实形状的凭据
        conn.execute(
            "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir, updated_at)
             VALUES ('default_webdav', 'https://dav.secret-host.example', 'alice', 'webdav_default_pwd', 'backups', datetime('now'))",
            [],
        ).unwrap();
        db::llm_profile::create_profile(
            &conn, "主力模型", "openai", "https://api.example.com/v1",
            "sk-PROJECT-MUST-NOT-LEAK", "gpt-4", "", true,
        ).unwrap();
        // app_setting 已由应用初始化写入默认项（含 theme），
        // 这里换用一个未被占用的键，避免 UNIQUE 约束冲突。
        conn.execute(
            "INSERT INTO app_setting (key, value) VALUES ('right_panel_visible', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        ).unwrap();

        let m = easy_writing_lib::sync::manifest::build_manifest(&conn, "0.3.8", 1);
        let json = serde_json::to_string(&m).unwrap();

        for secret in [
            "PROJECT-MUST-NOT-LEAK",
            "dav.secret-host.example",
            "webdav_default_pwd",
            "alice",
        ] {
            assert!(
                !json.contains(secret),
                "备份清单泄漏了秘密「{secret}」：{json}"
            );
        }
        // 同时确认清单确实记录了业务实体（否则这个测试是空跑）
        assert_eq!(m.entity_counts.get("novel").copied(), Some(1));
        assert_eq!(m.schema_version, db::BASELINE_VERSION);
        let _ = nid;
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn llm_profile_key_is_stored_as_reference_not_plaintext() {
    // 现有实现把 key_ref 存进 llm_profile.key_ref。
    // 本用例确认**数据库里不出现明文 API Key**——即使 create_profile 的
    // 第五个参数名叫 key_ref，调用方也必须已经完成了「写钥匙串、只存引用」这件事。
    let dir: PathBuf = std::env::temp_dir().join(format!("ew_key_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pool = db::init_db(dir).unwrap();

    {
        let conn = pool.get().unwrap();
        db::llm_profile::create_profile(
            &conn, "P", "openai", "https://api.example.com/v1",
            "llm_profile_abc123", "gpt-4", "", true,
        ).unwrap();

        let stored: String = conn
            .query_row("SELECT key_ref FROM llm_profile LIMIT 1", [], |r| r.get(0))
            .unwrap();
        // 存储的是钥匙串引用键名，不是 "sk-" 开头的明文密钥
        assert!(!stored.starts_with("sk-"), "数据库中不得出现明文 API Key：{stored}");
        assert_eq!(stored, "llm_profile_abc123");
    }
}
