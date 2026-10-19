//! AI 调用错误的结构化分类。
//!
//! ## 为什么需要它
//!
//! 旧实现把一切失败都 `format!` 成一句 `String`，前端只能原样展示。
//! 于是「API Key 失效」和「网络断线」和「本章确实没有新设定」
//! 在作者眼里是同一件事：**没有提炼结果**。
//!
//! 更糟的是编排层把这些错误吞掉（见 `extraction.rs` 中
//! `Ok(Err(e)) => { all_succeeded = false; eprintln!(...) }`），
//! 最终无论成败都 `Ok(vec![])` —— 全员 401 会显示成「本章无新内容」。
//! 作者永远不会意识到该去检查配置。
//!
//! 分类的价值不在于「信息更丰富」，而在于让**该做什么动作**变得明确：
//! 鉴权失败要去改 Key，限流要等一会，格式异常可以重试，
//! 而「确实没有新设定」根本不是错误。

/// 稳定的错误类别。前端据此决定提示文案与是否提供「重试」。
///
/// `code` 会随 IPC 传给前端，取值一旦发布就不要再改语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionFailureKind {
    /// 网络层失败：连不上、DNS、TLS。
    Network,
    /// 鉴权 / 授权失败（401 / 403）。**必须提示用户检查 API Key。**
    Auth,
    /// 限流（429 / 5xx 过载）。
    RateLimited,
    /// 请求超时。
    Timeout,
    /// 模型返回了无法解析为候选的内容（闲聊、截断 JSON、格式漂移）。
    MalformedOutput,
    /// 数据库写入 / 去重查询失败。
    Database,
    /// 用户主动取消。
    Cancelled,
    /// 内部状态不一致等其它问题。
    Internal,
}

impl ExtractionFailureKind {
    /// 面向作者的一句话说明：**告诉他该做什么**，而不是复述错误。
    pub fn user_hint(&self) -> &'static str {
        match self {
            ExtractionFailureKind::Network => "无法连接模型服务，请检查网络或代理设置。",
            ExtractionFailureKind::Auth => "模型服务拒绝了本次请求，请检查 API Key 是否正确、是否已过期或欠费。",
            ExtractionFailureKind::RateLimited => "模型服务当前限流，请稍后重试。",
            ExtractionFailureKind::Timeout => "模型响应超时，请稍后重试，或换用更快的模型。",
            ExtractionFailureKind::MalformedOutput => "模型返回的内容无法解析，重试通常可以解决。",
            ExtractionFailureKind::Database => "写入本地设定库失败，请检查磁盘空间。",
            ExtractionFailureKind::Cancelled => "本次提炼已被取消。",
            ExtractionFailureKind::Internal => "提炼过程出现内部问题，请重启应用后重试。",
        }
    }

    /// 是否值得自动重试。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ExtractionFailureKind::Network
                | ExtractionFailureKind::RateLimited
                | ExtractionFailureKind::Timeout
                | ExtractionFailureKind::MalformedOutput
        )
    }
}

/// 单个 Agent 的失败记录。**不携带正文、API Key 或厂商原始响应体。**
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentFailure {
    pub agent: String,
    pub kind: ExtractionFailureKind,
    /// 已脱敏、可直接展示的简短原因。
    pub reason: String,
}

/// 一次提炼的完整结果。
///
/// 关键设计：**「没有提炼到东西」和「提炼失败」必须是两种不同的结果。**
/// 旧实现把两者都表达成 `Ok(vec![])`，这是本次修复的核心。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractionOutcome {
    pub candidates: Vec<shared_types::ExtractionCandidate>,
    /// 成功产出的候选数。
    pub succeeded_agents: usize,
    /// 失败的 Agent 明细（已脱敏）。
    pub failures: Vec<AgentFailure>,
    /// 本次正文指纹未变、跳过了模型调用。
    pub skipped_fingerprint_match: bool,
    /// 全员成功（含入库）。
    pub all_succeeded: bool,
}

impl ExtractionOutcome {
    /// 是否应当以「失败」而不是「成功但没有结果」呈现给作者。
    pub fn is_failure(&self) -> bool {
        !self.all_succeeded
    }

