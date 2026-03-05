use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use async_trait::async_trait;

use arc_agent::Sandbox;

use super::config::{HookDefinition, HookType, TlsMode};
use super::types::{HookContext, HookDecision, HookResult};

/// Trait for executing hooks via different transports.
#[async_trait]
pub trait HookExecutor: Send + Sync {
    async fn execute(
        &self,
        definition: &HookDefinition,
        context: &HookContext,
        sandbox: &dyn Sandbox,
        work_dir: Option<&Path>,
    ) -> HookResult;
}

/// Interpolate `$VAR` and `${VAR}` references in `value` using environment
/// variables, but only when the variable name appears in `allowed_vars`.
/// Unlisted or missing vars are replaced with the empty string.
pub fn interpolate_env_vars(value: &str, allowed_vars: &[String]) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            let braced = chars.peek() == Some(&'{');
            if braced {
                chars.next(); // consume '{'
            }

            let mut var_name = String::new();
            while let Some(&c) = chars.peek() {
                if braced {
                    if c == '}' {
                        chars.next();
                        break;
                    }
                } else if !c.is_ascii_alphanumeric() && c != '_' {
                    break;
                }
                var_name.push(c);
                chars.next();
            }

            if !var_name.is_empty() && allowed_vars.iter().any(|v| v == &var_name) {
                if let Ok(val) = std::env::var(&var_name) {
                    result.push_str(&val);
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Executes hooks via shell commands or HTTP POST.
pub struct HookExecutorImpl;

impl HookExecutorImpl {
    /// Parse a hook decision from JSON stdout and exit code.
    fn parse_decision(exit_code: i32, stdout: &str) -> HookDecision {
        if exit_code == 0 {
            // Try parsing JSON response for explicit decision
            if let Ok(decision) = serde_json::from_str::<HookDecision>(stdout.trim()) {
                return decision;
            }
            HookDecision::Proceed
        } else if exit_code == 2 {
            // Exit 2 = block/skip
            if let Ok(decision) = serde_json::from_str::<HookDecision>(stdout.trim()) {
                return decision;
            }
            HookDecision::Block {
                reason: Some(format!("hook exited with code 2")),
            }
        } else {
            HookDecision::Block {
                reason: Some(format!("hook exited with code {exit_code}")),
            }
        }
    }

    /// Execute a command hook (sandbox or host).
    async fn execute_command(
        definition: &HookDefinition,
        command: &str,
        context: &HookContext,
        sandbox: &dyn Sandbox,
        work_dir: Option<&Path>,
    ) -> HookDecision {
        let context_json = serde_json::to_string(context).unwrap_or_default();
        let timeout_ms = definition.timeout().as_millis() as u64;

        let mut env_vars = HashMap::new();
        env_vars.insert("ARC_EVENT".to_string(), context.event.to_string());
        env_vars.insert("ARC_RUN_ID".to_string(), context.run_id.clone());
        env_vars.insert("ARC_WORKFLOW".to_string(), context.workflow_name.clone());
        if let Some(ref node_id) = context.node_id {
            env_vars.insert("ARC_NODE_ID".to_string(), node_id.clone());
        }

        if definition.runs_in_sandbox() {
            let ctx_path = format!(
                "/tmp/arc-hook-context-{}.json",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            if sandbox.write_file(&ctx_path, &context_json).await.is_ok() {
                env_vars.insert("ARC_HOOK_CONTEXT".to_string(), ctx_path.clone());
            }
            match sandbox
                .exec_command(command, timeout_ms, None, Some(&env_vars), None)
                .await
            {
                Ok(result) => Self::parse_decision(result.exit_code, &result.stdout),
                Err(e) => HookDecision::Block {
                    reason: Some(format!("sandbox exec failed: {e}")),
                },
            }
        } else {
            let mut cmd = std::process::Command::new("sh");
            cmd.arg("-c").arg(command);
            if let Some(wd) = work_dir {
                cmd.current_dir(wd);
            }
            for (k, v) in &env_vars {
                cmd.env(k, v);
            }
            cmd.stdin(std::process::Stdio::piped());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            match cmd.spawn() {
                Ok(mut child) => {
                    if let Some(mut stdin) = child.stdin.take() {
                        use std::io::Write;
                        let _ = stdin.write_all(context_json.as_bytes());
                    }
                    match child.wait_with_output() {
                        Ok(output) => {
                            let exit_code = output.status.code().unwrap_or(1);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            Self::parse_decision(exit_code, &stdout)
                        }
                        Err(e) => HookDecision::Block {
                            reason: Some(format!("command wait failed: {e}")),
                        },
                    }
                }
                Err(e) => HookDecision::Block {
                    reason: Some(format!("command spawn failed: {e}")),
                },
            }
        }
    }

    /// Execute an HTTP hook: POST context JSON and parse the response.
    /// Fail-open: non-2xx and connection errors return `Proceed`.
    async fn execute_http(
        url: &str,
        headers: &Option<HashMap<String, String>>,
        allowed_env_vars: &[String],
        tls: &TlsMode,
        context: &HookContext,
        timeout: std::time::Duration,
    ) -> HookDecision {
        // Enforce URL scheme based on TLS mode
        match tls {
            TlsMode::Verify | TlsMode::NoVerify => {
                if !url.starts_with("https://") {
                    return HookDecision::Block {
                        reason: Some(format!(
                            "HTTP hook URL must use https:// (tls mode is {tls:?})"
                        )),
                    };
                }
            }
            TlsMode::Off => {}
        }

        let accept_invalid = matches!(tls, TlsMode::NoVerify | TlsMode::Off);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .danger_accept_invalid_certs(accept_invalid)
            .build()
            .unwrap_or_default();

        let mut request = client.post(url).json(context);

        if let Some(hdrs) = headers {
            for (key, value) in hdrs {
                let interpolated = interpolate_env_vars(value, allowed_env_vars);
                request = request.header(key, interpolated);
            }
        }

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!(url, error = %e, "HTTP hook request failed, proceeding");
                return HookDecision::Proceed;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                url,
                status = response.status().as_u16(),
                "HTTP hook returned non-2xx, proceeding"
            );
            return HookDecision::Proceed;
        }

        let body = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(url, error = %e, "HTTP hook body read failed, proceeding");
                return HookDecision::Proceed;
            }
        };

        if body.trim().is_empty() {
            return HookDecision::Proceed;
        }

        match serde_json::from_str::<HookDecision>(body.trim()) {
            Ok(decision) => decision,
            Err(e) => {
                tracing::warn!(url, error = %e, "HTTP hook response parse failed, proceeding");
                HookDecision::Proceed
            }
        }
    }
}

