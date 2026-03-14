use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

use super::run_config::PullRequestConfig;

const CONFIG_FILENAME: &str = "fabro.toml";

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub version: u32,
    #[serde(default)]
    pub fabro: ProjectFabroConfig,
    pub pull_request: Option<PullRequestConfig>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectFabroConfig {
    #[serde(default = "default_root")]
    pub root: String,
    #[serde(default = "default_retro")]
    pub retro: bool,
}

fn default_root() -> String {
    ".".to_string()
}

fn default_retro() -> bool {
    true
}

impl Default for ProjectFabroConfig {
    fn default() -> Self {
        Self {
            root: default_root(),
            retro: default_retro(),
        }
    }
}

/// Parse a project config from a TOML string.
pub fn parse_project_config(content: &str) -> anyhow::Result<ProjectConfig> {
    let config: ProjectConfig =
        toml::from_str(content).context("Failed to parse project config")?;
    if config.version != 1 {
        bail!(
            "Unsupported project config version: {}. Only version 1 is supported.",
            config.version,
        );
    }
    Ok(config)
}

/// Load a project config from a file path.
pub fn load_project_config(path: &Path) -> anyhow::Result<ProjectConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config = parse_project_config(&content)?;
    tracing::debug!(path = %path.display(), root = %config.fabro.root, "Loaded project config");
    Ok(config)
}

/// Walk ancestor directories from `start` looking for `fabro.toml`.
/// Returns the config file path and parsed config, or `None` if not found.
pub fn discover_project_config(start: &Path) -> anyhow::Result<Option<(PathBuf, ProjectConfig)>> {
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(CONFIG_FILENAME);
        if candidate.is_file() {
            tracing::debug!(path = %candidate.display(), "Discovered project config");
            let config = load_project_config(&candidate)?;
            return Ok(Some((candidate, config)));
        }
    }
    Ok(None)
}

/// Resolve a workflow argument to a path.
///
/// - If the arg has a file extension (`.toml`, `.fabro`, etc.), return it as-is.
/// - If no extension, attempt project-based resolution: find `fabro.toml`, resolve
///   `{fabro_root}/workflows/{name}/workflow.toml`. Returns an error with suggestions
///   if an `fabro.toml` exists but the workflow wasn't found.
pub fn resolve_workflow_arg(arg: &Path) -> anyhow::Result<PathBuf> {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_workflow_arg_from(arg, &start)
}

fn resolve_workflow_arg_from(arg: &Path, start_dir: &Path) -> anyhow::Result<PathBuf> {
    resolve_workflow_arg_impl(arg, start_dir, user_workflows_dir().as_deref())
}

fn resolve_workflow_arg_impl(
    arg: &Path,
    start_dir: &Path,
    user_workflows: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if arg.extension().is_some() {
        tracing::debug!(arg = %arg.display(), "Workflow arg has extension, returning as-is");
        return Ok(arg.to_path_buf());
    }

    let name = arg.to_string_lossy();
    match discover_project_config(start_dir) {
        Ok(Some((config_path, config))) => {
            let fabro_root = resolve_fabro_root(&config_path, &config);
            let project_candidate = fabro_root
                .join("workflows")
                .join(&*name)
                .join("workflow.toml");
            if project_candidate.is_file() {
                tracing::debug!(arg = %arg.display(), resolved = %project_candidate.display(), "Resolved workflow name via project config");
                return Ok(project_candidate);
            }

            if let Some(user_wf) = user_workflows {
                let user_candidate = user_wf.join(&*name).join("workflow.toml");
                if user_candidate.is_file() {
                    tracing::debug!(arg = %arg.display(), resolved = %user_candidate.display(), "Resolved workflow name via user workflows");
                    return Ok(user_candidate);
                }
            }

            let project_wf_dir = fabro_root.join("workflows");
            let available = list_available_workflows(Some(&project_wf_dir), user_workflows);
            if available.is_empty() {
                bail!(
                    "Unknown workflow '{name}'\n\nNo workflows found in {}",
                    project_wf_dir.display()
                );
            }
            let mut msg = format!(
                "Unknown workflow '{name}'\n\nAvailable workflows: {}",
                available.join(", ")
            );
            if let Some(suggestion) = find_closest_match(&name, &available) {
                msg.push_str(&format!("\n\nDid you mean '{suggestion}'?"));
            }
            bail!("{msg}");
        }
        Ok(None) => {
            if let Some(user_wf) = user_workflows {
                let user_candidate = user_wf.join(&*name).join("workflow.toml");
                if user_candidate.is_file() {
                    tracing::debug!(arg = %arg.display(), resolved = %user_candidate.display(), "Resolved workflow name via user workflows (no project config)");
                    return Ok(user_candidate);
                }
            }
            tracing::debug!(arg = %arg.display(), "No project config found, returning literal");
            Ok(arg.to_path_buf())
        }
        Err(err) => {
            tracing::debug!(arg = %arg.display(), error = %err, "Error discovering project config, returning literal");
            Ok(arg.to_path_buf())
        }
    }
}

