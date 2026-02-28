use crate::execution_env::*;
use std::collections::HashSet;
use std::path::{Component, PathBuf};
use std::sync::{Arc, Mutex};

/// Decorator that prevents writing to files the agent hasn't read first.
///
/// Tracks which file paths the agent has seen (via `read_file` or `grep`) and
/// returns an error when `write_file` or `delete_file` targets an existing file
/// that hasn't been read. Writing to new (non-existent) files is always allowed.
pub struct ReadBeforeWriteEnvironment {
    inner: Arc<dyn ExecutionEnvironment>,
    read_set: Mutex<HashSet<String>>,
}

impl ReadBeforeWriteEnvironment {
    pub fn new(inner: Arc<dyn ExecutionEnvironment>) -> Self {
        Self {
            inner,
            read_set: Mutex::new(HashSet::new()),
        }
    }

    fn normalize_path(&self, path: &str) -> String {
        let full = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            PathBuf::from(self.inner.working_directory()).join(path)
        };

        let mut parts: Vec<String> = Vec::new();
        for component in full.components() {
            match component {
                Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    parts.pop();
                }
                Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            }
        }

        format!("/{}", parts.join("/"))
    }

    fn mark_read(&self, path: &str) {
        let normalized = self.normalize_path(path);
        self.read_set
            .lock()
            .expect("read_set lock poisoned")
            .insert(normalized);
    }

    fn has_read(&self, path: &str) -> bool {
        let normalized = self.normalize_path(path);
        self.read_set
            .lock()
            .expect("read_set lock poisoned")
            .contains(&normalized)
    }

    async fn guard_write(&self, path: &str) -> Result<(), String> {
        let exists = self.inner.file_exists(path).await?;
        if exists && !self.has_read(path) {
            Err(format!(
                "Cannot write to '{path}': file exists but has not been read. \
                 Use read_file to read the file before writing to it."
            ))
        } else {
            Ok(())
        }
    }
}

crate::delegate_execution_env! {
    ReadBeforeWriteEnvironment => inner {
        async fn read_file(
            &self,
            path: &str,
            offset: Option<usize>,
            limit: Option<usize>,
        ) -> Result<String, String> {
            let result = self.inner.read_file(path, offset, limit).await?;
            self.mark_read(path);
            Ok(result)
        }

        async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
            self.guard_write(path).await?;
            self.inner.write_file(path, content).await
        }

        async fn delete_file(&self, path: &str) -> Result<(), String> {
            self.guard_write(path).await?;
            self.inner.delete_file(path).await
        }

        async fn grep(
            &self,
            pattern: &str,
            path: &str,
            options: &GrepOptions,
        ) -> Result<Vec<String>, String> {
            let results = self.inner.grep(pattern, path, options).await?;
            for line in &results {
                if let Some(file_path) = line.split(':').next() {
                    if !file_path.is_empty() {
                        self.mark_read(file_path);
                    }
                }
            }
            Ok(results)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockExecutionEnvironment;
    use std::collections::HashMap;

    // Cycle 1: write to existing unread file → error
    #[tokio::test]
    async fn write_to_existing_unread_file_returns_error() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([("a.ts".into(), "content".into())]),
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        let result = env.write_file("a.ts", "new content").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("a.ts"));
        assert!(err.contains("read"));
    }

    // Cycle 2: write to non-existent file → success
    #[tokio::test]
    async fn write_to_nonexistent_file_succeeds() {
        let mock = MockExecutionEnvironment::default();
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        let result = env.write_file("new.ts", "content").await;

        assert!(result.is_ok());
    }

    // Cycle 3: read then write → success
    #[tokio::test]
    async fn read_then_write_succeeds() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([("a.ts".into(), "content".into())]),
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        env.read_file("a.ts", None, None).await.unwrap();
        let result = env.write_file("a.ts", "new content").await;

        assert!(result.is_ok());
    }

    // Cycle 4: grep results populate read set
    #[tokio::test]
    async fn grep_populates_read_set() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([("b.ts".into(), "content".into())]),
            grep_results: vec!["b.ts:1:content".into()],
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        env.grep("pattern", ".", &GrepOptions::default())
            .await
            .unwrap();
        let result = env.write_file("b.ts", "new").await;

        assert!(result.is_ok());
    }

    // Cycle 5: glob does NOT populate read set
    #[tokio::test]
    async fn glob_does_not_populate_read_set() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([("c.ts".into(), "content".into())]),
            glob_results: vec!["c.ts".into()],
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        env.glob("*.ts", None).await.unwrap();
        let result = env.write_file("c.ts", "new").await;

        assert!(result.is_err());
    }

    // Cycle 6: path normalization — relative vs absolute
    #[tokio::test]
    async fn path_normalization_relative_and_absolute() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([
                ("a.ts".into(), "content".into()),
                ("/work/a.ts".into(), "content".into()),
            ]),
            working_dir: "/work",
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        env.read_file("a.ts", None, None).await.unwrap();
        let result = env.write_file("/work/a.ts", "new content").await;

        assert!(result.is_ok());
    }

    // Cycle 7: delete unread file → error
    #[tokio::test]
    async fn delete_unread_file_returns_error() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([("d.ts".into(), "content".into())]),
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        let result = env.delete_file("d.ts").await;

        assert!(result.is_err());
    }

    // Cycle 8: error message is actionable
    #[tokio::test]
    async fn error_message_is_actionable() {
        let mock = MockExecutionEnvironment {
            files: HashMap::from([("main.rs".into(), "fn main() {}".into())]),
            ..Default::default()
        };
        let env = ReadBeforeWriteEnvironment::new(Arc::new(mock));

        let err = env.write_file("main.rs", "new").await.unwrap_err();

        assert!(err.contains("main.rs"));
        assert!(err.contains("read_file"));
    }
}
