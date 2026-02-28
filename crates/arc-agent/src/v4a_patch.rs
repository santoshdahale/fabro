use crate::execution_env::ExecutionEnvironment;
use crate::tool_registry::RegisteredTool;
use arc_llm::types::ToolDefinition;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Remove(String),
    Add(String),
    Context(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub context_line: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOperation {
    Add { path: String, content: String },
    Delete { path: String },
    Update { path: String, hunks: Vec<Hunk> },
}

/// Parses a v4a format patch string into a list of patch operations.
///
/// # Errors
/// Returns an error if the patch format is invalid.
pub fn parse_v4a_patch(text: &str) -> Result<Vec<PatchOperation>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut ops = Vec::new();
    let mut i = 0;

    // Find "*** Begin Patch"
    while i < lines.len() {
        if lines[i].trim() == "*** Begin Patch" {
            i += 1;
            break;
        }
        i += 1;
    }

    while i < lines.len() {
        let line = lines[i].trim();

        if line == "*** End Patch" {
            break;
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let path = path.to_string();
            i += 1;
            let mut content = String::new();
            while i < lines.len() {
                let l = lines[i];
                if l.starts_with("*** ") {
                    break;
                }
                if let Some(text_line) = l.strip_prefix('+') {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(text_line);
                } else {
                    return Err(format!("Expected '+' prefix in Add File block, got: {l}"));
                }
                i += 1;
            }
            ops.push(PatchOperation::Add { path, content });
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            ops.push(PatchOperation::Delete {
                path: path.to_string(),
            });
            i += 1;
        } else if let Some(path) = line.strip_prefix("*** Update File: ") {
            let path = path.to_string();
            i += 1;
            let mut hunks = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.starts_with("*** ") && !l.starts_with("@@ ") {
                    break;
                }
                if l.starts_with("@@ ") && l.ends_with(" @@") {
                    let context_line = l[3..l.len() - 3].to_string();
                    i += 1;
                    let mut changes = Vec::new();
                    while i < lines.len() {
                        let cl = lines[i];
                        if cl.starts_with("*** ") || (cl.starts_with("@@ ") && cl.ends_with(" @@"))
                        {
                            break;
                        }
                        if let Some(removed) = cl.strip_prefix('-') {
                            changes.push(Change::Remove(removed.to_string()));
                        } else if let Some(added) = cl.strip_prefix('+') {
                            changes.push(Change::Add(added.to_string()));
                        } else if let Some(ctx) = cl.strip_prefix(' ') {
                            changes.push(Change::Context(ctx.to_string()));
                        } else if cl.is_empty() {
                            changes.push(Change::Context(String::new()));
                        } else {
                            return Err(format!(
                                "Unexpected line in hunk (expected +, -, or space prefix): {cl}"
                            ));
                        }
                        i += 1;
                    }
                    hunks.push(Hunk {
                        context_line,
                        changes,
                    });
                } else {
                    return Err(format!("Expected @@ context @@ line, got: {l}"));
                }
            }
            ops.push(PatchOperation::Update { path, hunks });
        } else {
            return Err(format!("Unexpected line in patch: {line}"));
        }
    }

    Ok(ops)
}

/// Applies a list of patch operations using the given execution environment.
///
/// # Errors
/// Returns an error if any file operation fails.
pub async fn apply_patch_operations(
    ops: &[PatchOperation],
    env: &dyn ExecutionEnvironment,
) -> Result<String, String> {
    let mut results = Vec::new();

    for op in ops {
        match op {
            PatchOperation::Add { path, content } => {
                env.write_file(path, content).await?;
                results.push(format!("Added file: {path}"));
            }
            PatchOperation::Delete { path } => {
                env.delete_file(path).await?;
                results.push(format!("Deleted file: {path}"));
            }
            PatchOperation::Update { path, hunks } => {
                let original = env.read_file(path, None, None).await?;
                let updated = apply_hunks(&original, hunks)?;
                env.write_file(path, &updated).await?;
                results.push(format!("Updated file: {path}"));
            }
        }
    }

    Ok(results.join("\n"))
}

fn apply_hunks(content: &str, hunks: &[Hunk]) -> Result<String, String> {
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    // Apply hunks in reverse order to preserve line indices
    for hunk in hunks.iter().rev() {
        let context_pos = lines
            .iter()
            .position(|l| l.trim() == hunk.context_line.trim())
            .ok_or_else(|| {
                format!(
                    "Could not find context line in file: '{}'",
                    hunk.context_line
                )
            })?;

        // Build what we expect to find and what to replace with
        let mut new_lines: Vec<String> = Vec::new();

        // The context line itself is part of the hunk context
        // We start replacing at context_pos
        new_lines.push(lines[context_pos].clone());
        let mut file_idx = context_pos + 1;

        for change in &hunk.changes {
            match change {
                Change::Remove(_) => {
                    file_idx += 1;
                }
                Change::Add(text) => {
                    new_lines.push(text.clone());
                }
                Change::Context(_) => {
                    if file_idx < lines.len() {
                        new_lines.push(lines[file_idx].clone());
                    }
                    file_idx += 1;
                }
            }
        }

        // Calculate total lines consumed from original (context_line + removes + context changes)
        let total_original_lines = 1 + hunk
            .changes
            .iter()
            .filter(|c| matches!(c, Change::Remove(_) | Change::Context(_)))
            .count();

        // Replace the range
        let end = (context_pos + total_original_lines).min(lines.len());
        lines.splice(context_pos..end, new_lines);
    }

    Ok(lines.join("\n"))
}