/// Return the user-level workflows directory (`~/.fabro/workflows/`).
fn user_workflows_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".fabro").join("workflows"))
}

/// List workflow names in a single directory by scanning for subdirs containing `workflow.toml`.
fn list_workflows_in(workflows_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(workflows_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() && path.join("workflow.toml").is_file() {
                entry.file_name().to_str().map(String::from)
            } else {
                None
            }
        })
        .collect()
}

/// List workflow names by scanning project and user workflow directories.
/// Project workflows appear first; user workflows are deduplicated.
fn list_available_workflows(
    project_workflows_dir: Option<&Path>,
    user_workflows_dir: Option<&Path>,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    if let Some(dir) = project_workflows_dir {
        names.extend(list_workflows_in(dir));
    }
    if let Some(dir) = user_workflows_dir {
        for name in list_workflows_in(dir) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }

    names.sort();
    names
}

/// Find the closest match using normalized Levenshtein distance (threshold: 0.5).
fn find_closest_match(input: &str, candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .map(|c| (c, strsim::normalized_levenshtein(input, c)))
        .filter(|(_, score)| *score >= 0.5)
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(name, _)| name.clone())
}

/// Resolve a workflow argument to a DOT path and optional run config.
///
/// Calls `resolve_workflow_arg` first, then if the result is a `.toml` file,
/// loads the run config and resolves the graph path within it.
pub fn resolve_workflow(
    arg: &Path,
) -> anyhow::Result<(PathBuf, Option<super::run_config::WorkflowRunConfig>)> {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_workflow_from(arg, &start)
}

fn resolve_workflow_from(
    arg: &Path,
    start_dir: &Path,
) -> anyhow::Result<(PathBuf, Option<super::run_config::WorkflowRunConfig>)> {
    let path = resolve_workflow_arg_from(arg, start_dir)?;
    if path.extension().is_some_and(|ext| ext == "toml") {
        let cfg = super::run_config::load_run_config(&path)?;
        let dot = super::run_config::resolve_graph_path(&path, &cfg.graph);
        Ok((dot, Some(cfg)))
    } else {
        Ok((path, None))
    }
}

/// Check whether retros are enabled in the project config.
/// Returns `true` (the default) if no config is found or on error.
pub fn is_retro_enabled() -> bool {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match discover_project_config(&start) {
        Ok(Some((_path, config))) => config.fabro.retro,
        _ => true,
    }
}

