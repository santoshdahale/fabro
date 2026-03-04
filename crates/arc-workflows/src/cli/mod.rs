pub mod backend;
pub mod cli_backend;
pub mod run;
pub mod runs;
pub mod run_config;
pub mod validate;

use std::path::Path;

use arc_util::terminal::Styles;
use clap::{Args, Parser, Subcommand, ValueEnum};
use indicatif::{HumanBytes, HumanCount};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::event::WorkflowRunEvent;
use crate::outcome::StageUsage;
use crate::validation::{Diagnostic, Severity};
use arc_agent::AgentEvent;

/// Sandbox provider for agent tool operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum SandboxProvider {
    /// Run tools on the local host (default)
    #[default]
    Local,
    /// Run tools inside a Docker container
    Docker,
    /// Run tools inside a Daytona cloud sandbox
    Daytona,
}

impl fmt::Display for SandboxProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Docker => write!(f, "docker"),
            Self::Daytona => write!(f, "daytona"),
        }
    }
}

impl FromStr for SandboxProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "docker" => Ok(Self::Docker),
            "daytona" => Ok(Self::Daytona),
            other => Err(format!("unknown sandbox provider: {other}")),
        }
    }
}

#[derive(Parser)]
#[command(
    name = "arc-workflows",
    version,
    about = "DOT-based workflow runner for AI workflows"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Launch a workflow from a .dot or .toml task file
    Run(RunArgs),
    /// Parse and validate a workflow without executing
    Validate(ValidateArgs),
}

#[derive(Args)]
pub struct RunArgs {
    /// Path to a .dot workflow file or .toml task config (not required with --run-branch)
    #[arg(required_unless_present = "run_branch")]
    pub workflow: Option<PathBuf>,

    /// Log/artifact directory
    #[arg(long)]
    pub logs_dir: Option<PathBuf>,

    /// Execute with simulated LLM backend
    #[arg(long)]
    pub dry_run: bool,

    /// Validate run configuration without executing
    #[arg(long, conflicts_with_all = ["resume", "run_branch", "dry_run"])]
    pub preflight: bool,

    /// Auto-approve all human gates
    #[arg(long)]
    pub auto_approve: bool,

    /// Resume from a checkpoint file
    #[arg(long)]
    pub resume: Option<PathBuf>,

    /// Resume from a git run branch (reads checkpoint and graph from metadata branch)
    #[arg(long, conflicts_with = "resume")]
    pub run_branch: Option<String>,

    /// Override default LLM model
    #[arg(long)]
    pub model: Option<String>,

    /// Override default LLM provider
    #[arg(long)]
    pub provider: Option<String>,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Sandbox for agent tools
    #[arg(long, value_enum)]
    pub sandbox: Option<SandboxProvider>,

    /// Attach a label to this run (repeatable, format: KEY=VALUE)
    #[arg(long = "label", value_name = "KEY=VALUE")]
    pub label: Vec<String>,
}

#[derive(Args)]
pub struct ValidateArgs {
    /// Path to the .dot workflow file
    pub workflow: PathBuf,
}

/// Read a .dot file from disk.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn read_dot_file(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))
}

/// Print diagnostics to stderr, colored by severity.
pub fn print_diagnostics(diagnostics: &[Diagnostic], styles: &Styles) {
    for d in diagnostics {
        let location = match (&d.node_id, &d.edge) {
            (Some(node), _) => format!(" [node: {node}]"),
            (_, Some((from, to))) => format!(" [edge: {from} -> {to}]"),
            _ => String::new(),
        };
        match d.severity {
            Severity::Error => eprintln!(
                "{}{location}: {} ({})",
                styles.red.apply_to("error"),
                d.message,
                styles.dim.apply_to(&d.rule),
            ),
            Severity::Warning => eprintln!(
                "{}{location}: {} ({})",
                styles.yellow.apply_to("warning"),
                d.message,
                styles.dim.apply_to(&d.rule),
            ),
            Severity::Info => eprintln!(
                "{}",
                styles.dim.apply_to(format!("info{location}: {} ({})", d.message, d.rule)),
            ),
        }
    }
}

