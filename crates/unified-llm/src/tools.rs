use crate::types::{ToolCall, ToolDefinition, ToolResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// An execute handler for a tool.
pub type ExecuteHandler = Arc<
    dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>
        + Send
        + Sync,
>;

/// A tool with an optional execute handler (Section 5.1, 5.5).
/// "Active" tools have an execute handler and are automatically executed.
/// "Passive" tools have no handler and are returned to the caller.
pub struct Tool {
    pub definition: ToolDefinition,
    pub execute: Option<ExecuteHandler>,
}

impl Tool {
    /// Create a passive tool (no execute handler).
    ///
    /// # Panics
    ///
    /// Panics if the tool name is invalid (see [`validate_tool_name`]).
    #[must_use]
    pub fn passive(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        if let Err(e) = validate_tool_name(name) {
            panic!("Invalid tool name: {e}");
        }
        Self {
            definition: ToolDefinition {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
            },
            execute: None,
        }
    }

    /// Create an active tool with an execute handler.
    ///
    /// # Panics
    ///
    /// Panics if the tool name is invalid (see [`validate_tool_name`]).
    pub fn active<F, Fut>(
        name: &str,
        description: &str,
        parameters: serde_json::Value,
        handler: F,
    ) -> Self
    where
        F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<serde_json::Value, String>> + Send + 'static,
    {
        if let Err(e) = validate_tool_name(name) {
            panic!("Invalid tool name: {e}");
        }
        Self {
            definition: ToolDefinition {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
            },
            execute: Some(Arc::new(move |args| Box::pin(handler(args)))),
        }
    }

    #[must_use] 
    pub fn is_active(&self) -> bool {
        self.execute.is_some()
    }
}

/// Validate a tool name: [a-zA-Z][a-zA-Z0-9_]* max 64 chars (Section 5.1).
///
/// # Errors
///
/// Returns a description of the validation failure if the name is empty,
/// too long, starts with a non-letter, or contains invalid characters.
pub fn validate_tool_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Tool name cannot be empty".to_string());
    }
    if name.len() > 64 {
        return Err(format!(
            "Tool name '{name}' exceeds 64 character limit"
        ));
    }
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        if !first.is_ascii_alphabetic() {
            return Err(format!(
                "Tool name '{name}' must start with a letter"
            ));
        }
    }
    for ch in chars {
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            return Err(format!(
                "Tool name '{name}' contains invalid character '{ch}'"
            ));
        }
    }
    Ok(())
}

