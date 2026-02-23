use std::path::Path;

use async_trait::async_trait;

use crate::context::Context;
use crate::error::AttractorError;
use crate::graph::{Graph, Node};
use crate::outcome::Outcome;

use super::{EngineServices, Handler};

/// Executes an external tool (shell command) configured via node attributes.
pub struct ToolHandler;

fn process_output(
    output: std::process::Output,
    command: &str,
) -> Result<Outcome, AttractorError> {
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

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

#[async_trait]
impl Handler for ToolHandler {
    async fn execute(
        &self,
        node: &Node,
        _context: &Context,
        _graph: &Graph,
        _logs_root: &Path,
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

        let cmd_future = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output();

        let output = if let Some(timeout_dur) = node.timeout() {
            match tokio::time::timeout(timeout_dur, cmd_future).await {
                Ok(result) => result,
                Err(_elapsed) => {
                    return Ok(Outcome::fail(format!(
                        "Tool timed out after {}ms: {command}",
                        timeout_dur.as_millis()
                    )));
                }
            }
        } else {
            cmd_future.await
        };

        match output {
            Ok(output) => process_output(output, command),
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
        }
    }

    #[tokio::test]
    async fn tool_handler_no_command() {
        let handler = ToolHandler;
        let node = Node::new("tool_node");
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = Path::new("/tmp/test");

        let outcome = handler
            .execute(&node, &context, &graph, logs_root, &make_services())
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
        let logs_root = Path::new("/tmp/test");

        let outcome = handler
            .execute(&node, &context, &graph, logs_root, &make_services())
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
        let logs_root = Path::new("/tmp/test");

        let outcome = handler
            .execute(&node, &context, &graph, logs_root, &make_services())
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
        let logs_root = Path::new("/tmp/test");

        let outcome = handler
            .execute(&node, &context, &graph, logs_root, &make_services())
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
}