/// One-line summary of a workflow run event for `-v` output (dimmed).
#[must_use]
pub fn format_event_summary(event: &WorkflowRunEvent, styles: &Styles) -> String {
    let body = match event {
        WorkflowRunEvent::WorkflowRunStarted { name, run_id, .. } => {
            format!("[WORKFLOW_RUN_STARTED] name={name} id={run_id}")
        }
        WorkflowRunEvent::WorkflowRunCompleted {
            duration_ms,
            artifact_count,
            total_cost,
            ..
        } => {
            let mut s =
                format!("[WORKFLOW_RUN_COMPLETED] duration={duration_ms}ms artifacts={artifact_count}");
            if let Some(cost) = total_cost {
                s.push_str(&format!(" total_cost={}", format_cost(*cost)));
            }
            s
        }
        WorkflowRunEvent::WorkflowRunFailed {
            error, duration_ms, ..
        } => {
            format!("[WORKFLOW_RUN_FAILED] error=\"{error}\" duration={duration_ms}ms")
        }
        WorkflowRunEvent::StageStarted {
            node_id,
            name,
            index,
            handler_type,
            attempt,
            max_attempts,
        } => {
            let mut s = format!("[STAGE_STARTED] node_id={node_id} name={name} index={index}");
            if let Some(ht) = handler_type {
                s.push_str(&format!(" handler_type={ht}"));
            }
            s.push_str(&format!(" attempt={attempt}/{max_attempts}"));
            s
        }
        WorkflowRunEvent::StageCompleted {
            node_id,
            name,
            index,
            duration_ms,
            status,
            preferred_label,
            suggested_next_ids,
            usage,
            failure,
            notes,
            files_touched,
            attempt,
            max_attempts,
        } => {
            let mut s = format!("[STAGE_COMPLETED] node_id={node_id} name={name} index={index} duration={duration_ms}ms status={status}");
            if let Some(label) = preferred_label {
                s.push_str(&format!(" preferred_label=\"{label}\""));
            }
            if !suggested_next_ids.is_empty() {
                s.push_str(&format!(
                    " suggested_next_ids={}",
                    suggested_next_ids.join(",")
                ));
            }
            if let Some(u) = usage {
                let total = (u.input_tokens + u.output_tokens) as u64;
                if let Some(cost) = compute_stage_cost(u) {
                    s.push_str(&format!(" tokens={} cost={}", HumanCount(total), format_cost(cost)));
                } else {
                    s.push_str(&format!(" tokens={}", HumanCount(total)));
                }
            }
            if let Some(ref f) = failure {
                s.push_str(&format!(" failure_reason=\"{}\"", f.message));
                s.push_str(&format!(" failure_class={}", f.failure_class));
            }
            if let Some(n) = notes {
                s.push_str(&format!(" notes=\"{n}\""));
            }
            if !files_touched.is_empty() {
                s.push_str(&format!(" files_touched={}", files_touched.len()));
            }
            s.push_str(&format!(" attempt={attempt}/{max_attempts}"));
            s
        }
        WorkflowRunEvent::StageFailed {
            node_id,
            name,
            index,
            failure,
            will_retry,
        } => {
            format!(
                "[STAGE_FAILED] node_id={node_id} name={name} index={index} error=\"{}\" will_retry={will_retry} failure_class={}",
                failure.message, failure.failure_class
            )
        }
        WorkflowRunEvent::StageRetrying {
            node_id,
            name,
            index,
            attempt,
            max_attempts,
            delay_ms,
        } => {
            format!(
                "[STAGE_RETRYING] node_id={node_id} name={name} index={index} attempt={attempt}/{max_attempts} delay={delay_ms}ms"
            )
        }
        WorkflowRunEvent::ParallelStarted {
            branch_count,
            join_policy,
            error_policy,
        } => {
            format!("[PARALLEL_STARTED] branches={branch_count} join_policy={join_policy} error_policy={error_policy}")
        }
        WorkflowRunEvent::ParallelBranchStarted { branch, index } => {
            format!("[PARALLEL_BRANCH_STARTED] branch={branch} index={index}")
        }
        WorkflowRunEvent::ParallelBranchCompleted {
            branch,
            index,
            duration_ms,
            status,
        } => {
            format!("[PARALLEL_BRANCH_COMPLETED] branch={branch} index={index} duration={duration_ms}ms status={status}")
        }
        WorkflowRunEvent::ParallelCompleted {
            duration_ms,
            success_count,
            failure_count,
        } => {
            format!("[PARALLEL_COMPLETED] duration={duration_ms}ms succeeded={success_count} failed={failure_count}")
        }
        WorkflowRunEvent::InterviewStarted {
            question,
            stage,
            question_type,
        } => {
            format!("[INTERVIEW_STARTED] stage={stage} question=\"{question}\" question_type={question_type}")
        }
        WorkflowRunEvent::InterviewCompleted {
            question,
            answer,
            duration_ms,
        } => {
            format!(
                "[INTERVIEW_COMPLETED] question=\"{question}\" answer=\"{answer}\" duration={duration_ms}ms"
            )
        }
        WorkflowRunEvent::InterviewTimeout {
            stage, duration_ms, ..
        } => {
            format!("[INTERVIEW_TIMEOUT] stage={stage} duration={duration_ms}ms")
        }
        WorkflowRunEvent::CheckpointSaved { node_id } => {
            format!("[CHECKPOINT_SAVED] node={node_id}")
        }
        WorkflowRunEvent::GitCheckpoint {
            node_id,
            git_commit_sha,
            status,
            ..
        } => {
            format!("[GIT_CHECKPOINT] node={node_id} sha={git_commit_sha} status={status}")
        }
        WorkflowRunEvent::EdgeSelected {
            from_node,
            to_node,
            label,
            condition,
        } => {
            let mut s = format!("[EDGE_SELECTED] from={from_node} to={to_node}");
            if let Some(l) = label {
                s.push_str(&format!(" label=\"{l}\""));
            }
            if let Some(c) = condition {
                s.push_str(&format!(" condition=\"{c}\""));
            }
            s
        }
        WorkflowRunEvent::LoopRestart { from_node, to_node } => {
            format!("[LOOP_RESTART] from={from_node} to={to_node}")
        }
        WorkflowRunEvent::Prompt { stage, text } => {
            let truncated = if text.len() > 80 { &text[..80] } else { text };
            format!("[PROMPT] stage={stage} text=\"{truncated}\"")
        }
        WorkflowRunEvent::Agent { stage, event } => match event {
            AgentEvent::AssistantMessage {
                model,
                usage,
                tool_call_count,
                ..
            } => {
                let total = (usage.input_tokens + usage.output_tokens) as u64;
                let mut s = format!("[ASSISTANT_MESSAGE] stage={stage} model={model} tokens={} tool_calls={tool_call_count}", HumanCount(total));
                if let Some(cache_read) = usage.cache_read_tokens {
                    s.push_str(&format!(" cache_read={}", HumanCount(cache_read as u64)));
                }
                if let Some(reasoning) = usage.reasoning_tokens {
                    s.push_str(&format!(" reasoning={}", HumanCount(reasoning as u64)));
                }
                s
            }
            AgentEvent::ToolCallStarted { tool_name, .. } => {
                format!("[TOOL_CALL_STARTED] stage={stage} tool={tool_name}")
            }
            AgentEvent::ToolCallCompleted {
                tool_name,
                is_error,
                ..
            } => {
                format!("[TOOL_CALL_COMPLETED] stage={stage} tool={tool_name} is_error={is_error}")
            }
            AgentEvent::Error { error } => {
                format!("[SESSION_ERROR] stage={stage} error=\"{error}\"")
            }
            AgentEvent::ContextWindowWarning { usage_percent, .. } => {
                format!("[CONTEXT_WINDOW_WARNING] stage={stage} usage={usage_percent}%")
            }
            AgentEvent::LoopDetected => format!("[LOOP_DETECTED] stage={stage}"),
            AgentEvent::TurnLimitReached { max_turns } => {
                format!("[TURN_LIMIT_REACHED] stage={stage} max_turns={max_turns}")
            }
            AgentEvent::CompactionStarted {
                estimated_tokens,
                context_window_size,
            } => {
                format!("[COMPACTION_STARTED] stage={stage} estimated_tokens={estimated_tokens} context_window={context_window_size}")
            }
            AgentEvent::CompactionCompleted {
                original_turn_count,
                preserved_turn_count,
                summary_token_estimate,
                tracked_file_count,
            } => {
                format!("[COMPACTION_COMPLETED] stage={stage} original_turns={original_turn_count} preserved_turns={preserved_turn_count} summary_tokens={summary_token_estimate} tracked_files={tracked_file_count}")
            }
            AgentEvent::LlmRetry {
                provider,
                model,
                attempt,
                delay_secs,
                error,
            } => {
                let delay_ms = (*delay_secs * 1000.0) as u64;
                format!("[LLM_RETRY] stage={stage} provider={provider} model={model} attempt={attempt} delay={delay_ms}ms error=\"{error}\"")
            }
            AgentEvent::SubAgentSpawned {
                agent_id,
                depth,
                task,
            } => {
                let short_id = &agent_id[..8.min(agent_id.len())];
                let task_preview = if task.len() > 60 { &task[..60] } else { task };
                format!("[SUBAGENT_SPAWNED] stage={stage} agent_id={short_id} depth={depth} task=\"{task_preview}\"")
            }
            AgentEvent::SubAgentCompleted {
                agent_id,
                depth,
                success,
                turns_used,
            } => {
                let short_id = &agent_id[..8.min(agent_id.len())];
                format!("[SUBAGENT_COMPLETED] stage={stage} agent_id={short_id} depth={depth} success={success} turns={turns_used}")
            }
            AgentEvent::SubAgentFailed {
                agent_id,
                depth,
                error,
            } => {
                let short_id = &agent_id[..8.min(agent_id.len())];
                format!("[SUBAGENT_FAILED] stage={stage} agent_id={short_id} depth={depth} error=\"{error}\"")
            }
            AgentEvent::SubAgentClosed { agent_id, depth } => {
                let short_id = &agent_id[..8.min(agent_id.len())];
                format!("[SUBAGENT_CLOSED] stage={stage} agent_id={short_id} depth={depth}")
            }
            AgentEvent::SubAgentEvent {
                agent_id,
                depth,
                event,
            } => {
                let short_id = &agent_id[..8.min(agent_id.len())];
                format!("[SUBAGENT_EVENT] stage={stage} agent_id={short_id} depth={depth} event={event:?}")
            }
            other => format!("[AGENT] stage={stage} event={other:?}"),
        },
        WorkflowRunEvent::ParallelEarlyTermination {
            reason,
            completed_count,
            pending_count,
        } => {
            format!("[PARALLEL_EARLY_TERMINATION] reason={reason} completed={completed_count} pending={pending_count}")
        }
        WorkflowRunEvent::SubgraphStarted {
            node_id,
            start_node,
        } => {
            format!("[SUBGRAPH_STARTED] node={node_id} start_node={start_node}")
        }
        WorkflowRunEvent::SubgraphCompleted {
            node_id,
            steps_executed,
            status,
            duration_ms,
        } => {
            format!("[SUBGRAPH_COMPLETED] node={node_id} steps={steps_executed} status={status} duration={duration_ms}ms")
        }
        WorkflowRunEvent::Sandbox { event } => {
            use arc_agent::SandboxEvent;
            match event {
                SandboxEvent::Initializing { provider } => format!("[SANDBOX_INITIALIZING] provider={provider}"),
                SandboxEvent::Ready { provider, duration_ms } => format!("[SANDBOX_READY] provider={provider} duration={duration_ms}ms"),
                SandboxEvent::InitializeFailed { provider, error, duration_ms } => format!("[SANDBOX_INIT_FAILED] provider={provider} error=\"{error}\" duration={duration_ms}ms"),
                SandboxEvent::CleanupStarted { provider } => format!("[SANDBOX_CLEANUP_STARTED] provider={provider}"),
                SandboxEvent::CleanupCompleted { provider, duration_ms } => format!("[SANDBOX_CLEANUP_COMPLETED] provider={provider} duration={duration_ms}ms"),
                SandboxEvent::CleanupFailed { provider, error } => format!("[SANDBOX_CLEANUP_FAILED] provider={provider} error=\"{error}\""),
                SandboxEvent::SnapshotPulling { name } => format!("[SANDBOX_SNAPSHOT_PULLING] name={name}"),
                SandboxEvent::SnapshotPulled { name, duration_ms } => format!("[SANDBOX_SNAPSHOT_PULLED] name={name} duration={duration_ms}ms"),
                SandboxEvent::SnapshotEnsuring { name } => format!("[SANDBOX_SNAPSHOT_ENSURING] name={name}"),
                SandboxEvent::SnapshotCreating { name } => format!("[SANDBOX_SNAPSHOT_CREATING] name={name}"),
                SandboxEvent::SnapshotReady { name, duration_ms } => format!("[SANDBOX_SNAPSHOT_READY] name={name} duration={duration_ms}ms"),
                SandboxEvent::SnapshotFailed { name, error } => format!("[SANDBOX_SNAPSHOT_FAILED] name={name} error=\"{error}\""),
                SandboxEvent::GitCloneStarted { url, branch } => {
                    let branch_str = branch.as_deref().unwrap_or("(default)");
                    format!("[SANDBOX_GIT_CLONE_STARTED] url={url} branch={branch_str}")
                }
                SandboxEvent::GitCloneCompleted { url, duration_ms } => format!("[SANDBOX_GIT_CLONE_COMPLETED] url={url} duration={duration_ms}ms"),
                SandboxEvent::GitCloneFailed { url, error } => format!("[SANDBOX_GIT_CLONE_FAILED] url={url} error=\"{error}\""),
            }
        }
        WorkflowRunEvent::SetupStarted { command_count } => {
            format!("[SETUP_STARTED] command_count={command_count}")
        }
        WorkflowRunEvent::SetupCommandStarted { command, index } => {
            format!("[SETUP_COMMAND_STARTED] index={index} command=\"{command}\"")
        }
        WorkflowRunEvent::SetupCommandCompleted {
            command,
            index,
            exit_code,
            duration_ms,
        } => {
            format!("[SETUP_COMMAND_COMPLETED] index={index} command=\"{command}\" exit_code={exit_code} duration={duration_ms}ms")
        }
        WorkflowRunEvent::SetupCompleted { duration_ms } => {
            format!("[SETUP_COMPLETED] duration={duration_ms}ms")
        }
        WorkflowRunEvent::SetupFailed {
            command,
            index,
            exit_code,
            stderr,
        } => {
            let truncated = if stderr.len() > 80 {
                &stderr[..80]
            } else {
                stderr
            };
            format!("[SETUP_FAILED] index={index} command=\"{command}\" exit_code={exit_code} stderr=\"{truncated}\"")
        }
        WorkflowRunEvent::StallWatchdogTimeout { node, idle_seconds } => {
            format!("[STALL_WATCHDOG_TIMEOUT] node={node} idle_seconds={idle_seconds}")
        }
        WorkflowRunEvent::AssetsCaptured { node_id, files_copied, total_bytes, files_skipped } => {
            format!("[ASSETS_CAPTURED] node={node_id} files_copied={files_copied} total_bytes={} files_skipped={files_skipped}", HumanBytes(*total_bytes))
        }
    };
    format!("{}", styles.dim.apply_to(body))
}