/// Resolve the fabro root directory from a config file path and its config.
/// The returned path is the directory containing `fabro.toml` joined with the `root` value.
pub fn resolve_fabro_root(config_path: &Path, config: &ProjectConfig) -> PathBuf {
    let project_dir = config_path
        .parent()
        .expect("config_path should have a parent directory");
    project_dir.join(&config.fabro.root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parse_minimal_config() {
        let config = parse_project_config("version = 1\n").unwrap();
        assert_eq!(
            config,
            ProjectConfig {
                version: 1,
                fabro: ProjectFabroConfig {
                    root: ".".to_string(),
                    retro: true,
                },
                pull_request: None,
            }
        );
    }

    #[test]
    fn parse_full_config() {
        let config = parse_project_config("version = 1\n[fabro]\nroot = \"fabro/\"\n").unwrap();
        assert_eq!(config.fabro.root, "fabro/");
    }

    #[test]
    fn parse_retro_false() {
        let config = parse_project_config("version = 1\n[fabro]\nretro = false\n").unwrap();
        assert!(!config.fabro.retro);
    }

    #[test]
    fn parse_version_mismatch() {
        let err = parse_project_config("version = 2\n").unwrap_err();
        assert!(
            err.to_string().contains("Unsupported"),
            "Expected 'Unsupported' in error, got: {err}"
        );
    }

    #[test]
    fn parse_pull_request_config() {
        let config =
            parse_project_config("version = 1\n\n[pull_request]\nenabled = true\ndraft = false\n")
                .unwrap();
        assert_eq!(
            config.pull_request,
            Some(PullRequestConfig {
                enabled: true,
                draft: false,
            })
        );
    }

    #[test]
    fn parse_unknown_field_rejected() {
        let err = parse_project_config("version = 1\nfoo = \"bar\"\n").unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("unknown field"), "got: {chain}");
    }

    #[test]
    fn load_from_disk() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("fabro.toml");
        fs::write(&path, "version = 1\n").unwrap();
        let config = load_project_config(&path).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.fabro.root, ".");
    }

    #[test]
    fn discover_walks_ancestors() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("fabro.toml"), "version = 1\n").unwrap();
        let sub = tmp.path().join("sub").join("dir");
        fs::create_dir_all(&sub).unwrap();

        let (found_path, config) = discover_project_config(&sub).unwrap().unwrap();
        assert_eq!(found_path, tmp.path().join("fabro.toml"));
        assert_eq!(config.version, 1);
    }

    #[test]
    fn discover_returns_none_when_absent() {
        let tmp = TempDir::new().unwrap();
        let result = discover_project_config(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn resolve_fabro_root_with_subdirectory() {
        let config_path = Path::new("/repo/fabro.toml");
        let config = ProjectConfig {
            version: 1,
            fabro: ProjectFabroConfig {
                root: "fabro/".to_string(),
                ..Default::default()
            },
            pull_request: None,
        };
        assert_eq!(
            resolve_fabro_root(config_path, &config),
            Path::new("/repo/fabro/")
        );
    }

    #[test]
    fn resolve_fabro_root_with_dot() {
        let config_path = Path::new("/repo/fabro.toml");
        let config = ProjectConfig {
            version: 1,
            fabro: ProjectFabroConfig {
                root: ".".to_string(),
                ..Default::default()
            },
            pull_request: None,
        };
        assert_eq!(
            resolve_fabro_root(config_path, &config),
            Path::new("/repo/.")
        );
    }

    #[test]
    fn resolve_workflow_arg_toml_extension_returned_as_is() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_workflow_arg_from(Path::new("my-workflow.toml"), tmp.path()).unwrap();
        assert_eq!(result, Path::new("my-workflow.toml"));
    }

    #[test]
    fn resolve_workflow_arg_fabro_extension_returned_as_is() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_workflow_arg_from(Path::new("my-workflow.fabro"), tmp.path()).unwrap();
        assert_eq!(result, Path::new("my-workflow.fabro"));
    }

    #[test]
    fn resolve_workflow_arg_no_extension_no_config_returns_literal() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_workflow_arg_from(Path::new("my-workflow"), tmp.path()).unwrap();
        assert_eq!(result, Path::new("my-workflow"));
    }

    #[test]
    fn resolve_workflow_arg_no_extension_with_config_and_workflow_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("fabro.toml"), "version = 1\n").unwrap();
        let wf_dir = tmp.path().join("workflows").join("my-workflow");
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(
            wf_dir.join("workflow.toml"),
            "version = 1\ngraph = \"workflow.fabro\"\n",
        )
        .unwrap();

        let result = resolve_workflow_arg_from(Path::new("my-workflow"), tmp.path()).unwrap();
        assert_eq!(result, wf_dir.join("workflow.toml"));
    }

    #[test]
    fn resolve_workflow_arg_typo_suggests_similar_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("fabro.toml"), "version = 1\n").unwrap();
        let wf_dir = tmp.path().join("workflows").join("implement");
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(
            wf_dir.join("workflow.toml"),
            "version = 1\ngraph = \"w.fabro\"\n",
        )
        .unwrap();

        let err = resolve_workflow_arg_from(Path::new("implemet"), tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown workflow 'implemet'"), "got: {msg}");
        assert!(msg.contains("Did you mean 'implement'?"), "got: {msg}");
    }

    #[test]
    fn resolve_workflow_arg_unknown_lists_available() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("fabro.toml"), "version = 1\n").unwrap();
        let wf_dir = tmp.path().join("workflows").join("hello");
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(
            wf_dir.join("workflow.toml"),
            "version = 1\ngraph = \"w.fabro\"\n",
        )
        .unwrap();

        let err = resolve_workflow_arg_from(Path::new("zzzzz"), tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown workflow 'zzzzz'"), "got: {msg}");
        assert!(msg.contains("Available workflows: hello"), "got: {msg}");
        assert!(!msg.contains("Did you mean"), "got: {msg}");
    }

    #[test]
    fn resolve_workflow_arg_no_workflows_dir() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("fabro.toml"), "version = 1\n").unwrap();

        let err = resolve_workflow_arg_from(Path::new("my-workflow"), tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No workflows found"), "got: {msg}");
    }

    #[test]
    fn resolve_workflow_arg_custom_root_respected() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("fabro.toml"),
            "version = 1\n[fabro]\nroot = \"fabro/\"\n",
        )
        .unwrap();
        let wf_dir = tmp.path().join("fabro").join("workflows").join("factory");
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(
            wf_dir.join("workflow.toml"),
            "version = 1\ngraph = \"workflow.fabro\"\n",
        )
        .unwrap();

        let result = resolve_workflow_arg_from(Path::new("factory"), tmp.path()).unwrap();
        assert_eq!(result, wf_dir.join("workflow.toml"));
    }

    /// Helper: create a temp dir with fabro.toml + workflows/{name}/{workflow.toml, workflow.fabro}
    /// and chdir into it so `resolve_workflow` (which uses cwd) can find the config.
    fn setup_workflow_project(name: &str) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("fabro.toml"), "version = 1\n").unwrap();
        let wf_dir = tmp.path().join("workflows").join(name);
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(
            wf_dir.join("workflow.toml"),
            "version = 1\ngraph = \"workflow.fabro\"\n",
        )
        .unwrap();
        fs::write(
            wf_dir.join("workflow.fabro"),
            "digraph G { start [shape=Mdiamond]; exit [shape=Msquare]; start -> exit }",
        )
        .unwrap();
        let dot_path = wf_dir.join("workflow.fabro");
        (tmp, dot_path)
    }

    #[test]
    fn resolve_workflow_bare_name() {
        let (tmp, expected_dot) = setup_workflow_project("hello");
        let (dot_path, cfg) = resolve_workflow_from(Path::new("hello"), tmp.path()).unwrap();
        assert_eq!(
            dot_path.canonicalize().unwrap(),
            expected_dot.canonicalize().unwrap()
        );
        assert!(cfg.is_some(), "expected Some(RunConfig) for bare name");
    }

    #[test]
    fn resolve_workflow_toml_path() {
        let (tmp, expected_dot) = setup_workflow_project("hello");
        let toml_path = tmp.path().join("workflows/hello/workflow.toml");
        let (dot_path, cfg) = resolve_workflow_from(&toml_path, tmp.path()).unwrap();
        assert_eq!(dot_path, expected_dot);
        assert!(cfg.is_some(), "expected Some(RunConfig) for .toml path");
    }

    #[test]
    fn resolve_workflow_fabro_path() {
        let (tmp, expected_dot) = setup_workflow_project("hello");
        let (dot_path, cfg) = resolve_workflow_from(&expected_dot, tmp.path()).unwrap();
        assert_eq!(dot_path, expected_dot);
        assert!(cfg.is_none(), "expected None for .fabro path");
    }

    /// Helper: create a workflow dir with workflow.toml + workflow.fabro inside `base/workflows/{name}/`
    fn create_workflow_in(base: &Path, name: &str) {
        let wf_dir = base.join("workflows").join(name);
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(
            wf_dir.join("workflow.toml"),
            "version = 1\ngraph = \"workflow.fabro\"\n",
        )
        .unwrap();
        fs::write(
            wf_dir.join("workflow.fabro"),
            "digraph G { start [shape=Mdiamond]; exit [shape=Msquare]; start -> exit }",
        )
        .unwrap();
    }

    #[test]
    fn resolve_workflow_arg_user_workflow_found() {
        let project_dir = TempDir::new().unwrap();
        // No fabro.toml in project_dir
        let user_dir = TempDir::new().unwrap();
        create_workflow_in(user_dir.path(), "my-wf");

        let result = resolve_workflow_arg_impl(
            Path::new("my-wf"),
            project_dir.path(),
            Some(user_dir.path().join("workflows").as_path()),
        )
        .unwrap();
        assert_eq!(
            result,
            user_dir.path().join("workflows/my-wf/workflow.toml")
        );
    }

    #[test]
    fn resolve_workflow_arg_project_takes_precedence() {
        let project_dir = TempDir::new().unwrap();
        fs::write(project_dir.path().join("fabro.toml"), "version = 1\n").unwrap();
        create_workflow_in(project_dir.path(), "shared");

        let user_dir = TempDir::new().unwrap();
        create_workflow_in(user_dir.path(), "shared");

        let result = resolve_workflow_arg_impl(
            Path::new("shared"),
            project_dir.path(),
            Some(user_dir.path().join("workflows").as_path()),
        )
        .unwrap();
        // Should resolve to project, not user
        assert_eq!(
            result,
            project_dir.path().join("workflows/shared/workflow.toml")
        );
    }

    #[test]
    fn resolve_workflow_arg_user_fallback_when_project_missing() {
        let project_dir = TempDir::new().unwrap();
        fs::write(project_dir.path().join("fabro.toml"), "version = 1\n").unwrap();
        // Project has a different workflow
        create_workflow_in(project_dir.path(), "other");

        let user_dir = TempDir::new().unwrap();
        create_workflow_in(user_dir.path(), "my-wf");

        let result = resolve_workflow_arg_impl(
            Path::new("my-wf"),
            project_dir.path(),
            Some(user_dir.path().join("workflows").as_path()),
        )
        .unwrap();
        assert_eq!(
            result,
            user_dir.path().join("workflows/my-wf/workflow.toml")
        );
    }

    #[test]
    fn resolve_workflow_arg_user_workflow_listed_in_error() {
        let project_dir = TempDir::new().unwrap();
        fs::write(project_dir.path().join("fabro.toml"), "version = 1\n").unwrap();
        create_workflow_in(project_dir.path(), "proj-wf");

        let user_dir = TempDir::new().unwrap();
        create_workflow_in(user_dir.path(), "user-wf");

        let err = resolve_workflow_arg_impl(
            Path::new("nonexistent"),
            project_dir.path(),
            Some(user_dir.path().join("workflows").as_path()),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("proj-wf"), "expected proj-wf in: {msg}");
        assert!(msg.contains("user-wf"), "expected user-wf in: {msg}");
    }
}
