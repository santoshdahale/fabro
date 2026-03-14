use std::path::Path;

use anyhow::{bail, Context};
use clap::Args;
use fabro_util::terminal::Styles;

use super::project_config::{
    discover_project_config, list_workflows_detailed, resolve_fabro_root, WorkflowInfo,
    WorkflowSource,
};
use super::relative_path;

const GOAL_MAX_LEN: usize = 60;

#[derive(Args)]
pub struct WorkflowListArgs {}

pub fn workflow_list_command(_args: &WorkflowListArgs) -> anyhow::Result<()> {
    let styles = Styles::detect_stderr();
    let cwd = std::env::current_dir()?;

    let (config_path, config) = match discover_project_config(&cwd)? {
        Some(found) => found,
        None => bail!(
            "No fabro.toml found in {cwd} or any parent directory",
            cwd = cwd.display()
        ),
    };

    let fabro_root = resolve_fabro_root(&config_path, &config);
    let project_wf_dir = fabro_root.join("workflows");
    let user_wf_dir = dirs::home_dir().map(|h| h.join(".fabro").join("workflows"));

    let workflows = list_workflows_detailed(Some(&project_wf_dir), user_wf_dir.as_deref());

    let project: Vec<_> = workflows
        .iter()
        .filter(|w| w.source == WorkflowSource::Project)
        .collect();
    let user: Vec<_> = workflows
        .iter()
        .filter(|w| w.source == WorkflowSource::User)
        .collect();

    let name_width = workflows.iter().map(|w| w.name.len()).max().unwrap_or(0);

    eprintln!(
        "{} workflow(s) found\n",
        styles.bold.apply_to(workflows.len())
    );

    let user_path = user_wf_dir
        .as_deref()
        .map(relative_path)
        .unwrap_or_else(|| "~/.fabro/workflows".to_string());
    print_section("User Workflows", &user_path, &user, name_width, &styles);

    eprintln!();

    print_section(
        "Project Workflows",
        &relative_path(&project_wf_dir),
        &project,
        name_width,
        &styles,
    );

    Ok(())
}

#[derive(Args)]
pub struct WorkflowCreateArgs {
    /// Name of the workflow
    pub name: String,

    /// Goal description for the workflow
    #[arg(short, long)]
    goal: Option<String>,
}

pub fn workflow_create_command(args: &WorkflowCreateArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    let (config_path, config) = match discover_project_config(&cwd)? {
        Some(found) => found,
        None => bail!(
            "No fabro.toml found in {cwd} or any parent directory",
            cwd = cwd.display()
        ),
    };

    let fabro_root = resolve_fabro_root(&config_path, &config);
    write_workflow_scaffold(args, &fabro_root)?;

    let workflows_dir = fabro_root.join("workflows").join(&args.name);
    let green = console::Style::new().green();
    let bold = console::Style::new().bold();
    let cyan_bold = console::Style::new().cyan().bold();
    let dim = console::Style::new().dim();

    let rel_dir = relative_path(&workflows_dir);
    eprintln!(
        "  {} {}",
        green.apply_to("✔"),
        dim.apply_to(format!("{rel_dir}/workflow.fabro"))
    );
    eprintln!(
        "  {} {}",
        green.apply_to("✔"),
        dim.apply_to(format!("{rel_dir}/workflow.toml"))
    );

    eprintln!("\n{} Next steps:\n", bold.apply_to("Workflow created!"));
    eprintln!(
        "  1. Edit the graph:  {}",
        cyan_bold.apply_to(format!("{rel_dir}/workflow.fabro"))
    );
    eprintln!(
        "  2. Validate:        {}",
        cyan_bold.apply_to(format!("fabro validate {}", args.name))
    );
    eprintln!(
        "  3. Run:             {}",
        cyan_bold.apply_to(format!("fabro run {}", args.name))
    );

    Ok(())
}

