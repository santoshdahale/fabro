use std::path::Path;

use arc_agent::cli::{OutputFormat, PermissionLevel};
use serde::Deserialize;
use tracing::debug;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct AgentDefaults {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub permissions: Option<PermissionLevel>,
    pub output_format: Option<OutputFormat>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct LlmDefaults {
    pub model: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CliConfig {
    pub agent: Option<AgentDefaults>,
    pub llm: Option<LlmDefaults>,
}

/// Load CLI config from an explicit path or `~/.arc/cli.toml`, returning defaults if the
/// default file doesn't exist. An explicit path that doesn't exist is an error.
pub fn load_cli_config(path: Option<&Path>) -> anyhow::Result<CliConfig> {
    if let Some(explicit) = path {
        debug!(path = %explicit.display(), "Loading CLI config from explicit path");
        let contents = std::fs::read_to_string(explicit)?;
        return Ok(toml::from_str(&contents)?);
    }

    let Some(home) = dirs::home_dir() else {
        debug!("No home directory found, using default CLI config");
        return Ok(CliConfig::default());
    };
    let default_path = home.join(".arc").join("cli.toml");
    debug!(path = %default_path.display(), "Loading CLI config");
    match std::fs::read_to_string(&default_path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CliConfig::default()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config_defaults() {
        let config: CliConfig = toml::from_str("").unwrap();
        assert_eq!(config, CliConfig::default());
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
[agent]
provider = "anthropic"
model = "claude-opus-4-6"
permissions = "read-write"
output_format = "text"

[llm]
model = "claude-sonnet-4-5"
"#;
        let config: CliConfig = toml::from_str(toml).unwrap();
        let agent = config.agent.unwrap();
        assert_eq!(agent.provider.as_deref(), Some("anthropic"));
        assert_eq!(agent.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(agent.permissions, Some(PermissionLevel::ReadWrite));
        assert_eq!(agent.output_format, Some(OutputFormat::Text));
        let llm = config.llm.unwrap();
        assert_eq!(llm.model.as_deref(), Some("claude-sonnet-4-5"));
    }

    #[test]
    fn parse_partial_agent_config() {
        let toml = r#"
[agent]
provider = "openai"
"#;
        let config: CliConfig = toml::from_str(toml).unwrap();
        let agent = config.agent.unwrap();
        assert_eq!(agent.provider.as_deref(), Some("openai"));
        assert_eq!(agent.model, None);
        assert_eq!(agent.permissions, None);
        assert_eq!(agent.output_format, None);
        assert_eq!(config.llm, None);
    }

    #[test]
    fn load_cli_config_from_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.toml");
        std::fs::write(
            &path,
            r#"
[agent]
provider = "gemini"
model = "gemini-pro"
"#,
        )
        .unwrap();
        let config = load_cli_config(Some(&path)).unwrap();
        let agent = config.agent.unwrap();
        assert_eq!(agent.provider.as_deref(), Some("gemini"));
        assert_eq!(agent.model.as_deref(), Some("gemini-pro"));
    }

    #[test]
    fn load_cli_config_explicit_path_missing_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let result = load_cli_config(Some(&path));
        assert!(result.is_err());
    }
}
