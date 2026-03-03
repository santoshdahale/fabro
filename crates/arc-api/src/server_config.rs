use std::path::PathBuf;

use arc_workflows::cli::run_config::RunDefaults;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    Github,
    InsecureDisabled,
}

impl Default for AuthProvider {
    fn default() -> Self {
        Self::Github
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct AuthConfig {
    #[serde(default)]
    pub provider: AuthProvider,
    #[serde(default)]
    pub allowed_usernames: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiAuthenticationStrategy {
    Jwt,
    InsecureDisabled,
}

impl Default for ApiAuthenticationStrategy {
    fn default() -> Self {
        Self::Jwt
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub authentication_strategy: ApiAuthenticationStrategy,
}

fn default_base_url() -> String {
    "http://localhost:3000".to_string()
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            authentication_strategy: ApiAuthenticationStrategy::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GitProvider {
    Github,
}

impl Default for GitProvider {
    fn default() -> Self {
        Self::Github
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct GitConfig {
    #[serde(default)]
    pub provider: GitProvider,
    pub app_id: Option<String>,
    pub client_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ServerConfig {
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(flatten)]
    pub run_defaults: RunDefaults,
}

/// Load server config from `~/.arc/arc.toml`, returning defaults if the file doesn't exist.
pub fn load_server_config() -> anyhow::Result<ServerConfig> {
    let Some(home) = dirs::home_dir() else {
        return Ok(ServerConfig::default());
    };
    let path = home.join(".arc").join("arc.toml");
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ServerConfig::default()),
        Err(e) => Err(e.into()),
    }
}

/// Resolve the data directory: config value > default `~/.arc`.
pub fn resolve_data_dir(config: &ServerConfig) -> PathBuf {
    if let Some(ref dir) = config.data_dir {
        return dir.clone();
    }
    dirs::home_dir()
        .map(|h| h.join(".arc"))
        .unwrap_or_else(|| PathBuf::from(".arc"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_with_data_dir() {
        let toml = r#"data_dir = "/custom/path""#;
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.data_dir, Some(PathBuf::from("/custom/path")));
    }

    #[test]
    fn parse_empty_config_defaults() {
        let toml = "";
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.data_dir, None);
    }

    #[test]
    fn resolve_data_dir_uses_config_value() {
        let config = ServerConfig {
            data_dir: Some(PathBuf::from("/my/data")),
            ..ServerConfig::default()
        };
        assert_eq!(resolve_data_dir(&config), PathBuf::from("/my/data"));
    }

    #[test]
    fn resolve_data_dir_defaults_to_home_arc() {
        let config = ServerConfig::default();
        let dir = resolve_data_dir(&config);
        // Should end with .arc
        assert!(
            dir.ends_with(".arc"),
            "expected path ending with .arc, got: {}",
            dir.display()
        );
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
[auth]
provider = "github"
allowed_usernames = ["brynary", "alice"]

[api]
base_url = "http://example.com:8080"
authentication_strategy = "jwt"

[git]
provider = "github"
app_id = "12345"
client_id = "Iv1.abc123"
"#;
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::Github);
        assert_eq!(config.auth.allowed_usernames, vec!["brynary", "alice"]);
        assert_eq!(config.api.base_url, "http://example.com:8080");
        assert_eq!(
            config.api.authentication_strategy,
            ApiAuthenticationStrategy::Jwt
        );
        assert_eq!(config.git.provider, GitProvider::Github);
        assert_eq!(config.git.app_id.as_deref(), Some("12345"));
        assert_eq!(config.git.client_id.as_deref(), Some("Iv1.abc123"));
    }

    #[test]
    fn parse_auth_defaults() {
        let toml = "";
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::Github);
        assert!(config.auth.allowed_usernames.is_empty());
    }

    #[test]
    fn parse_api_defaults() {
        let toml = "";
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.api.base_url, "http://localhost:3000");
        assert_eq!(
            config.api.authentication_strategy,
            ApiAuthenticationStrategy::Jwt
        );
    }

    #[test]
    fn parse_git_config() {
        let toml = r#"
[git]
provider = "github"
app_id = "12345"
client_id = "Iv1.abc123"
"#;
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.git.provider, GitProvider::Github);
        assert_eq!(config.git.app_id.as_deref(), Some("12345"));
        assert_eq!(config.git.client_id.as_deref(), Some("Iv1.abc123"));
    }

    #[test]
    fn parse_git_defaults() {
        let toml = "";
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.git.provider, GitProvider::Github);
        assert_eq!(config.git.app_id, None);
        assert_eq!(config.git.client_id, None);
    }

    #[test]
    fn parse_config_with_run_defaults() {
        let toml = r#"
[llm]
model = "claude-haiku"
provider = "anthropic"

[sandbox]
provider = "daytona"

[vars]
repo_url = "https://github.com/org/repo"
"#;
        let config: ServerConfig = toml::from_str(toml).unwrap();
        let llm = config.run_defaults.llm.unwrap();
        assert_eq!(llm.model.as_deref(), Some("claude-haiku"));
        assert_eq!(llm.provider.as_deref(), Some("anthropic"));
        let sandbox = config.run_defaults.sandbox.unwrap();
        assert_eq!(sandbox.provider.as_deref(), Some("daytona"));
        let vars = config.run_defaults.vars.unwrap();
        assert_eq!(vars["repo_url"], "https://github.com/org/repo");
    }

    #[test]
    fn parse_config_server_and_run_defaults_together() {
        let toml = r#"
[auth]
provider = "github"

[git]
provider = "github"
app_id = "123"

[llm]
model = "gpt-4"
"#;
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::Github);
        assert_eq!(config.git.app_id.as_deref(), Some("123"));
        let llm = config.run_defaults.llm.unwrap();
        assert_eq!(llm.model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn parse_insecure_disabled_values() {
        let toml = r#"
[auth]
provider = "insecure_disabled"

[api]
authentication_strategy = "insecure_disabled"
"#;
        let config: ServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::InsecureDisabled);
        assert_eq!(
            config.api.authentication_strategy,
            ApiAuthenticationStrategy::InsecureDisabled
        );
    }
}
