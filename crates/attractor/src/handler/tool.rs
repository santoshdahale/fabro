use std::path::Path;

use async_trait::async_trait;

use crate::context::Context;
use crate::error::AttractorError;
use crate::graph::{Graph, Node};
use crate::outcome::Outcome;

use super::{EngineServices, Handler};

fn timeout_ms(node: &Node) -> Option<u64> {
    node.timeout().map(|d| d.as_millis() as u64)
}

/// Executes an external tool (shell command) configured via node attributes.
pub struct ToolHandler;

#[async_trait]
impl Handler for ToolHandler {
    async fn execute(
        &self,
        node: &Node,
        _context: &Context,
        _graph: &Graph,
        logs_root: &Path,
        _services: &EngineServices,
    ) -> Result<Outcome, AttractorError> {
        let command = node
            .attrs
            .get("tool_command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if command.is_empty() {
            return Ok(Outcome::fail("No tool_command specified"));
        }

        let stage_dir = logs_root.join(&node.id);
        tokio::fs::create_dir_all(&stage_dir).await?;

        let invocation = serde_json::json!({
            "command": command,
            "timeout_ms": timeout_ms(node),
        });
        tokio::fs::write(
            stage_dir.join("tool_invocation.json"),
            serde_json::to_string_pretty(&invocation).unwrap(),
        )
        .await?;

        let cmd_future = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output();

        let started = std::time::Instant::now();

        let output = if let Some(timeout_dur) = node.timeout() {
            match tokio::time::timeout(timeout_dur, cmd_future).await {
                Ok(result) => result,
                Err(_elapsed) => {
                    let duration_ms = started.elapsed().as_millis() as u64;
                    let timing = serde_json::json!({
                        "duration_ms": duration_ms,
                        "exit_code": null,
                        "timed_out": true,
                    });
                    tokio::fs::write(
                        stage_dir.join("tool_timing.json"),
                        serde_json::to_string_pretty(&timing).unwrap(),
                    )
                    .await?;

                    return Ok(Outcome::fail(format!(
                        "Tool timed out after {}ms: {command}",
                        timeout_dur.as_millis()
                    )));
                }
            }
        } else {
            cmd_future.await
        };

        let duration_ms = started.elapsed().as_millis() as u64;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                tokio::fs::write(stage_dir.join("stdout.log"), &stdout).await?;
                tokio::fs::write(stage_dir.join("stderr.log"), &stderr).await?;

                let timing = serde_json::json!({
                    "duration_ms": duration_ms,
                    "exit_code": output.status.code(),
                    "timed_out": false,
                });
                tokio::fs::write(
                    stage_dir.join("tool_timing.json"),
                    serde_json::to_string_pretty(&timing).unwrap(),
                )
                .await?;

                if output.status.success() {
                    let mut outcome = Outcome::success();
                    outcome
                        .context_updates
                        .insert("tool.output".to_string(), serde_json::json!(stdout));
                    outcome.notes = Some(format!("Tool completed: {command}"));
                    Ok(outcome)
                } else {
                    let reason = if stderr.is_empty() {
                        format!(
                            "Tool failed with exit code: {}",
                            output.status.code().unwrap_or(-1)
                        )
                    } else {
                        format!("Tool failed: {}", stderr.trim())
                    };
                    Ok(Outcome::fail(reason))
                }
            }
            Err(e) => Ok(Outcome::fail(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventEmitter;
    use crate::graph::AttrValue;
    use crate::handler::start::StartHandler;
    use crate::handler::HandlerRegistry;
    use crate::outcome::StageStatus;
    use std::time::Duration;

    fn make_services() -> EngineServices {
        EngineServices {
            registry: std::sync::Arc::new(HandlerRegistry::new(Box::new(StartHandler))),
            emitter: std::sync::Arc::new(EventEmitter::new()),
            execution_env: std::sync::Arc::new(agent::LocalExecutionEnvironment::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            )),
        }
    }

    #[tokio::test]
    async fn tool_handler_no_command() {
        let handler = ToolHandler;
        let node = Node::new("tool_node");
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        let outcome = handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();
        assert_eq!(outcome.status, StageStatus::Fail);
        assert_eq!(
            outcome.failure_reason.as_deref(),
            Some("No tool_command specified")
        );
    }

    #[tokio::test]
    async fn tool_handler_echo_command() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("echo hello".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        let outcome = handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();
        assert_eq!(outcome.status, StageStatus::Success);
        assert!(outcome.notes.as_deref().unwrap().contains("echo hello"));
        let tool_output = outcome.context_updates.get("tool.output").unwrap();
        assert!(tool_output.as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn tool_handler_failing_command() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("false".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        let outcome = handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();
        assert_eq!(outcome.status, StageStatus::Fail);
    }

    #[tokio::test]
    async fn tool_handler_timeout() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("sleep 60".to_string()),
        );
        node.attrs.insert(
            "timeout".to_string(),
            AttrValue::Duration(Duration::from_millis(50)),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        let outcome = handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();
        assert_eq!(outcome.status, StageStatus::Fail);
        assert!(
            outcome
                .failure_reason
                .as_deref()
                .unwrap()
                .contains("timed out"),
            "expected timeout message, got: {:?}",
            outcome.failure_reason
        );
    }

    #[tokio::test]
    async fn writes_tool_invocation_json() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("echo hello".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let invocation_path = logs_root
            .path()
            .join("tool_node")
            .join("tool_invocation.json");
        let content = std::fs::read_to_string(&invocation_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["command"], "echo hello");
        assert_eq!(json["timeout_ms"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn writes_tool_invocation_json_with_timeout() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("echo hello".to_string()),
        );
        node.attrs.insert(
            "timeout".to_string(),
            AttrValue::Duration(Duration::from_millis(5000)),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let invocation_path = logs_root
            .path()
            .join("tool_node")
            .join("tool_invocation.json");
        let content = std::fs::read_to_string(&invocation_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["command"], "echo hello");
        assert_eq!(json["timeout_ms"], 5000);
    }

    #[tokio::test]
    async fn writes_stdout_and_stderr_logs() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("echo hello".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let stage_dir = logs_root.path().join("tool_node");
        let stdout = std::fs::read_to_string(stage_dir.join("stdout.log")).unwrap();
        assert_eq!(stdout.trim(), "hello");
        let stderr = std::fs::read_to_string(stage_dir.join("stderr.log")).unwrap();
        assert_eq!(stderr, "");
    }

    #[tokio::test]
    async fn writes_stderr_log_on_failure() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("echo oops >&2 && false".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let stage_dir = logs_root.path().join("tool_node");
        let stderr = std::fs::read_to_string(stage_dir.join("stderr.log")).unwrap();
        assert_eq!(stderr.trim(), "oops");
    }

    #[tokio::test]
    async fn writes_tool_timing_json_on_success() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("echo hello".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let timing_path = logs_root.path().join("tool_node").join("tool_timing.json");
        let content = std::fs::read_to_string(&timing_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["duration_ms"].as_u64().unwrap() >= 0);
        assert_eq!(json["exit_code"], 0);
        assert_eq!(json["timed_out"], false);
    }

    #[tokio::test]
    async fn writes_tool_timing_json_on_failure() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("false".to_string()),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let timing_path = logs_root.path().join("tool_node").join("tool_timing.json");
        let content = std::fs::read_to_string(&timing_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["exit_code"], 1);
        assert_eq!(json["timed_out"], false);
    }

    #[tokio::test]
    async fn writes_tool_timing_json_on_timeout() {
        let handler = ToolHandler;
        let mut node = Node::new("tool_node");
        node.attrs.insert(
            "tool_command".to_string(),
            AttrValue::String("sleep 60".to_string()),
        );
        node.attrs.insert(
            "timeout".to_string(),
            AttrValue::Duration(Duration::from_millis(50)),
        );
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = tempfile::tempdir().unwrap();

        handler
            .execute(&node, &context, &graph, logs_root.path(), &make_services())
            .await
            .unwrap();

        let timing_path = logs_root.path().join("tool_node").join("tool_timing.json");
        let content = std::fs::read_to_string(&timing_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["duration_ms"].as_u64().unwrap() >= 0);
        assert_eq!(json["exit_code"], serde_json::Value::Null);
        assert_eq!(json["timed_out"], true);
    }
}