/// Create a workflow in a specific project (for testing).
pub fn workflow_create_in(args: &WorkflowCreateArgs, config_path: &Path) -> anyhow::Result<()> {
    let config = super::project_config::load_project_config(config_path)?;
    let fabro_root = resolve_fabro_root(config_path, &config);
    write_workflow_scaffold(args, &fabro_root)
}

fn write_workflow_scaffold(args: &WorkflowCreateArgs, fabro_root: &Path) -> anyhow::Result<()> {
    let workflows_dir = fabro_root.join("workflows").join(&args.name);

    if workflows_dir.exists() {
        bail!(
            "Workflow '{}' already exists at {}",
            args.name,
            workflows_dir.display()
        );
    }

    std::fs::create_dir_all(&workflows_dir)
        .with_context(|| format!("failed to create {}", workflows_dir.display()))?;

    let goal = args.goal.as_deref().unwrap_or("TODO: describe the goal");
    let digraph_name = to_pascal_case(&args.name);

    let fabro_content = format!(
        r#"digraph {digraph_name} {{
    graph [goal="{goal}"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    main [label="Main", prompt="TODO: describe what this agent should do"]

    start -> main -> exit
}}
"#
    );

    let dot_path = workflows_dir.join("workflow.fabro");
    std::fs::write(&dot_path, &fabro_content)
        .with_context(|| format!("failed to write {}", dot_path.display()))?;

    let toml_path = workflows_dir.join("workflow.toml");
    std::fs::write(&toml_path, "version = 1\n")
        .with_context(|| format!("failed to write {}", toml_path.display()))?;

    Ok(())
}

fn to_pascal_case(s: &str) -> String {
    s.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{upper}{rest}", rest = chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect()
}

