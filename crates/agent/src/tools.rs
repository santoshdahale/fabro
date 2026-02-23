use crate::config::SessionConfig;
use crate::execution_env::GrepOptions;
use crate::tool_registry::RegisteredTool;
use std::fmt::Write;
use std::sync::Arc;
use llm::types::ToolDefinition;

pub(crate) fn required_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {key}"))
}

#[must_use]
pub fn make_read_file_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "read_file".into(),
            description: "Read the contents of a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {"type": "string", "description": "Absolute path to the file"},
                    "offset": {"type": "integer", "description": "1-based line number to start reading from"},
                    "limit": {"type": "integer", "description": "Number of lines to read (default 2000)"}
                },
                "required": ["file_path"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let file_path = required_str(&args, "file_path")?;
                let offset = args.get("offset").and_then(serde_json::Value::as_u64);
                let limit = args.get("limit").and_then(serde_json::Value::as_u64);

                #[allow(clippy::cast_possible_truncation)]
                let offset_usize = offset.map(|v| v as usize);
                #[allow(clippy::cast_possible_truncation)]
                let limit_usize = limit.map(|v| v as usize);

                let content = env.read_file(file_path, offset_usize, limit_usize).await?;
                Ok(content)
            })
        }),
    }
}

#[must_use]
pub fn make_write_file_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "write_file".into(),
            description: "Write content to a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {"type": "string", "description": "Absolute path to the file"},
                    "content": {"type": "string", "description": "Content to write to the file"}
                },
                "required": ["file_path", "content"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let file_path = required_str(&args, "file_path")?;
                let content = required_str(&args, "content")?;

                env.write_file(file_path, content).await?;
                Ok(format!("Successfully wrote to {file_path}"))
            })
        }),
    }
}

#[must_use]
pub fn make_edit_file_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "edit_file".into(),
            description: "Edit a file by replacing a string".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {"type": "string", "description": "Absolute path to the file"},
                    "old_string": {"type": "string", "description": "The string to find and replace"},
                    "new_string": {"type": "string", "description": "The replacement string"},
                    "replace_all": {"type": "boolean", "description": "Replace all occurrences (default false)"}
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let file_path = required_str(&args, "file_path")?;
                let old_string = required_str(&args, "old_string")?;
                let new_string = required_str(&args, "new_string")?;
                let replace_all = args
                    .get("replace_all")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);

                let numbered_content = env.read_file(file_path, None, None).await?;

                // Strip line numbers: each line looks like "  1 | content" or " 10 | content"
                let raw_lines: Vec<&str> = numbered_content
                    .lines()
                    .map(|line| {
                        line.find(" | ")
                            .map_or(line, |idx| &line[idx + 3..])
                    })
                    .collect();
                let raw_content = raw_lines.join("\n");

                let count = raw_content.matches(old_string).count();
                if count == 0 {
                    return Err("old_string not found in file".to_string());
                }
                if count > 1 && !replace_all {
                    return Err(format!(
                        "old_string is not unique in file (found {count} occurrences). Use replace_all or provide more context"
                    ));
                }

                let new_content = if replace_all {
                    raw_content.replace(old_string, new_string)
                } else {
                    raw_content.replacen(old_string, new_string, 1)
                };

                env.write_file(file_path, &new_content).await?;
                Ok(format!("Successfully edited {file_path}"))
            })
        }),
    }
}

#[must_use]
pub fn make_shell_tool() -> RegisteredTool {
    make_shell_tool_with_config(&SessionConfig::default())
}