#[async_trait]
impl HookExecutor for HookExecutorImpl {
    async fn execute(
        &self,
        definition: &HookDefinition,
        context: &HookContext,
        sandbox: &dyn Sandbox,
        work_dir: Option<&Path>,
    ) -> HookResult {
        let start = Instant::now();

        let decision = match definition.resolved_hook_type() {
            Some(HookType::Command { ref command }) => {
                Self::execute_command(definition, command, context, sandbox, work_dir).await
            }
            Some(HookType::Http {
                ref url,
                ref headers,
                ref allowed_env_vars,
                ref tls,
            }) => {
                Self::execute_http(url, headers, allowed_env_vars, tls, context, definition.timeout())
                    .await
            }
            None => HookDecision::Block {
                reason: Some("no hook type specified".into()),
            },
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        HookResult {
            hook_name: definition.name.clone(),
            decision,
            duration_ms,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::config::HookType;
    use crate::hook::types::HookEvent;

    fn make_context() -> HookContext {
        HookContext::new(HookEvent::StageStart, "run-1".into(), "test-wf".into())
    }

    fn make_definition(command: &str) -> HookDefinition {
        HookDefinition {
            name: Some("test-hook".into()),
            event: HookEvent::StageStart,
            command: Some(command.into()),
            hook_type: None,
            matcher: None,
            blocking: None,
            timeout_ms: Some(5000),
            sandbox: Some(false), // host execution for tests
        }
    }

    #[test]
    fn parse_decision_exit_0_proceed() {
        assert_eq!(
            HookExecutorImpl::parse_decision(0, ""),
            HookDecision::Proceed
        );
    }

    #[test]
    fn parse_decision_exit_0_with_json() {
        let json = r#"{"decision": "skip", "reason": "not needed"}"#;
        assert_eq!(
            HookExecutorImpl::parse_decision(0, json),
            HookDecision::Skip {
                reason: Some("not needed".into())
            }
        );
    }

    #[test]
    fn parse_decision_exit_2_block() {
        assert!(matches!(
            HookExecutorImpl::parse_decision(2, ""),
            HookDecision::Block { .. }
        ));
    }

    #[test]
    fn parse_decision_exit_2_with_json() {
        let json = r#"{"decision": "skip", "reason": "skipping"}"#;
        assert_eq!(
            HookExecutorImpl::parse_decision(2, json),
            HookDecision::Skip {
                reason: Some("skipping".into())
            }
        );
    }

    #[test]
    fn parse_decision_exit_1_block() {
        assert!(matches!(
            HookExecutorImpl::parse_decision(1, ""),
            HookDecision::Block { .. }
        ));
    }

    #[test]
    fn parse_decision_exit_0_override() {
        let json = r#"{"decision": "override", "edge_to": "node_b"}"#;
        assert_eq!(
            HookExecutorImpl::parse_decision(0, json),
            HookDecision::Override {
                edge_to: "node_b".into()
            }
        );
    }

    #[tokio::test]
    async fn command_executor_host_success() {
        let executor = HookExecutorImpl;
        let def = make_definition("exit 0");
        let ctx = make_context();
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;
        assert_eq!(result.decision, HookDecision::Proceed);
        assert_eq!(result.hook_name.as_deref(), Some("test-hook"));
    }

    #[tokio::test]
    async fn command_executor_host_failure() {
        let executor = HookExecutorImpl;
        let def = make_definition("exit 1");
        let ctx = make_context();
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;
        assert!(matches!(result.decision, HookDecision::Block { .. }));
    }

    #[tokio::test]
    async fn command_executor_host_skip_via_exit_2() {
        let executor = HookExecutorImpl;
        let def = make_definition("exit 2");
        let ctx = make_context();
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;
        assert!(matches!(result.decision, HookDecision::Block { .. }));
    }

    #[tokio::test]
    async fn command_executor_host_json_decision() {
        let executor = HookExecutorImpl;
        let def =
            make_definition(r#"echo '{"decision": "skip", "reason": "test skip"}'"#);
        let ctx = make_context();
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;
        assert_eq!(
            result.decision,
            HookDecision::Skip {
                reason: Some("test skip".into())
            }
        );
    }

    #[tokio::test]
    async fn command_executor_env_vars_set() {
        let executor = HookExecutorImpl;
        // Print env vars to stdout for verification
        let def = make_definition("echo $ARC_EVENT:$ARC_RUN_ID:$ARC_WORKFLOW");
        let mut ctx = make_context();
        ctx.node_id = Some("plan".into());
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;
        assert_eq!(result.decision, HookDecision::Proceed);
    }

    #[tokio::test]
    async fn no_hook_type_blocks() {
        let executor = HookExecutorImpl;
        let def = HookDefinition {
            name: None,
            event: HookEvent::StageStart,
            command: None,
            hook_type: None,
            matcher: None,
            blocking: None,
            timeout_ms: None,
            sandbox: Some(false),
        };
        let ctx = make_context();
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;
        assert!(matches!(result.decision, HookDecision::Block { .. }));
    }

    // --- interpolate_env_vars tests ---

    #[test]
    fn interpolate_resolves_allowed_var() {
        std::env::set_var("ARC_TEST_KEY_1", "secret123");
        let result = interpolate_env_vars(
            "Bearer $ARC_TEST_KEY_1",
            &["ARC_TEST_KEY_1".to_string()],
        );
        assert_eq!(result, "Bearer secret123");
        std::env::remove_var("ARC_TEST_KEY_1");
    }

    #[test]
    fn interpolate_resolves_braced_var() {
        std::env::set_var("ARC_TEST_KEY_2", "val");
        let result = interpolate_env_vars(
            "x${ARC_TEST_KEY_2}y",
            &["ARC_TEST_KEY_2".to_string()],
        );
        assert_eq!(result, "xvaly");
        std::env::remove_var("ARC_TEST_KEY_2");
    }

    #[test]
    fn interpolate_unlisted_var_becomes_empty() {
        std::env::set_var("ARC_TEST_KEY_3", "should_not_appear");
        let result = interpolate_env_vars(
            "prefix-$ARC_TEST_KEY_3-suffix",
            &[],
        );
        assert_eq!(result, "prefix--suffix");
        std::env::remove_var("ARC_TEST_KEY_3");
    }

    #[test]
    fn interpolate_missing_var_becomes_empty() {
        std::env::remove_var("ARC_TEST_NOEXIST");
        let result = interpolate_env_vars(
            "a$ARC_TEST_NOEXIST-b",
            &["ARC_TEST_NOEXIST".to_string()],
        );
        assert_eq!(result, "a-b");
    }

    #[test]
    fn interpolate_no_vars_passes_through() {
        assert_eq!(interpolate_env_vars("plain text", &[]), "plain text");
    }

    #[test]
    fn interpolate_mixed_text() {
        std::env::set_var("ARC_TEST_A", "hello");
        std::env::set_var("ARC_TEST_B", "world");
        let result = interpolate_env_vars(
            "$ARC_TEST_A ${ARC_TEST_B}!",
            &["ARC_TEST_A".to_string(), "ARC_TEST_B".to_string()],
        );
        assert_eq!(result, "hello world!");
        std::env::remove_var("ARC_TEST_A");
        std::env::remove_var("ARC_TEST_B");
    }

    // --- HTTP hook execution tests ---

    #[tokio::test]
    async fn http_hook_posts_json_and_parses_decision() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .match_header("content-type", "application/json")
            .with_status(200)
            .with_body(r#"{"decision": "skip", "reason": "not needed"}"#)
            .create_async()
            .await;

        let decision = HookExecutorImpl::execute_http(
            &format!("{}/hook", server.url()),
            &None,
            &[],
            &TlsMode::Off,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        mock.assert_async().await;
        assert_eq!(
            decision,
            HookDecision::Skip {
                reason: Some("not needed".into())
            }
        );
    }

    #[tokio::test]
    async fn http_hook_empty_2xx_returns_proceed() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .with_status(200)
            .with_body("")
            .create_async()
            .await;

        let decision = HookExecutorImpl::execute_http(
            &format!("{}/hook", server.url()),
            &None,
            &[],
            &TlsMode::Off,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        mock.assert_async().await;
        assert_eq!(decision, HookDecision::Proceed);
    }

    #[tokio::test]
    async fn http_hook_non_2xx_returns_proceed() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let decision = HookExecutorImpl::execute_http(
            &format!("{}/hook", server.url()),
            &None,
            &[],
            &TlsMode::Off,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        mock.assert_async().await;
        assert_eq!(decision, HookDecision::Proceed);
    }

    #[tokio::test]
    async fn http_hook_connection_failure_returns_proceed() {
        let decision = HookExecutorImpl::execute_http(
            "http://127.0.0.1:1",
            &None,
            &[],
            &TlsMode::Off,
            &make_context(),
            std::time::Duration::from_secs(1),
        )
        .await;

        assert_eq!(decision, HookDecision::Proceed);
    }

    #[tokio::test]
    async fn http_hook_sends_interpolated_headers() {
        std::env::set_var("ARC_TEST_TOKEN", "my-secret");

        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .match_header("authorization", "Bearer my-secret")
            .with_status(200)
            .with_body("")
            .create_async()
            .await;

        let headers = HashMap::from([
            ("Authorization".to_string(), "Bearer $ARC_TEST_TOKEN".to_string()),
        ]);

        let decision = HookExecutorImpl::execute_http(
            &format!("{}/hook", server.url()),
            &Some(headers),
            &["ARC_TEST_TOKEN".to_string()],
            &TlsMode::Off,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        mock.assert_async().await;
        assert_eq!(decision, HookDecision::Proceed);
        std::env::remove_var("ARC_TEST_TOKEN");
    }

    // --- TLS mode enforcement tests ---

    #[tokio::test]
    async fn http_hook_rejects_http_url_when_tls_verify() {
        let decision = HookExecutorImpl::execute_http(
            "http://example.com/hook",
            &None,
            &[],
            &TlsMode::Verify,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        assert!(matches!(decision, HookDecision::Block { .. }));
    }

    #[tokio::test]
    async fn http_hook_rejects_http_url_when_tls_no_verify() {
        let decision = HookExecutorImpl::execute_http(
            "http://example.com/hook",
            &None,
            &[],
            &TlsMode::NoVerify,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        assert!(matches!(decision, HookDecision::Block { .. }));
    }

    #[tokio::test]
    async fn http_hook_allows_http_url_when_tls_off() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .with_status(200)
            .with_body("")
            .create_async()
            .await;

        let decision = HookExecutorImpl::execute_http(
            &format!("{}/hook", server.url()),
            &None,
            &[],
            &TlsMode::Off,
            &make_context(),
            std::time::Duration::from_secs(5),
        )
        .await;

        mock.assert_async().await;
        assert_eq!(decision, HookDecision::Proceed);
    }

    #[tokio::test]
    async fn executor_dispatches_http_hook() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .with_status(200)
            .with_body(r#"{"decision": "proceed"}"#)
            .create_async()
            .await;

        let executor = HookExecutorImpl;
        let def = HookDefinition {
            name: Some("http-test".into()),
            event: HookEvent::StageStart,
            command: None,
            hook_type: Some(HookType::Http {
                url: format!("{}/hook", server.url()),
                headers: None,
                allowed_env_vars: vec![],
                tls: TlsMode::Off,
            }),
            matcher: None,
            blocking: None,
            timeout_ms: Some(5000),
            sandbox: Some(false),
        };
        let ctx = make_context();
        let sandbox = arc_agent::LocalSandbox::new(std::env::current_dir().unwrap());
        let result = executor.execute(&def, &ctx, &sandbox, None).await;

        mock.assert_async().await;
        assert_eq!(result.decision, HookDecision::Proceed);
        assert_eq!(result.hook_name.as_deref(), Some("http-test"));
    }
}
