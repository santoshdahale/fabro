use async_trait::async_trait;
use std::fmt::Write;
use tokio_util::sync::CancellationToken;

/// Formats file content with line numbers for display.
///
/// Applies optional offset (0-based lines to skip) and limit (max lines to return).
/// Line numbers are 1-based and right-aligned.
pub fn format_lines_numbered(content: &str, offset: Option<usize>, limit: Option<usize>) -> String {
    let all_lines: Vec<&str> = content.lines().collect();
    let skip = offset.unwrap_or(0);
    let take = limit.unwrap_or(all_lines.len());
    let selected: Vec<&str> = all_lines.into_iter().skip(skip).take(take).collect();
    let width = (skip + selected.len()).to_string().len().max(1);
    let mut result = String::new();
    for (i, line) in selected.iter().enumerate() {
        let line_num = skip + i + 1;
        let _ = writeln!(result, "{line_num:>width$} | {line}");
    }
    result
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct GrepOptions {
    pub glob_filter: Option<String>,
    pub case_insensitive: bool,
    pub max_results: Option<usize>,
}

#[async_trait]
pub trait ExecutionEnvironment: Send + Sync {
    async fn read_file(&self, path: &str, offset: Option<usize>, limit: Option<usize>) -> Result<String, String>;
    async fn write_file(&self, path: &str, content: &str) -> Result<(), String>;
    async fn delete_file(&self, path: &str) -> Result<(), String>;
    async fn file_exists(&self, path: &str) -> Result<bool, String>;
    async fn list_directory(&self, path: &str, depth: Option<usize>) -> Result<Vec<DirEntry>, String>;
    async fn exec_command(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&std::collections::HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<ExecResult, String>;
    async fn grep(
        &self,
        pattern: &str,
        path: &str,
        options: &GrepOptions,
    ) -> Result<Vec<String>, String>;
    async fn glob(&self, pattern: &str, path: Option<&str>) -> Result<Vec<String>, String>;
    async fn initialize(&self) -> Result<(), String>;
    async fn cleanup(&self) -> Result<(), String>;
    fn working_directory(&self) -> &str;
    fn platform(&self) -> &str;
    fn os_version(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockExecutionEnvironment;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn mock_env_read_file() {
        let mut files = HashMap::new();
        files.insert("test.rs".into(), "hello".into());
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment {
            files,
            ..Default::default()
        });
        let result = env.read_file("test.rs", None, None).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn mock_env_exec_command() {
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment::default());
        let result = env.exec_command("echo", 5000, None, None, None).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn mock_env_list_directory() {
        let env: Arc<dyn ExecutionEnvironment> = Arc::new(MockExecutionEnvironment::default());
        let entries = env.list_directory("/tmp", None).await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn exec_result_fields() {
        let result = ExecResult {
            stdout: "out".into(),
            stderr: "err".into(),
            exit_code: 1,
            timed_out: true,
            duration_ms: 5000,
        };
        assert_eq!(result.exit_code, 1);
        assert!(result.timed_out);
        assert_eq!(result.duration_ms, 5000);
    }

    #[test]
    fn dir_entry_fields() {
        let entry = DirEntry {
            name: "src".into(),
            is_dir: true,
            size: None,
        };
        assert_eq!(entry.name, "src");
        assert!(entry.is_dir);
        assert!(entry.size.is_none());
    }

    #[test]
    fn grep_options_defaults() {
        let opts = GrepOptions::default();
        assert!(opts.glob_filter.is_none());
        assert!(!opts.case_insensitive);
        assert!(opts.max_results.is_none());
    }

    #[test]
    fn mock_env_platform() {
        let env = MockExecutionEnvironment::default();
        assert_eq!(env.platform(), "darwin");
        assert_eq!(env.working_directory(), "/tmp/test");
        assert_eq!(env.os_version(), "Darwin 24.0.0");
    }
}
