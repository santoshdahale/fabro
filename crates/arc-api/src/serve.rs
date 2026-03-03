use std::sync::Arc;

use arc_llm::provider::Provider;
use arc_util::terminal::Styles;
use tokio::net::TcpListener;
use tracing::info;

use clap::Args;

use crate::server::{build_router, create_app_state_with_options};
use arc_workflows::cli::backend::AgentApiBackend;
use arc_workflows::cli::SandboxProvider;
use arc_workflows::handler::default_registry;
use arc_workflows::interviewer::Interviewer;

#[derive(Args)]
pub struct ServeArgs {
    /// Port to listen on
    #[arg(long, default_value = "3000")]
    pub port: u16,

    /// Host address to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Override default LLM model
    #[arg(long)]
    pub model: Option<String>,

    /// Override default LLM provider
    #[arg(long)]
    pub provider: Option<String>,

    /// Execute with simulated LLM backend
    #[arg(long)]
    pub dry_run: bool,

    /// Sandbox for agent tools
    #[arg(long, value_enum)]
    pub sandbox: Option<SandboxProvider>,

    /// Serve static demo data (disables auth, read-only)
    #[arg(long)]
    pub demo: bool,
}

/// Start the HTTP API server.
///
/// # Errors
///
/// Returns an error if the server fails to bind or encounters a fatal error.
pub async fn serve_command(args: ServeArgs, styles: &'static Styles) -> anyhow::Result<()> {
    // Resolve dry-run mode (same pattern as run.rs)
    let dry_run_mode = if args.dry_run {
        true
    } else {
        match arc_llm::client::Client::from_env().await {
            Ok(c) if c.provider_names().is_empty() => {
                eprintln!(
                    "{yellow}Warning:{reset} No LLM providers configured. Running in dry-run mode.",
                    yellow = styles.yellow,
                    reset = styles.reset,
                );
                true
            }
            Ok(_) => false,
            Err(e) => {
                eprintln!(
                    "{yellow}Warning:{reset} Failed to initialize LLM client: {e}. Running in dry-run mode.",
                    yellow = styles.yellow, reset = styles.reset,
                );
                true
            }
        }
    };

    // Resolve model/provider defaults
    let provider_str = args.provider;
    let model = args.model.unwrap_or_else(|| match provider_str.as_deref() {
        Some("openai") => "gpt-5.2".to_string(),
        Some("gemini") => "gemini-3.1-pro-preview".to_string(),
        _ => "claude-opus-4-6".to_string(),
    });

    // Resolve model alias through catalog
    let (model, provider_str) = match arc_llm::catalog::get_model_info(&model) {
        Some(info) => (info.id, provider_str.or(Some(info.provider))),
        None => (model, provider_str),
    };

    // Parse provider string to enum (defaults to Anthropic)
    let provider_enum: Provider = provider_str
        .as_deref()
        .map(|s| s.parse::<Provider>())
        .transpose()
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .unwrap_or(Provider::Anthropic);

    // Build registry factory
    let factory = move |interviewer: Arc<dyn Interviewer>| {
        let model = model.clone();
        default_registry(interviewer, move || {
            if dry_run_mode {
                None
            } else {
                Some(Box::new(AgentApiBackend::new(
                    model.clone(),
                    provider_enum,
                    false,
                    styles,
                )))
            }
        })
    };

    // Initialize data directory and SQLite database
    let server_config = crate::server_config::load_server_config()?;
    let data_dir = crate::server_config::resolve_data_dir(&server_config);
    std::fs::create_dir_all(&data_dir)?;
    let db = arc_db::connect(&data_dir.join("arc.db")).await?;
    arc_db::initialize_db(&db).await?;

    let auth_mode = if args.demo {
        crate::jwt_auth::AuthMode::Disabled
    } else {
        crate::jwt_auth::resolve_auth_mode(&server_config.api)
    };

    let state = create_app_state_with_options(db, factory, dry_run_mode, args.demo);
    let router = build_router(state, auth_mode);

    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr).await?;

    info!(host = %args.host, port = args.port, dry_run = dry_run_mode, "API server started");

    eprintln!(
        "{bold}Arc server listening on {green}{addr}{reset}",
        bold = styles.bold,
        green = styles.green,
        reset = styles.reset,
    );
    if dry_run_mode {
        eprintln!(
            "{dim}(dry-run mode){reset}",
            dim = styles.dim,
            reset = styles.reset,
        );
    }

    axum::serve(listener, router).await?;

    Ok(())
}
