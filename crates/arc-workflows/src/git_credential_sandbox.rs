use std::collections::HashMap;
use std::sync::Arc;

use arc_agent::sandbox::{DirEntry, ExecResult, GrepOptions, Sandbox};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::github_app::GitHubAppCredentials;

/// Sandbox decorator that overrides `refresh_push_credentials` to rotate
/// GitHub App tokens on the git remote URL.
///
/// This lets `ExeSandbox` (in `arc-exe`) get credential refresh behaviour
/// without depending on `github_app` (which lives in `arc-workflows`).
pub struct GitCredentialSandbox {
    inner: Arc<dyn Sandbox>,
    github_app: Option<GitHubAppCredentials>,
}

impl GitCredentialSandbox {
    pub fn new(inner: Arc<dyn Sandbox>, github_app: Option<GitHubAppCredentials>) -> Self {
        Self { inner, github_app }
    }
}

#[async_trait]
impl Sandbox for GitCredentialSandbox {
    async fn initialize(&self) -> Result<(), String> {
        self.inner.initialize().await
    }

    async fn cleanup(&self) -> Result<(), String> {
        self.inner.cleanup().await
    }

    async fn exec_command(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<ExecResult, String> {
        self.inner
            .exec_command(command, timeout_ms, working_dir, env_vars, cancel_token)
            .await
    }

    async fn read_file(
        &self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<String, String> {
        self.inner.read_file(path, offset, limit).await
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        self.inner.write_file(path, content).await
    }

    async fn delete_file(&self, path: &str) -> Result<(), String> {
        self.inner.delete_file(path).await
    }

    async fn file_exists(&self, path: &str) -> Result<bool, String> {
        self.inner.file_exists(path).await
    }

    async fn list_directory(
        &self,
        path: &str,
        depth: Option<usize>,
    ) -> Result<Vec<DirEntry>, String> {
        self.inner.list_directory(path, depth).await
    }

    async fn grep(
        &self,
        pattern: &str,
        path: &str,
        options: &GrepOptions,
    ) -> Result<Vec<String>, String> {
        self.inner.grep(pattern, path, options).await
    }

    async fn glob(&self, pattern: &str, path: Option<&str>) -> Result<Vec<String>, String> {
        self.inner.glob(pattern, path).await
    }

    async fn download_file_to_local(
        &self,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> Result<(), String> {
        self.inner
            .download_file_to_local(remote_path, local_path)
            .await
    }

    async fn upload_file_from_local(
        &self,
        local_path: &std::path::Path,
        remote_path: &str,
    ) -> Result<(), String> {
        self.inner
            .upload_file_from_local(local_path, remote_path)
            .await
    }

    fn working_directory(&self) -> &str {
        self.inner.working_directory()
    }

    fn platform(&self) -> &str {
        self.inner.platform()
    }

    fn os_version(&self) -> String {
        self.inner.os_version()
    }

    fn sandbox_info(&self) -> String {
        self.inner.sandbox_info()
    }

    async fn refresh_push_credentials(&self) -> Result<(), String> {
        let origin_url = match self.inner.origin_url() {
            Some(url) => url,
            None => return Ok(()),
        };
        let creds = match &self.github_app {
            Some(c) => c,
            None => return Ok(()),
        };

        let (owner, repo) = crate::github_app::parse_github_owner_repo(origin_url)
            .map_err(|e| format!("Failed to parse origin URL for credential refresh: {e}"))?;

        let (_username, password) =
            crate::github_app::resolve_clone_credentials(creds, &owner, &repo)
                .await
                .map_err(|e| format!("Failed to refresh GitHub App token: {e}"))?;

        if let Some(token) = password {
            let auth_url =
                origin_url.replacen("https://", &format!("https://x-access-token:{token}@"), 1);
            let quoted = shlex::try_quote(&auth_url).map_or_else(
                |_| format!("'{}'", auth_url.replace('\'', "'\\''")),
                |q| q.to_string(),
            );
            let cmd = format!("git -c maintenance.auto=0 remote set-url origin {quoted}");
            self.inner
                .exec_command(&cmd, 10_000, None, None, None)
                .await
                .map_err(|e| format!("Failed to set refreshed push credentials: {e}"))?;
        }

        Ok(())
    }

    async fn set_autostop_interval(&self, minutes: i32) -> Result<(), String> {
        self.inner.set_autostop_interval(minutes).await
    }

    fn is_remote(&self) -> bool {
        self.inner.is_remote()
    }

    async fn ssh_access_command(&self) -> Result<Option<String>, String> {
        self.inner.ssh_access_command().await
    }

    fn origin_url(&self) -> Option<&str> {
        self.inner.origin_url()
    }
}