fn print_section(
    title: &str,
    path: &str,
    workflows: &[&WorkflowInfo],
    name_width: usize,
    styles: &Styles,
) {
    eprintln!(
        "{} {}",
        styles.bold.apply_to(title),
        styles.dim.apply_to(format!("({path})")),
    );
    if workflows.is_empty() {
        eprintln!("  {}", styles.dim.apply_to("(none)"));
        return;
    }
    eprintln!();
    eprintln!(
        "  {:<name_width$}  {}",
        styles.bold_dim.apply_to("NAME"),
        styles.bold_dim.apply_to("DESCRIPTION"),
    );
    for w in workflows {
        let goal_str = w
            .goal
            .as_deref()
            .map(|g| truncate_str(g, GOAL_MAX_LEN))
            .unwrap_or_default();
        eprintln!(
            "  {:<name_width$}  {}",
            styles.cyan.apply_to(&w.name),
            styles.dim.apply_to(goal_str),
        );
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(tmp: &TempDir) -> std::path::PathBuf {
        let config_path = tmp.path().join("fabro.toml");
        fs::write(&config_path, "version = 1\n\n[fabro]\nroot = \"fabro/\"\n").unwrap();
        config_path
    }

    #[test]
    fn creates_workflow_directory_and_files() {
        let tmp = TempDir::new().unwrap();
        let config_path = setup_project(&tmp);

        let args = WorkflowCreateArgs {
            name: "deploy".to_string(),
            goal: None,
        };
        workflow_create_in(&args, &config_path).unwrap();

        let wf_dir = tmp.path().join("fabro/workflows/deploy");
        assert!(wf_dir.join("workflow.fabro").exists());
        assert!(wf_dir.join("workflow.toml").exists());
    }

    #[test]
    fn goal_appears_in_generated_fabro() {
        let tmp = TempDir::new().unwrap();
        let config_path = setup_project(&tmp);

        let args = WorkflowCreateArgs {
            name: "deploy".to_string(),
            goal: Some("Deploy the app".to_string()),
        };
        workflow_create_in(&args, &config_path).unwrap();

        let content =
            fs::read_to_string(tmp.path().join("fabro/workflows/deploy/workflow.fabro")).unwrap();
        assert!(content.contains(r#"goal="Deploy the app""#));
    }

    #[test]
    fn default_goal_is_todo_placeholder() {
        let tmp = TempDir::new().unwrap();
        let config_path = setup_project(&tmp);

        let args = WorkflowCreateArgs {
            name: "deploy".to_string(),
            goal: None,
        };
        workflow_create_in(&args, &config_path).unwrap();

        let content =
            fs::read_to_string(tmp.path().join("fabro/workflows/deploy/workflow.fabro")).unwrap();
        assert!(content.contains(r#"goal="TODO:"#));
    }

    #[test]
    fn fails_if_workflow_already_exists() {
        let tmp = TempDir::new().unwrap();
        let config_path = setup_project(&tmp);

        let wf_dir = tmp.path().join("fabro/workflows/deploy");
        fs::create_dir_all(&wf_dir).unwrap();

        let args = WorkflowCreateArgs {
            name: "deploy".to_string(),
            goal: None,
        };
        let err = workflow_create_in(&args, &config_path).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "expected 'already exists' in: {err}"
        );
    }

    #[test]
    fn fails_if_no_fabro_toml() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("fabro.toml");

        let args = WorkflowCreateArgs {
            name: "deploy".to_string(),
            goal: None,
        };
        let result = workflow_create_in(&args, &config_path);
        assert!(result.is_err());
    }

    #[test]
    fn digraph_name_derived_from_workflow_name() {
        let tmp = TempDir::new().unwrap();
        let config_path = setup_project(&tmp);

        let args = WorkflowCreateArgs {
            name: "my-workflow".to_string(),
            goal: None,
        };
        workflow_create_in(&args, &config_path).unwrap();

        let content = fs::read_to_string(
            tmp.path()
                .join("fabro/workflows/my-workflow/workflow.fabro"),
        )
        .unwrap();
        assert!(
            content.contains("digraph MyWorkflow"),
            "expected 'digraph MyWorkflow' in:\n{content}"
        );
    }

    #[test]
    fn generated_fabro_parses_and_validates() {
        let tmp = TempDir::new().unwrap();
        let config_path = setup_project(&tmp);

        let args = WorkflowCreateArgs {
            name: "test-wf".to_string(),
            goal: Some("Test goal".to_string()),
        };
        workflow_create_in(&args, &config_path).unwrap();

        let content =
            fs::read_to_string(tmp.path().join("fabro/workflows/test-wf/workflow.fabro")).unwrap();

        let graph = crate::parser::parse(&content).expect("generated .fabro should parse");
        let diagnostics = crate::validation::validate(&graph, &[]);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == crate::validation::Severity::Error)
            .collect();
        assert!(errors.is_empty(), "validation errors: {errors:?}");
    }

    #[test]
    fn to_pascal_case_simple() {
        assert_eq!(to_pascal_case("hello"), "Hello");
    }

    #[test]
    fn to_pascal_case_hyphenated() {
        assert_eq!(to_pascal_case("my-workflow"), "MyWorkflow");
    }

    #[test]
    fn to_pascal_case_underscored() {
        assert_eq!(to_pascal_case("my_workflow"), "MyWorkflow");
    }

    #[test]
    fn to_pascal_case_mixed() {
        assert_eq!(to_pascal_case("my-cool_workflow"), "MyCoolWorkflow");
    }

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hello", 60), "hello");
    }

    #[test]
    fn truncate_str_exact_limit() {
        let s = "a".repeat(60);
        assert_eq!(truncate_str(&s, 60), s);
    }

    #[test]
    fn truncate_str_over_limit() {
        let s = "a".repeat(70);
        let result = truncate_str(&s, 60);
        assert_eq!(result.len(), 60);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn truncate_str_multiline_uses_first_line() {
        assert_eq!(truncate_str("first\nsecond\nthird", 60), "first");
    }
}