    /// 合并多条失败记录为一句面向作者的提示。
    pub fn summarize(&self) -> Option<String> {
        if self.failures.is_empty() {
            return None;
        }
        let auth = self
            .failures
            .iter()
            .any(|f| f.kind == ExtractionFailureKind::Auth);
        if auth {
            return Some(format!(
                "{} / {} 个 Agent 因鉴权失败未完成。{}",
                self.succeeded_agents,
                self.succeeded_agents + self.failures.len(),
                ExtractionFailureKind::Auth.user_hint()
            ));
        }
        let kinds: Vec<&str> = self.failures.iter().map(|f| f.agent.as_str()).collect();
        Some(format!(
            "{} / {} 个 Agent 未完成（{}）。最可能的原因：{}",
            self.succeeded_agents,
            self.succeeded_agents + self.failures.len(),
            kinds.join("、"),
            self.failures[0].kind.user_hint()
        ))
    }
}

/// 把一条错误文本归类。
///
/// 这是**启发式**归类而不是结构化解析：上游 `openai.rs` / `anthropic.rs`
/// 目前仍然返回裸 `String`，改动它们的返回类型会波及全部调用点
/// （chat / polish / consistency / models 等），超出本次可靠性专项的最小范围。
/// 因此这里从文本反推类别 —— 不够优雅，但零破坏性，
/// 且**比现在的「全部 Unknown」有实质改善**。
///
/// 后续若要让上游返回结构化错误，应先让本函数对已知格式做同样判断，
/// 再逐步替换为直接匹配。
pub fn classify_error(raw: &str) -> ExtractionFailureKind {
    let s = raw.to_ascii_lowercase();

    // 鉴权优先于其它网络类判断：
    // 厂商错误正文常同时含 "request" / "unauthorized"。
    if s.contains("401")
        || s.contains("403")
        || s.contains("unauthorized")
        || s.contains("forbidden")
        || s.contains("invalid api key")
        || s.contains("invalid_api_key")
        || s.contains("authentication")
        || s.contains("permission denied")
    {
        return ExtractionFailureKind::Auth;
    }

    if s.contains("429")
        || s.contains("rate limit")
        || s.contains("rate_limit")
        || s.contains("too many requests")
        || s.contains("overloaded")
        || s.contains("quota")
        {
        return ExtractionFailureKind::RateLimited;
    }

    if s.contains("timeout") || s.contains("timed out") || s.contains("deadline exceeded") {
        return ExtractionFailureKind::Timeout;
    }

    if s.contains("connection")
        || s.contains("dns")
        || s.contains("network")
        || s.contains("failed to lookup")
        || s.contains("tcp connect")
        || s.contains("eof")
        || s.contains("reset by peer")
    {
        return ExtractionFailureKind::Network;
    }

    if s.contains("database")
        || s.contains("sqlite")
        || s.contains("sql ")
        || s.contains("no such table")
        || s.contains("disk i/o")
    {
        return ExtractionFailureKind::Database;
    }

    if s.contains("cancel") || s.contains("abort") {
        return ExtractionFailureKind::Cancelled;
    }

    ExtractionFailureKind::Internal
}

/// 生成一条**已脱敏**的失败原因。
///
/// 旧实现把厂商返回的原始响应体整段打进日志
/// （`openai.rs` 的 `format!("OpenAI API error ({}): {}", status, err_text)`），
/// 而某些厂商会在错误响应里回显请求摘要。作者正文与 Key 都不该进日志。
/// 这里只保留 HTTP 状态码这一项确定安全的信息。
pub fn sanitize_reason(raw: &str) -> String {
    // 提取 HTTP 状态码（若存在），其余一律丢弃。
    if let Some(status) = extract_http_status(raw) {
        return format!("模型服务返回 HTTP {status}");
    }

    let s = raw.to_ascii_lowercase();
    if s.contains("timeout") || s.contains("timed out") {
        return "请求超时".to_string();
    }
    if s.contains("connection") || s.contains("dns") || s.contains("network") {
        return "网络连接失败".to_string();
    }
    // 解析失败：只说明「格式无法解析」，不回显模型原文。
    if s.contains("json") || s.contains("解析") || s.contains("parse") {
        return "模型返回内容无法解析为候选列表".to_string();
    }
    "模型调用失败".to_string()
}

