mod cli_config;
mod doctor;
mod logging;
mod setup;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::debug;

#[derive(Parser)]
#[command(name = "arc", version)]
struct Cli {
    /// Skip loading .env file
    #[arg(long, global = true)]
    no_dotenv: bool,

    /// Enable DEBUG-level logging (default is INFO)
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// LLM prompt operations
    Llm {
        #[command(subcommand)]
        command: LlmCommand,
    },
    /// Run an agentic coding session
    Agent(arc_agent::cli::AgentArgs),
    /// Launch and manage workflow runs
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
    /// Validate a workflow
    Validate(arc_workflows::cli::ValidateArgs),
    /// List and test LLM models
    Models {
        #[command(subcommand)]
        command: Option<arc_llm::cli::ModelsCommand>,
    },
    /// Start the HTTP API server
    Serve(arc_api::serve::ServeArgs),
    /// Check environment and integration health
    Doctor {
        /// Show detailed information for each check
        #[arg(short, long)]
        verbose: bool,

        /// Probe live services (LLM, sandbox, API, web, Brave Search)
        #[arg(short, long)]
        live: bool,
    },
    /// Interactive setup wizard for Arc
    Setup,
}

#[derive(Subcommand)]
enum RunCommand {
    /// Launch a workflow from a .dot or .toml task file
    Start(arc_workflows::cli::RunArgs),
    /// List workflow runs
    List(arc_workflows::cli::runs::RunsListArgs),
    /// Delete old workflow runs
    Prune(arc_workflows::cli::runs::RunsPruneArgs),
}

#[derive(Subcommand)]
enum LlmCommand {
    /// Execute a prompt
    Prompt(arc_llm::cli::PromptArgs),
    /// Interactive multi-turn chat
    Chat(arc_llm::cli::ChatArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();
    if !cli.no_dotenv {
        if let Some(home) = dirs::home_dir() {
            let _ = dotenvy::from_path(home.join(".arc").join(".env"));
        }
        dotenvy::dotenv().ok();
    }

    if let Err(err) = logging::init_tracing(cli.debug) {
        eprintln!("Warning: failed to initialize logging: {err:#}");
    }

    let command_name = match &cli.command {
        Command::Llm { .. } => "llm",
        Command::Agent(_) => "agent",
        Command::Run { .. } => "run",
        Command::Validate(_) => "validate",
        Command::Models { .. } => "models",
        Command::Serve(_) => "serve",
        Command::Doctor { .. } => "doctor",
        Command::Setup => "setup",
    };
    debug!(command = %command_name, "CLI command started");

    match cli.command {
        Command::Llm { command } => {
            let cli_config = cli_config::load_cli_config(None)?;
            let llm_defaults = cli_config.llm.as_ref();
            match command {
                LlmCommand::Prompt(mut args) => {
                    if args.model.is_none() {
                        args.model = llm_defaults.and_then(|l| l.model.clone());
                    }
                    arc_llm::cli::run_prompt(args).await?
                }
                LlmCommand::Chat(mut args) => {
                    if args.model.is_none() {
                        args.model = llm_defaults.and_then(|l| l.model.clone());
                    }
                    arc_llm::cli::run_chat(args).await?
                }
            }
        }
        Command::Agent(mut args) => {
            let cli_config = cli_config::load_cli_config(None)?;
            let agent_defaults = cli_config.agent.as_ref();
            args.apply_cli_defaults(
                agent_defaults.and_then(|a| a.provider.as_deref()),
                agent_defaults.and_then(|a| a.model.as_deref()),
                agent_defaults.and_then(|a| a.permissions),
                agent_defaults.and_then(|a| a.output_format),
            );
            arc_agent::cli::run_with_args(args).await?
        }
        Command::Run { command } => match command {
            RunCommand::Start(args) => {
                let styles: &'static arc_util::terminal::Styles =
                    Box::leak(Box::new(arc_util::terminal::Styles::detect_stderr()));
                let server_config = arc_api::server_config::load_server_config(None)?;
                arc_workflows::cli::run::run_command(args, server_config.run_defaults, styles)
                    .await?;
            }
            RunCommand::List(args) => {
                arc_workflows::cli::runs::list_command(&args)?;
            }
            RunCommand::Prune(args) => {
                arc_workflows::cli::runs::prune_command(&args)?;
            }
        },
        Command::Validate(args) => {
            let styles = arc_util::terminal::Styles::detect_stderr();
            arc_workflows::cli::validate::validate_command(&args, &styles)?;
        }
        Command::Models { command } => arc_llm::cli::run_models(command).await?,
        Command::Serve(args) => {
            let styles: &'static arc_util::terminal::Styles =
                Box::leak(Box::new(arc_util::terminal::Styles::detect_stderr()));
            arc_api::serve::serve_command(args, styles).await?;
        }
        Command::Doctor { verbose, live } => {
            let exit_code = doctor::run_doctor(verbose, live).await;
            std::process::exit(exit_code);
        }
        Command::Setup => {
            setup::run_setup().await?;
        }
    }

    Ok(())
}
