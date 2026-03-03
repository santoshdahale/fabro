use std::path::PathBuf;

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

#[derive(Debug, Default, Deserialize)]
pub struct AppConfig {
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub api: ApiConfig,
}

/// Load app config from `~/.arc/arc.toml`, returning defaults if the file doesn't exist.
pub fn load_app_config() -> anyhow::Result<AppConfig> {
    let Some(home) = dirs::home_dir() else {
        return Ok(AppConfig::default());
    };
    let path = home.join(".arc").join("arc.toml");
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    let config: AppConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// Resolve the data directory: config value > default `~/.arc`.
pub fn resolve_data_dir(config: &AppConfig) -> PathBuf {
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
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.data_dir, Some(PathBuf::from("/custom/path")));
    }

    #[test]
    fn parse_empty_config_defaults() {
        let toml = "";
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.data_dir, None);
    }

    #[test]
    fn resolve_data_dir_uses_config_value() {
        let config = AppConfig {
            data_dir: Some(PathBuf::from("/my/data")),
            ..AppConfig::default()
        };
        assert_eq!(resolve_data_dir(&config), PathBuf::from("/my/data"));
    }

    #[test]
    fn resolve_data_dir_defaults_to_home_arc() {
        let config = AppConfig::default();
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
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::Github);
        assert_eq!(config.auth.allowed_usernames, vec!["brynary", "alice"]);
        assert_eq!(config.api.base_url, "http://example.com:8080");
        assert_eq!(
            config.api.authentication_strategy,
            ApiAuthenticationStrategy::Jwt
        );
    }

    #[test]
    fn parse_auth_defaults() {
        let toml = "";
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::Github);
        assert!(config.auth.allowed_usernames.is_empty());
    }

    #[test]
    fn parse_api_defaults() {
        let toml = "";
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.api.base_url, "http://localhost:3000");
        assert_eq!(
            config.api.authentication_strategy,
            ApiAuthenticationStrategy::Jwt
        );
    }

    #[test]
    fn parse_insecure_disabled_values() {
        let toml = r#"
[auth]
provider = "insecure_disabled"

[api]
authentication_strategy = "insecure_disabled"
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.auth.provider, AuthProvider::InsecureDisabled);
        assert_eq!(
            config.api.authentication_strategy,
            ApiAuthenticationStrategy::InsecureDisabled
        );
    }
}