fn extract_http_status(raw: &str) -> Option<String> {
    // 形如 "error (429): ..." / "HTTP 429" / "status 429"
    let re = regex::Regex::new(r"(?i)(?:error[^\d]{0,12}|http|status[^\d]{0,12})(\d{3})")
        .ok()?;
    re.captures(raw).map(|c| c[1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_auth_failures() {
        assert_eq!(
            classify_error("OpenAI API error (401): invalid api key"),
            ExtractionFailureKind::Auth
        );
        assert_eq!(
            classify_error("HTTP 403 Forbidden"),
            ExtractionFailureKind::Auth
        );
        assert_eq!(
            classify_error("Unauthorized"),
            ExtractionFailureKind::Auth
        );
    }

    #[test]
    fn classifies_rate_limits() {
        assert_eq!(
            classify_error("OpenAI API error (429): rate limit exceeded"),
            ExtractionFailureKind::RateLimited
        );
        assert_eq!(
            classify_error("Too Many Requests"),
            ExtractionFailureKind::RateLimited
        );
        assert_eq!(classify_error("model overloaded"), ExtractionFailureKind::RateLimited);
    }

    #[test]
    fn classifies_network_and_timeout() {
        assert_eq!(
            classify_error("Network request failed: dns error"),
            ExtractionFailureKind::Network
        );
        assert_eq!(
            classify_error("request timed out after 90s"),
            ExtractionFailureKind::Timeout
        );
    }

    #[test]
    fn classifies_database_errors() {
        assert_eq!(
            classify_error("no such table: asset"),
            ExtractionFailureKind::Database
        );
    }

    #[test]
    fn sanitized_reason_drops_provider_body() {
        // 厂商错误正文可能回显请求内容，绝不能进日志。
        let raw = "OpenAI API error (429): {\"error\":{\"message\":\"rate limit\",\"request\":\"章节正文...\"}}";
        let clean = sanitize_reason(raw);
        assert_eq!(clean, "模型服务返回 HTTP 429");
        assert!(!clean.contains("章节正文"));
    }

    #[test]
    fn sanitized_reason_never_leaks_content() {
        let raw = "解析失败: 期望 JSON 数组，实际得到: 他缓缓推开窗棂，远处传来更鼓声……";
        let clean = sanitize_reason(raw);
        assert!(!clean.contains("窗棂"), "不得回显模型原文：{clean}");
        assert!(!clean.contains("更鼓"));
    }

    #[test]
    fn auth_failure_surfaces_actionable_hint() {
        let outcome = ExtractionOutcome {
            candidates: vec![],
            succeeded_agents: 0,
            failures: vec![AgentFailure {
                agent: "角色 Agent".into(),
                kind: ExtractionFailureKind::Auth,
                reason: "模型服务返回 HTTP 401".into(),
            }],
            skipped_fingerprint_match: false,
            all_succeeded: false,
        };
        assert!(outcome.is_failure());
        let msg = outcome.summarize().unwrap();
        assert!(msg.contains("API Key"), "鉴权失败必须提示检查 Key：{msg}");
    }

    #[test]
    fn empty_but_successful_is_not_a_failure() {
        // 「本章确实没有新设定」与「提炼失败」必须可区分。
        let outcome = ExtractionOutcome {
            candidates: vec![],
            succeeded_agents: 4,
            failures: vec![],
            skipped_fingerprint_match: false,
            all_succeeded: true,
        };
        assert!(!outcome.is_failure());
        assert!(outcome.summarize().is_none());
    }

    #[test]
    fn retryable_kinds_exclude_auth() {
        assert!(ExtractionFailureKind::Timeout.is_retryable());
        assert!(ExtractionFailureKind::RateLimited.is_retryable());
        assert!(!ExtractionFailureKind::Auth.is_retryable());
    }
}