#[must_use]
pub fn make_shell_tool_with_config(config: &SessionConfig) -> RegisteredTool {
    let default_timeout = config.default_command_timeout_ms;
    let max_timeout = config.max_command_timeout_ms;
    RegisteredTool {
        definition: ToolDefinition {
            name: "shell".into(),
            description: "Execute a shell command".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The shell command to execute"},
                    "timeout_ms": {"type": "integer", "description": "Timeout in milliseconds"},
                    "description": {"type": "string", "description": "Description of what this command does"}
                },
                "required": ["command"]
            }),
        },
        executor: Arc::new(move |args, env, cancel| {
            Box::pin(async move {
                let command = required_str(&args, "command")?;
                let timeout_ms = args
                    .get("timeout_ms")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(default_timeout)
                    .min(max_timeout);

                let result = env
                    .exec_command(command, timeout_ms, None, None, Some(cancel))
                    .await?;

                let mut output = String::new();
                if result.timed_out {
                    output.push_str("Command timed out.\n");
                }
                let _ = write!(
                    output,
                    "Exit code: {}\nstdout:\n{}\nstderr:\n{}",
                    result.exit_code, result.stdout, result.stderr
                );
                Ok(output)
            })
        }),
    }
}

#[must_use]
pub fn make_grep_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "grep".into(),
            description: "Search file contents with a regex pattern".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex pattern to search for"},
                    "path": {"type": "string", "description": "Path to search in (default \".\")"},
                    "glob_filter": {"type": "string", "description": "Glob pattern to filter files"},
                    "case_insensitive": {"type": "boolean", "description": "Case insensitive search"},
                    "max_results": {"type": "integer", "description": "Maximum number of results"}
                },
                "required": ["pattern"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let pattern = required_str(&args, "pattern")?;
                let path = args
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(".");

                #[allow(clippy::cast_possible_truncation)]
                let options = GrepOptions {
                    glob_filter: args
                        .get("glob_filter")
                        .and_then(serde_json::Value::as_str)
                        .map(String::from),
                    case_insensitive: args
                        .get("case_insensitive")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    max_results: args
                        .get("max_results")
                        .and_then(serde_json::Value::as_u64)
                        .map(|v| v as usize),
                };

                let results = env.grep(pattern, path, &options).await?;
                Ok(results.join("\n"))
            })
        }),
    }
}

#[must_use]
pub fn make_glob_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "glob".into(),
            description: "Find files matching a glob pattern".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern to match files"},
                    "path": {"type": "string", "description": "Directory to search in (default: working directory)"}
                },
                "required": ["pattern"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let pattern = required_str(&args, "pattern")?;
                let path = args
                    .get("path")
                    .and_then(serde_json::Value::as_str);

                let results = env.glob(pattern, path).await?;
                Ok(results.join("\n"))
            })
        }),
    }
}

#[must_use]
pub(crate) fn make_read_many_files_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "read_many_files".into(),
            description: "Read multiple files at once".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Array of absolute file paths to read"
                    }
                },
                "required": ["paths"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let paths = args["paths"]
                    .as_array()
                    .ok_or_else(|| "paths must be an array".to_string())?;

                let mut output = String::new();
                for path_val in paths {
                    let path = path_val
                        .as_str()
                        .ok_or_else(|| "each path must be a string".to_string())?;
                    match env.read_file(path, None, None).await {
                        Ok(content) => {
                            let _ = write!(output, "=== {path} ===\n{content}\n\n");
                        }
                        Err(err) => {
                            let _ = write!(output, "=== {path} ===\nError: {err}\n\n");
                        }
                    }
                }
                Ok(output)
            })
        }),
    }
}

#[must_use]
pub(crate) fn make_list_dir_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "list_dir".into(),
            description: "List directory contents with depth control".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path to list"},
                    "depth": {"type": "integer", "description": "Depth of listing (default 1)"}
                },
                "required": ["path"]
            }),
        },
        executor: Arc::new(|args, env, _cancel| {
            Box::pin(async move {
                let path = required_str(&args, "path")?;
                #[allow(clippy::cast_possible_truncation)]
                let depth = args
                    .get("depth")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as usize);

                let entries = env.list_directory(path, depth).await?;
                let lines: Vec<String> = entries
                    .iter()
                    .map(|e| {
                        if e.is_dir {
                            format!("{}/", e.name)
                        } else {
                            e.name.clone()
                        }
                    })
                    .collect();
                Ok(lines.join("\n"))
            })
        }),
    }
}

