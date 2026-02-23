use agent::{
    AnthropicProfile, EventData, EventKind, GeminiProfile, LocalExecutionEnvironment, OpenAiProfile,
    ProviderProfile, Session, SessionConfig, ToolApprovalFn, Turn,
};
use clap::{Parser, ValueEnum};
use llm::client::Client;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// Minimal CLI for the agent agentic loop.
#[derive(Parser)]
#[command(name = "agent")]
struct Cli {
    /// Task prompt
    prompt: String,

    /// LLM provider (anthropic, openai, gemini)
    #[arg(long, default_value = "anthropic")]
    provider: String,

    /// Model name (defaults per provider)
    #[arg(long)]
    model: Option<String>,

    /// Permission level for tool execution
    #[arg(long, default_value = "read-write", value_enum)]
    permissions: PermissionLevel,

    /// Skip interactive prompts; deny tools outside permission level
    #[arg(long)]
    auto_approve: bool,

    /// Print LLM request/response debug info to stderr
    #[arg(long)]
    debug: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PermissionLevel {
    ReadOnly,
    ReadWrite,
    Full,
}

fn default_model(provider: &str) -> &'static str {
    match provider {
        "openai" => "gpt-5.2",
        "gemini" => "gemini-3-pro-preview",
        // anthropic and unknown providers
        _ => "claude-sonnet-4-5-20250514",
    }
}

fn tool_category(name: &str) -> &'static str {
    match name {
        "read_file" | "read_many_files" | "grep" | "glob" | "list_dir" => "read",
        "write_file" | "edit_file" | "apply_patch" => "write",
        // shell and unknown tools require highest permission
        _ => "shell",
    }
}

fn is_auto_approved(level: PermissionLevel, category: &str) -> bool {
    matches!(
        (level, category),
        (_, "read")
            | (PermissionLevel::ReadWrite | PermissionLevel::Full, "write")
            | (PermissionLevel::Full, "shell")
    )
}

fn build_tool_approval(permissions: PermissionLevel, is_interactive: bool) -> ToolApprovalFn {
    let level = Arc::new(Mutex::new(permissions));

    Arc::new(move |tool_name: &str, _args: &serde_json::Value| {
        let current_level = *level.lock().expect("permission lock poisoned");

        if is_auto_approved(current_level, tool_category(tool_name)) {
            return Ok(());
        }

        if !is_interactive {
            return Err(format!(
                "{tool_name} tool denied at current permission level"
            ));
        }

        // Interactive prompt on stderr
        let category = tool_category(tool_name);
        eprint!("Allow {tool_name} ({category})? [y]es / [n]o / [a]lways: ");
        std::io::stderr().flush().ok();

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read input: {e}"))?;

        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(()),
            "a" | "always" => {
                let mut lvl = level.lock().expect("permission lock poisoned");
                *lvl = if category == "write" {
                    PermissionLevel::ReadWrite
                } else {
                    PermissionLevel::Full
                };
                Ok(())
            }
            _ => Err(format!("{tool_name} tool denied by user")),
        }
    })
}

fn build_profile(provider: &str, model: &str) -> Arc<dyn ProviderProfile> {
    match provider {
        "openai" => Arc::new(OpenAiProfile::new(model)),
        "gemini" => Arc::new(GeminiProfile::new(model)),
        // anthropic and unknown providers
        _ => Arc::new(AnthropicProfile::new(model)),
    }
}

fn validate_api_key(provider: &str) -> bool {
    match provider {
        "anthropic" => std::env::var("ANTHROPIC_API_KEY").is_ok(),
        "openai" => std::env::var("OPENAI_API_KEY").is_ok(),
        "gemini" => {
            std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok()
        }
        _ => false,
    }
}

fn print_output(session: &Session) {
    for turn in session.history().turns() {
        if let Turn::Assistant { content, .. } = turn {
            if !content.is_empty() {
                println!("{content}");
            }
        }
    }
}

fn print_summary(session: &Session) {
    let (mut turn_count, mut tool_call_count, mut total_tokens) = (0usize, 0usize, 0i64);
    for turn in session.history().turns() {
        if let Turn::Assistant {
            tool_calls, usage, ..
        } = turn
        {
            turn_count += 1;
            tool_call_count += tool_calls.len();
            total_tokens += usage.total_tokens;
        }
    }
    let token_str = if total_tokens >= 1000 {
        format!("{}k tokens", total_tokens / 1000)
    } else {
        format!("{total_tokens} tokens")
    };
    eprintln!("Done ({turn_count} turns, {tool_call_count} tool calls, {token_str})");
}

/// Middleware that logs LLM request/response summaries to stderr.
struct DebugMiddleware;

#[async_trait::async_trait]
impl llm::middleware::Middleware for DebugMiddleware {
    async fn handle_complete(
        &self,
        request: llm::types::Request,
        next: llm::middleware::NextFn,
    ) -> Result<llm::types::Response, llm::error::SdkError> {
        eprintln!(
            "[debug] request: model={} messages={} tools={}",
            request.model,
            request.messages.len(),
            request.tools.as_ref().map_or(0, Vec::len),
        );
        let response = next(request).await?;
        eprintln!(
            "[debug] response: model={} finish={:?} usage=({}/{}/{})",
            response.model,
            response.finish_reason,
            response.usage.input_tokens,
            response.usage.output_tokens,
            response.usage.total_tokens,
        );
        Ok(response)
    }

    async fn handle_stream(
        &self,
        request: llm::types::Request,
        next: llm::middleware::NextStreamFn,
    ) -> Result<llm::provider::StreamEventStream, llm::error::SdkError> {
        next(request).await
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Validate provider API key
    if !validate_api_key(&cli.provider) {
        anyhow::bail!("API key not set for provider '{}'", cli.provider);
    }

    // Build LLM client
    let mut client = Client::from_env()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create LLM client: {e}"))?;

    if cli.debug {
        client.add_middleware(Arc::new(DebugMiddleware));
    }

    // Resolve model and build profile
    let model = cli
        .model
        .as_deref()
        .unwrap_or_else(|| default_model(&cli.provider));
    let profile = build_profile(&cli.provider, model);

    // Build execution environment
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env = Arc::new(LocalExecutionEnvironment::new(cwd));

    // Build tool approval callback
    let is_interactive = std::io::stdin().is_terminal() && !cli.auto_approve;
    let tool_approval = build_tool_approval(cli.permissions, is_interactive);

    let config = SessionConfig {
        tool_approval: Some(tool_approval),
        ..SessionConfig::default()
    };

    let mut session = Session::new(client, profile, env, config);

    // SIGINT handler
    let abort_flag = session.abort_flag_handle();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        abort_flag.store(true, Ordering::SeqCst);
    });

    // Subscribe to events for real-time tool status on stderr
    let mut rx = session.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match (&event.kind, &event.data) {
                (EventKind::ToolCallStart, EventData::ToolCall { tool_name, .. }) => {
                    eprintln!("[tool] {tool_name}");
                }
                (EventKind::Error, EventData::Error { error }) => {
                    eprintln!("[error] {error}");
                }
                _ => {}
            }
        }
    });

    // Initialize and run
    session.initialize().await;
    let result = session.process_input(&cli.prompt).await;

    // Print assistant text to stdout
    print_output(&session);

    // Print completion summary to stderr
    print_summary(&session);

    // Propagate errors for exit code
    result?;
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[error] {e}");
            ExitCode::FAILURE
        }
    }
}