/// Compute the dollar cost for a stage's token usage, if pricing is available.
#[must_use]
pub fn compute_stage_cost(usage: &StageUsage) -> Option<f64> {
    let info = arc_llm::catalog::get_model_info(&usage.model)?;
    let input_rate = info.input_cost_per_million?;
    let output_rate = info.output_cost_per_million?;
    Some(
        usage.input_tokens as f64 * input_rate / 1_000_000.0
            + usage.output_tokens as f64 * output_rate / 1_000_000.0,
    )
}

/// Format a dollar cost for display (e.g. `"$1.23"`).
#[must_use]
pub fn format_cost(cost: f64) -> String {
    format!("${cost:.2}")
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_provider_default_is_local() {
        assert_eq!(SandboxProvider::default(), SandboxProvider::Local);
    }

    #[test]
    fn sandbox_provider_from_str() {
        assert_eq!(
            "local".parse::<SandboxProvider>().unwrap(),
            SandboxProvider::Local
        );
        assert_eq!(
            "docker".parse::<SandboxProvider>().unwrap(),
            SandboxProvider::Docker
        );
        assert_eq!(
            "daytona".parse::<SandboxProvider>().unwrap(),
            SandboxProvider::Daytona
        );
        assert_eq!(
            "LOCAL".parse::<SandboxProvider>().unwrap(),
            SandboxProvider::Local
        );
        assert!("invalid".parse::<SandboxProvider>().is_err());
    }

    #[test]
    fn sandbox_provider_display() {
        assert_eq!(SandboxProvider::Local.to_string(), "local");
        assert_eq!(SandboxProvider::Docker.to_string(), "docker");
        assert_eq!(SandboxProvider::Daytona.to_string(), "daytona");
    }

    fn test_styles() -> &'static Styles {
        static STYLES: std::sync::LazyLock<Styles> = std::sync::LazyLock::new(|| Styles::new(false));
        &STYLES
    }

    #[test]
    fn format_summary_sandbox_initializing() {
        let event = WorkflowRunEvent::Sandbox {
            event: arc_agent::SandboxEvent::Initializing {
                provider: "docker".into(),
            },
        };
        let s = format_event_summary(&event, test_styles());
        assert!(s.contains("[SANDBOX_INITIALIZING]"));
        assert!(s.contains("docker"));
    }

    #[test]
    fn format_summary_setup_started() {
        let event = WorkflowRunEvent::SetupStarted { command_count: 3 };
        let s = format_event_summary(&event, test_styles());
        assert!(s.contains("[SETUP_STARTED]"));
        assert!(s.contains("3"));
    }

    #[test]
    fn format_summary_subagent_spawned() {
        let event = WorkflowRunEvent::Agent {
            stage: "code".into(),
            event: AgentEvent::SubAgentSpawned {
                agent_id: "abcdef12-3456-7890-abcd-ef1234567890".into(),
                depth: 1,
                task: "list files".into(),
            },
        };
        let s = format_event_summary(&event, test_styles());
        assert!(s.contains("[SUBAGENT_SPAWNED]"));
        assert!(s.contains("abcdef12"));
        assert!(s.contains("depth=1"));
    }

    #[test]
    fn format_summary_subagent_completed() {
        let event = WorkflowRunEvent::Agent {
            stage: "code".into(),
            event: AgentEvent::SubAgentCompleted {
                agent_id: "abcdef12-xxxx".into(),
                depth: 1,
                success: true,
                turns_used: 5,
            },
        };
        let s = format_event_summary(&event, test_styles());
        assert!(s.contains("[SUBAGENT_COMPLETED]"));
        assert!(s.contains("success=true"));
        assert!(s.contains("turns=5"));
    }

    #[test]
    fn format_summary_subagent_event() {
        let event = WorkflowRunEvent::Agent {
            stage: "code".into(),
            event: AgentEvent::SubAgentEvent {
                agent_id: "abcdef12-xxxx".into(),
                depth: 1,
                event: Box::new(AgentEvent::SessionStarted),
            },
        };
        let s = format_event_summary(&event, test_styles());
        assert!(s.contains("[SUBAGENT_EVENT]"));
        assert!(s.contains("abcdef12"));
    }

}