#[must_use]
pub(crate) fn make_web_search_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "web_search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "max_results": {"type": "integer", "description": "Maximum number of results"}
                },
                "required": ["query"]
            }),
        },
        executor: Arc::new(|_args, _env, _cancel| {
            Box::pin(async move {
                Ok("Web search is not configured. This is a placeholder tool.".to_string())
            })
        }),
    }
}

#[must_use]
pub(crate) fn make_web_fetch_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "web_fetch".into(),
            description: "Fetch content from a URL".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch"}
                },
                "required": ["url"]
            }),
        },
        executor: Arc::new(|_args, _env, _cancel| {
            Box::pin(async move {
                Ok("Web fetch is not configured. This is a placeholder tool.".to_string())
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_env::*;
    use crate::test_support::MockExecutionEnvironment;
    use std::collections::HashMap;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn read_file_returns_content() {
        let tool = make_read_file_tool();
        let mut files = HashMap::new();
        files.insert("/test.txt".into(), "  1 | hello\n  2 | world".into());
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            files,
            apply_read_offset_limit: true,
            ..Default::default()
        });
        let result = (tool.executor)(serde_json::json!({"file_path": "/test.txt"}), env, CancellationToken::new()).await;
        assert_eq!(result.unwrap(), "  1 | hello\n  2 | world");
    }

    #[tokio::test]
    async fn read_file_with_offset_and_limit() {
        let tool = make_read_file_tool();
        let mut files = HashMap::new();
        files.insert(
            "/test.txt".into(),
            "  1 | line1\n  2 | line2\n  3 | line3\n  4 | line4".into(),
        );
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            files,
            apply_read_offset_limit: true,
            ..Default::default()
        });
        let result = (tool.executor)(
            serde_json::json!({"file_path": "/test.txt", "offset": 2, "limit": 2}),
            env,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap(), "  2 | line2\n  3 | line3");
    }

    #[tokio::test]
    async fn write_file_calls_env() {
        let tool = make_write_file_tool();
        let env = Arc::new(MockExecutionEnvironment::default());
        let env_clone: Arc<dyn ExecutionEnvironment> = env.clone();
        let result = (tool.executor)(
            serde_json::json!({"file_path": "/out.txt", "content": "hello"}),
            env_clone,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap(), "Successfully wrote to /out.txt");
        let written = env.written_files.lock().unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "/out.txt");
        assert_eq!(written[0].1, "hello");
    }

    #[tokio::test]
    async fn edit_file_replaces_match() {
        let tool = make_edit_file_tool();
        let mut files = HashMap::new();
        files.insert("/f.txt".into(), "  1 | hello world".into());
        let env = Arc::new(MockExecutionEnvironment {
            files,
            ..Default::default()
        });
        let env_clone: Arc<dyn ExecutionEnvironment> = env.clone();
        let result = (tool.executor)(
            serde_json::json!({
                "file_path": "/f.txt",
                "old_string": "hello",
                "new_string": "goodbye"
            }),
            env_clone,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap(), "Successfully edited /f.txt");
        let written = env.written_files.lock().unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].1, "goodbye world");
    }

    #[tokio::test]
    async fn edit_file_not_found_error() {
        let tool = make_edit_file_tool();
        let mut files = HashMap::new();
        files.insert("/f.txt".into(), "  1 | hello world".into());
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            files,
            ..Default::default()
        });
        let result = (tool.executor)(
            serde_json::json!({
                "file_path": "/f.txt",
                "old_string": "missing",
                "new_string": "replacement"
            }),
            env,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap_err(), "old_string not found in file");
    }

    #[tokio::test]
    async fn edit_file_not_unique_error() {
        let tool = make_edit_file_tool();
        let mut files = HashMap::new();
        files.insert("/f.txt".into(), "  1 | aa bb aa".into());
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            files,
            ..Default::default()
        });
        let result = (tool.executor)(
            serde_json::json!({
                "file_path": "/f.txt",
                "old_string": "aa",
                "new_string": "cc"
            }),
            env,
            CancellationToken::new(),
        )
        .await;
        let err = result.unwrap_err();
        assert!(err.contains("not unique"));
        assert!(err.contains("2 occurrences"));
    }

    #[tokio::test]
    async fn edit_file_replace_all() {
        let tool = make_edit_file_tool();
        let mut files = HashMap::new();
        files.insert("/f.txt".into(), "  1 | aa bb aa".into());
        let env = Arc::new(MockExecutionEnvironment {
            files,
            ..Default::default()
        });
        let env_clone: Arc<dyn ExecutionEnvironment> = env.clone();
        let result = (tool.executor)(
            serde_json::json!({
                "file_path": "/f.txt",
                "old_string": "aa",
                "new_string": "cc",
                "replace_all": true
            }),
            env_clone,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap(), "Successfully edited /f.txt");
        let written = env.written_files.lock().unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].1, "cc bb cc");
    }

    #[tokio::test]
    async fn shell_basic_command() {
        let tool = make_shell_tool();
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            exec_result: ExecResult {
                stdout: "hello".into(),
                stderr: String::new(),
                exit_code: 0,
                timed_out: false,
                duration_ms: 10,
            },
            ..Default::default()
        });
        let result = (tool.executor)(serde_json::json!({"command": "echo hello"}), env, CancellationToken::new()).await;
        let output = result.unwrap();
        assert!(output.contains("Exit code: 0"));
        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn shell_with_timeout() {
        let tool = make_shell_tool();
        let env = Arc::new(MockExecutionEnvironment::default());
        let env_clone: Arc<dyn ExecutionEnvironment> = env.clone();
        let _result = (tool.executor)(
            serde_json::json!({"command": "sleep 1", "timeout_ms": 5000}),
            env_clone,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(*env.captured_timeout.lock().unwrap(), Some(5000));
    }

    #[tokio::test]
    async fn shell_nonzero_exit_code() {
        let tool = make_shell_tool();
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            exec_result: ExecResult {
                stdout: String::new(),
                stderr: "error".into(),
                exit_code: 1,
                timed_out: false,
                duration_ms: 10,
            },
            ..Default::default()
        });
        let result = (tool.executor)(serde_json::json!({"command": "false"}), env, CancellationToken::new()).await;
        let output = result.unwrap();
        assert!(output.contains("Exit code: 1"));
        assert!(output.contains("error"));
    }

    #[tokio::test]
    async fn shell_timeout_output() {
        let tool = make_shell_tool();
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            exec_result: ExecResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                timed_out: true,
                duration_ms: 10000,
            },
            ..Default::default()
        });
        let result = (tool.executor)(serde_json::json!({"command": "sleep 100"}), env, CancellationToken::new()).await;
        let output = result.unwrap();
        assert!(output.starts_with("Command timed out.\n"));
    }

    #[tokio::test]
    async fn grep_basic() {
        let tool = make_grep_tool();
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            grep_results: vec!["src/main.rs:10:fn main()".into(), "src/lib.rs:5:pub fn".into()],
            ..Default::default()
        });
        let result = (tool.executor)(serde_json::json!({"pattern": "fn"}), env, CancellationToken::new()).await;
        let output = result.unwrap();
        assert!(output.contains("src/main.rs:10:fn main()"));
        assert!(output.contains("src/lib.rs:5:pub fn"));
    }

    #[tokio::test]
    async fn glob_basic() {
        let tool = make_glob_tool();
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            glob_results: vec!["src/main.rs".into(), "src/lib.rs".into()],
            ..Default::default()
        });
        let result = (tool.executor)(serde_json::json!({"pattern": "src/**/*.rs"}), env, CancellationToken::new()).await;
        let output = result.unwrap();
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/lib.rs"));
    }
}