/// Execute all tool calls concurrently (Section 5.7).
/// Returns results in the same order as the input calls.
///
/// # Panics
///
/// Panics if a matched tool has `is_active() == true` but its `execute` handler is `None`.
pub async fn execute_all_tools(
    tools: &[&Tool],
    tool_calls: &[ToolCall],
) -> Vec<ToolResult> {
    use futures::future::join_all;

    let futures: Vec<_> = tool_calls
        .iter()
        .map(|call| {
            let tool = tools.iter().find(|t| t.definition.name == call.name).copied();
            let call_id = call.id.clone();
            let call_name = call.name.clone();
            let args = call.arguments.clone();

            async move {
                match tool {
                    Some(t) if t.execute.is_some() => {
                        let handler = t.execute.as_ref().unwrap();
                        match handler(args).await {
                            Ok(result) => ToolResult {
                                tool_call_id: call_id,
                                content: result,
                                is_error: false,
                                image_data: None,
                                image_media_type: None,
                            },
                            Err(err_msg) => ToolResult {
                                tool_call_id: call_id,
                                content: serde_json::Value::String(err_msg),
                                is_error: true,
                                image_data: None,
                                image_media_type: None,
                            },
                        }
                    }
                    _ => ToolResult {
                        tool_call_id: call_id,
                        content: serde_json::Value::String(format!(
                            "Unknown tool: {call_name}"
                        )),
                        is_error: true,
                        image_data: None,
                        image_media_type: None,
                    },
                }
            }
        })
        .collect();

    join_all(futures).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tool_name_valid() {
        assert!(validate_tool_name("get_weather").is_ok());
        assert!(validate_tool_name("a").is_ok());
        assert!(validate_tool_name("myTool123").is_ok());
        assert!(validate_tool_name("A_B_C").is_ok());
    }

    #[test]
    fn validate_tool_name_empty() {
        assert!(validate_tool_name("").is_err());
    }

    #[test]
    fn validate_tool_name_starts_with_number() {
        assert!(validate_tool_name("1tool").is_err());
    }

    #[test]
    fn validate_tool_name_starts_with_underscore() {
        assert!(validate_tool_name("_tool").is_err());
    }

    #[test]
    fn validate_tool_name_contains_dash() {
        assert!(validate_tool_name("my-tool").is_err());
    }

    #[test]
    fn validate_tool_name_too_long() {
        let name = "a".repeat(65);
        assert!(validate_tool_name(&name).is_err());
    }

    #[test]
    fn validate_tool_name_max_length_ok() {
        let name = "a".repeat(64);
        assert!(validate_tool_name(&name).is_ok());
    }

    #[test]
    fn passive_tool_is_not_active() {
        let tool = Tool::passive(
            "test",
            "test tool",
            serde_json::json!({"type": "object", "properties": {}}),
        );
        assert!(!tool.is_active());
    }

    #[test]
    fn active_tool_is_active() {
        let tool = Tool::active(
            "test",
            "test tool",
            serde_json::json!({"type": "object", "properties": {}}),
            |_args| async { Ok(serde_json::json!("result")) },
        );
        assert!(tool.is_active());
    }

    #[tokio::test]
    async fn execute_all_tools_with_known_tools() {
        let tools = vec![Tool::active(
            "greet",
            "Greet someone",
            serde_json::json!({"type": "object", "properties": {"name": {"type": "string"}}}),
            |args| async move {
                let name = args["name"].as_str().unwrap_or("world");
                Ok(serde_json::json!(format!("Hello, {}!", name)))
            },
        )];

        let calls = vec![ToolCall::new(
            "call_1",
            "greet",
            serde_json::json!({"name": "Alice"}),
        )];

        let tool_refs: Vec<&Tool> = tools.iter().collect();
        let results = execute_all_tools(&tool_refs, &calls).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_call_id, "call_1");
        assert!(!results[0].is_error);
        assert_eq!(results[0].content, serde_json::json!("Hello, Alice!"));
    }

    #[tokio::test]
    async fn execute_all_tools_with_unknown_tool() {
        let tools = vec![];

        let calls = vec![ToolCall::new(
            "call_1",
            "nonexistent",
            serde_json::json!({}),
        )];

        let tool_refs: Vec<&Tool> = tools.iter().collect();
        let results = execute_all_tools(&tool_refs, &calls).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert!(results[0]
            .content
            .as_str()
            .unwrap()
            .contains("Unknown tool"));
    }

    #[tokio::test]
    async fn execute_all_tools_handler_error() {
        let tools = vec![Tool::active(
            "fail",
            "Always fails",
            serde_json::json!({"type": "object", "properties": {}}),
            |_args| async { Err("something went wrong".to_string()) },
        )];

        let calls = vec![ToolCall::new(
            "call_1",
            "fail",
            serde_json::json!({}),
        )];

        let tool_refs: Vec<&Tool> = tools.iter().collect();
        let results = execute_all_tools(&tool_refs, &calls).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert_eq!(
            results[0].content,
            serde_json::json!("something went wrong")
        );
    }

    #[tokio::test]
    async fn execute_all_tools_concurrent_multiple() {
        let tools = vec![
            Tool::active(
                "tool_a",
                "Tool A",
                serde_json::json!({"type": "object", "properties": {}}),
                |_args| async { Ok(serde_json::json!("result_a")) },
            ),
            Tool::active(
                "tool_b",
                "Tool B",
                serde_json::json!({"type": "object", "properties": {}}),
                |_args| async { Ok(serde_json::json!("result_b")) },
            ),
        ];

        let calls = vec![
            ToolCall::new("call_1", "tool_a", serde_json::json!({})),
            ToolCall::new("call_2", "tool_b", serde_json::json!({})),
        ];

        let tool_refs: Vec<&Tool> = tools.iter().collect();
        let results = execute_all_tools(&tool_refs, &calls).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_call_id, "call_1");
        assert_eq!(results[0].content, serde_json::json!("result_a"));
        assert_eq!(results[1].tool_call_id, "call_2");
        assert_eq!(results[1].content, serde_json::json!("result_b"));
    }

    #[tokio::test]
    async fn execute_all_tools_partial_failure() {
        let tools = vec![
            Tool::active(
                "succeed",
                "Succeeds",
                serde_json::json!({"type": "object", "properties": {}}),
                |_args| async { Ok(serde_json::json!("ok")) },
            ),
            Tool::active(
                "fail",
                "Fails",
                serde_json::json!({"type": "object", "properties": {}}),
                |_args| async { Err("boom".to_string()) },
            ),
        ];

        let calls = vec![
            ToolCall::new("call_1", "succeed", serde_json::json!({})),
            ToolCall::new("call_2", "fail", serde_json::json!({})),
        ];

        let tool_refs: Vec<&Tool> = tools.iter().collect();
        let results = execute_all_tools(&tool_refs, &calls).await;
        assert_eq!(results.len(), 2);
        assert!(!results[0].is_error);
        assert!(results[1].is_error);
    }

    #[test]
    #[should_panic(expected = "Invalid tool name")]
    fn passive_tool_panics_on_invalid_name() {
        let _ = Tool::passive(
            "1invalid",
            "bad name",
            serde_json::json!({"type": "object"}),
        );
    }

    #[test]
    #[should_panic(expected = "Invalid tool name")]
    fn active_tool_panics_on_invalid_name() {
        Tool::active(
            "my-tool",
            "bad name",
            serde_json::json!({"type": "object"}),
            |_args| async { Ok(serde_json::json!("result")) },
        );
    }
}