pub fn make_apply_patch_tool() -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name: "apply_patch".into(),
            description: "Apply a v4a format patch to modify files".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "patch": {
                        "type": "string",
                        "description": "The patch content in v4a format"
                    }
                },
                "required": ["patch"]
            }),
        },
        executor: Arc::new(|args, ctx| {
            Box::pin(async move {
                let patch_text = args
                    .get("patch")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: patch".to_string())?;

                let ops = parse_v4a_patch(patch_text)?;
                apply_patch_operations(&ops, ctx.env.as_ref()).await
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MutableMockExecutionEnvironment;
    use std::collections::HashMap;

    #[test]
    fn parse_v4a_add_file() {
        let patch = "\
*** Begin Patch
*** Add File: src/new_file.rs
+fn main() {
+    println!(\"hello\");
+}
*** End Patch";

        let ops = parse_v4a_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            PatchOperation::Add {
                path: "src/new_file.rs".into(),
                content: "fn main() {\n    println!(\"hello\");\n}".into(),
            }
        );
    }

    #[test]
    fn parse_v4a_delete_file() {
        let patch = "\
*** Begin Patch
*** Delete File: src/old_file.rs
*** End Patch";

        let ops = parse_v4a_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            PatchOperation::Delete {
                path: "src/old_file.rs".into(),
            }
        );
    }

    #[test]
    fn parse_v4a_update_file() {
        let patch = "\
*** Begin Patch
*** Update File: src/lib.rs
@@ fn hello() @@
-    println!(\"old\");
+    println!(\"new\");
*** End Patch";

        let ops = parse_v4a_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            PatchOperation::Update { path, hunks } => {
                assert_eq!(path, "src/lib.rs");
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].context_line, "fn hello()");
                assert_eq!(hunks[0].changes.len(), 2);
                assert_eq!(
                    hunks[0].changes[0],
                    Change::Remove("    println!(\"old\");".into())
                );
                assert_eq!(
                    hunks[0].changes[1],
                    Change::Add("    println!(\"new\");".into())
                );
            }
            _ => panic!("Expected Update operation"),
        }
    }

    #[test]
    fn parse_v4a_multi_operation() {
        let patch = "\
*** Begin Patch
*** Add File: src/a.rs
+// file a
*** Delete File: src/b.rs
*** Update File: src/c.rs
@@ fn main() @@
-    old_call();
+    new_call();
*** End Patch";

        let ops = parse_v4a_patch(patch).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], PatchOperation::Add { .. }));
        assert!(matches!(&ops[1], PatchOperation::Delete { .. }));
        assert!(matches!(&ops[2], PatchOperation::Update { .. }));
    }

    #[tokio::test]
    async fn apply_patch_add_file() {
        let env = MutableMockExecutionEnvironment::new(HashMap::new());
        let ops = vec![PatchOperation::Add {
            path: "src/new.rs".into(),
            content: "fn new() {}".into(),
        }];

        let result = apply_patch_operations(&ops, &env).await.unwrap();
        assert!(result.contains("Added file: src/new.rs"));

        let content = env.read_file("src/new.rs", None, None).await.unwrap();
        assert_eq!(content, "fn new() {}");
    }

    #[tokio::test]
    async fn apply_patch_update_file() {
        let mut files = HashMap::new();
        files.insert(
            "src/lib.rs".to_string(),
            "fn hello() {\n    println!(\"old\");\n}".to_string(),
        );
        let env = MutableMockExecutionEnvironment::new(files);

        let ops = vec![PatchOperation::Update {
            path: "src/lib.rs".into(),
            hunks: vec![Hunk {
                context_line: "fn hello() {".into(),
                changes: vec![
                    Change::Remove("    println!(\"old\");".into()),
                    Change::Add("    println!(\"new\");".into()),
                ],
            }],
        }];

        let result = apply_patch_operations(&ops, &env).await.unwrap();
        assert!(result.contains("Updated file: src/lib.rs"));

        let content = env.read_file("src/lib.rs", None, None).await.unwrap();
        assert!(content.contains("println!(\"new\")"));
        assert!(!content.contains("println!(\"old\")"));
    }
}
