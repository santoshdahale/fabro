mod logging;

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
    let cli = Cli::parse();
    if !cli.no_dotenv {
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
    };
    debug!(command = %command_name, "CLI command started");

    match cli.command {
        Command::Llm { command } => match command {
            LlmCommand::Prompt(args) => arc_llm::cli::run_prompt(args).await?,
            LlmCommand::Chat(args) => arc_llm::cli::run_chat(args).await?,
        },
        Command::Agent(args) => arc_agent::cli::run_with_args(args).await?,
        Command::Run { command } => match command {
            RunCommand::Start(args) => {
                let styles: &'static arc_util::terminal::Styles =
                    Box::leak(Box::new(arc_util::terminal::Styles::detect_stderr()));
                let server_config = arc_api::server_config::load_server_config()?;
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
    }

    Ok(())
}
