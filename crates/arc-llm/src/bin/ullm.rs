use std::io::{self, IsTerminal, Read};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use arc_llm::catalog;
use arc_llm::generate::{self, GenerateParams};

#[derive(Parser)]
#[command(name = "ullm")]
struct Cli {
    /// Skip loading .env file
    #[arg(long, global = true)]
    no_dotenv: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute a prompt
    Prompt {
        /// The prompt text (also accepts stdin)
        prompt: Option<String>,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,

        /// System prompt
        #[arg(short, long)]
        system: Option<String>,

        /// Do not stream output
        #[arg(long)]
        no_stream: bool,

        /// Show token usage
        #[arg(short, long)]
        usage: bool,

        /// JSON schema for structured output (inline JSON string)
        #[arg(short = 'S', long)]
        schema: Option<String>,

        /// key=value options (temperature, `max_tokens`, `top_p`)
        #[arg(short, long, value_parser = parse_option)]
        option: Vec<(String, String)>,
    },

    /// Manage models
    Models {
        #[command(subcommand)]
        command: Option<ModelsCommand>,
    },
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// List available models
    List {
        /// Filter by provider
        #[arg(short, long)]
        provider: Option<String>,

        /// Search for models matching this string
        #[arg(short, long)]
        query: Option<String>,
    },

    /// Download model metadata from OpenRouter
    Sync {
        /// URL to fetch models from
        #[arg(long, default_value = "https://openrouter.ai/api/v1/models")]
        url: String,

        /// Output file path
        #[arg(short, long, default_value = "openrouter_models.json")]
        output: String,
    },
}

fn parse_option(s: &str) -> Result<(String, String), String> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("expected key=value, got {s}"))?;
    Ok((key.to_string(), value.to_string()))
}

fn print_models_table(models: &[arc_llm::types::ModelInfo]) {
    println!(
        "{:<30} {:<12} {:<30} {:>14}",
        "ID", "PROVIDER", "ALIASES", "CONTEXT"
    );
    for model in models {
        let aliases = model.aliases.join(", ");
        println!(
            "{:<30} {:<12} {:<30} {:>14}",
            model.id, model.provider, aliases, model.context_window
        );
    }
}

fn read_stdin_prompt() -> Option<String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut buf = String::new();
    stdin.lock().read_to_string(&mut buf).ok()?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn resolve_prompt(arg: Option<String>, stdin: Option<String>) -> Result<String> {
    match (stdin, arg) {
        (Some(s), Some(a)) => Ok(format!("{s}\n{a}")),
        (Some(s), None) => Ok(s),
        (None, Some(a)) => Ok(a),
        (None, None) => bail!("Error: no prompt provided. Pass a prompt as an argument or pipe text via stdin."),
    }
}

/// Returns (`model_id`, provider) from the catalog, falling back to the first catalog model.
fn resolve_model(model_arg: Option<String>) -> (String, Option<String>) {
    let raw = model_arg.unwrap_or_else(|| {
        catalog::list_models(None)
            .first()
            .map_or_else(|| "claude-sonnet-4-5".to_string(), |m| m.id.clone())
    });
    match catalog::get_model_info(&raw) {
        Some(info) => (info.id, Some(info.provider)),
        None => (raw, None),
    }
}

fn apply_options(
    mut params: GenerateParams,
    options: &[(String, String)],
) -> Result<GenerateParams> {
    let mut provider_opts = serde_json::Map::new();

    for (key, value) in options {
        match key.as_str() {
            "temperature" => {
                let v: f64 = value
                    .parse()
                    .with_context(|| format!("invalid temperature value: {value}"))?;
                params = params.temperature(v);
            }
            "max_tokens" => {
                let v: i64 = value
                    .parse()
                    .with_context(|| format!("invalid max_tokens value: {value}"))?;
                params = params.max_tokens(v);
            }
            "top_p" => {
                let v: f64 = value
                    .parse()
                    .with_context(|| format!("invalid top_p value: {value}"))?;
                params = params.top_p(v);
            }
            _ => {
                provider_opts.insert(key.clone(), serde_json::Value::String(value.clone()));
            }
        }
    }

    if !provider_opts.is_empty() {
        params = params.provider_options(serde_json::Value::Object(provider_opts));
    }

    Ok(params)
}

struct PromptArgs {
    prompt: Option<String>,
    model: Option<String>,
    system: Option<String>,
    no_stream: bool,
    usage: bool,
    schema: Option<String>,
    option: Vec<(String, String)>,
}

fn print_usage(usage: &arc_llm::types::Usage) {
    eprintln!(
        "Tokens: {} input, {} output, {} total",
        usage.input_tokens, usage.output_tokens, usage.total_tokens
    );
}

async fn run_prompt(args: PromptArgs) -> Result<()> {
    let stdin_prompt = read_stdin_prompt();
    let prompt_text = resolve_prompt(args.prompt, stdin_prompt)?;
    let (model_id, provider) = resolve_model(args.model);

    eprintln!("Using model: {model_id}");

    let mut params = GenerateParams::new(&model_id).prompt(&prompt_text);
    if let Some(p) = provider {
        params = params.provider(&p);
    }
    if let Some(sys) = args.system {
        params = params.system(&sys);
    }
    params = apply_options(params, &args.option)?;

    let schema: Option<serde_json::Value> = match &args.schema {
        Some(s) => Some(serde_json::from_str(s).context("--schema must be valid JSON")?),
        None => None,
    };

    match (args.no_stream, schema) {
        (true, Some(schema)) => {
            let result = generate::generate_object(params, schema).await?;
            let object = result.output.as_ref().unwrap_or(&serde_json::Value::Null);
            println!("{}", serde_json::to_string_pretty(object)?);
            if args.usage {
                print_usage(&result.usage);
            }
        }
        (true, None) => {
            let result = generate::generate(params).await?;
            print!("{}", result.text());
            if args.usage {
                print_usage(&result.usage);
            }
        }
        (false, Some(schema)) => {
            let mut stream_result = generate::stream_object(params, schema).await?;
            while let Some(event) = stream_result.next().await {
                event?;
            }
            if let Some(object) = stream_result.object() {
                println!("{}", serde_json::to_string_pretty(object)?);
            }
        }
        (false, None) => {
            let mut stream_result = generate::stream(params).await?;
            while let Some(event) = stream_result.next().await {
                if let arc_llm::types::StreamEvent::TextDelta { delta, .. } = event? {
                    print!("{delta}");
                }
            }
            println!();
            if args.usage {
                if let Some(response) = stream_result.response() {
                    print_usage(&response.usage);
                }
            }
        }
    }

    Ok(())
}

async fn sync_models(url: &str, output: &str) -> Result<()> {
    let body = reqwest::get(url)
        .await
        .context("failed to connect to models endpoint")?
        .error_for_status()
        .context("models endpoint returned an error")?
        .text()
        .await
        .context("failed to read response body")?;

    let json: serde_json::Value =
        serde_json::from_str(&body).context("response is not valid JSON")?;
    let pretty =
        serde_json::to_string_pretty(&json).context("failed to format JSON")?;

    std::fs::write(output, &pretty).with_context(|| format!("failed to write {output}"))?;

    eprintln!("Saved models to {output}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.no_dotenv {
        dotenvy::dotenv().ok();
    }

    match cli.command {
        Command::Prompt {
            prompt,
            model,
            system,
            no_stream,
            usage,
            schema,
            option,
        } => {
            run_prompt(PromptArgs {
                prompt,
                model,
                system,
                no_stream,
                usage,
                schema,
                option,
            })
            .await?;
        }
        Command::Models { command } => {
            let command = command.unwrap_or(ModelsCommand::List {
                provider: None,
                query: None,
            });

            match command {
                ModelsCommand::List { provider, query } => {
                    let mut models = catalog::list_models(provider.as_deref());

                    if let Some(q) = &query {
                        let q_lower = q.to_lowercase();
                        models.retain(|m| {
                            m.id.to_lowercase().contains(&q_lower)
                                || m.display_name.to_lowercase().contains(&q_lower)
                                || m.aliases
                                    .iter()
                                    .any(|a| a.to_lowercase().contains(&q_lower))
                        });
                    }

                    print_models_table(&models);
                }
                ModelsCommand::Sync { url, output } => {
                    sync_models(&url, &output).await?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_cmd::Command;
    use predicates::prelude::*;

    #[allow(deprecated)] // assert_cmd deprecated cargo_bin; replacement macro has issues
    fn ullm() -> Command {
        Command::cargo_bin("ullm").unwrap()
    }

    // Step 1: models list prints all catalog models
    #[test]
    fn models_list_prints_all_models() {
        ullm()
            .args(["models", "list"])
            .assert()
            .success()
            .stdout(predicate::str::contains("claude-opus-4-6"))
            .stdout(predicate::str::contains("claude-sonnet-4-5"))
            .stdout(predicate::str::contains("gpt-5.2"))
            .stdout(predicate::str::contains("gemini-3.1-pro-preview"))
            .stdout(predicate::str::contains("anthropic"))
            .stdout(predicate::str::contains("openai"))
            .stdout(predicate::str::contains("gemini"));
    }

    // Step 2: models list --provider filters to that provider only
    #[test]
    fn models_list_filters_by_provider() {
        let assert = ullm()
            .args(["models", "list", "--provider", "anthropic"])
            .assert()
            .success()
            .stdout(predicate::str::contains("claude-opus-4-6"))
            .stdout(predicate::str::contains("claude-sonnet-4-5"));

        // Should NOT contain other providers
        assert
            .stdout(predicate::str::contains("gpt-5.2").not())
            .stdout(predicate::str::contains("gemini-3.1-pro-preview").not());
    }

    // Step 3: models list --query does substring match on id/name/aliases
    #[test]
    fn models_list_filters_by_query() {
        ullm()
            .args(["models", "list", "--query", "opus"])
            .assert()
            .success()
            .stdout(predicate::str::contains("claude-opus-4-6"))
            .stdout(predicate::str::contains("claude-sonnet-4-5").not());
    }

    #[test]
    fn models_list_query_is_case_insensitive() {
        ullm()
            .args(["models", "list", "--query", "OPUS"])
            .assert()
            .success()
            .stdout(predicate::str::contains("claude-opus-4-6"));
    }

    #[test]
    fn models_list_query_matches_aliases() {
        ullm()
            .args(["models", "list", "--query", "codex"])
            .assert()
            .success()
            .stdout(predicate::str::contains("gpt-5.2-codex"));
    }

    // Step 4: bare "models" defaults to list
    #[test]
    fn models_bare_defaults_to_list() {
        ullm()
            .args(["models"])
            .assert()
            .success()
            .stdout(predicate::str::contains("claude-opus-4-6"))
            .stdout(predicate::str::contains("gpt-5.2"))
            .stdout(predicate::str::contains("gemini-3.1-pro-preview"));
    }

    // models sync downloads and saves pretty-printed JSON
    #[test]
    fn models_sync_downloads_and_saves() {
        let server = httpmock::MockServer::start();
        let mock_response = serde_json::json!({
            "data": [{"id": "test-model", "name": "Test Model"}]
        });
        server.mock(|when, then| {
            when.method("GET").path("/api/v1/models");
            then.status(200)
                .header("content-type", "application/json")
                .body(serde_json::to_string(&mock_response).unwrap());
        });

        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("models.json");

        ullm()
            .args([
                "models",
                "sync",
                "--url",
                &server.url("/api/v1/models"),
                "--output",
                output_path.to_str().unwrap(),
            ])
            .assert()
            .success()
            .stderr(predicate::str::contains("Saved models to"));

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let expected = serde_json::to_string_pretty(&mock_response).unwrap();
        assert_eq!(contents, expected);
    }

    // models sync reports HTTP errors
    #[test]
    fn models_sync_reports_http_errors() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| {
            when.method("GET").path("/api/v1/models");
            then.status(500);
        });

        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("models.json");

        ullm()
            .args([
                "models",
                "sync",
                "--url",
                &server.url("/api/v1/models"),
                "--output",
                output_path.to_str().unwrap(),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("error").or(predicate::str::contains("Error")));
    }

    // models sync with real OpenRouter (requires network)
    #[test]
    #[ignore = "requires network"]
    fn models_sync_integration_smoke_test() {
        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("models.json");

        ullm()
            .args([
                "models",
                "sync",
                "--output",
                output_path.to_str().unwrap(),
            ])
            .assert()
            .success();

        let contents = std::fs::read_to_string(&output_path).unwrap();
        assert!(contents.contains("\"data\""));
    }

    // models sync --help succeeds and mentions openrouter
    #[test]
    fn models_sync_help_mentions_openrouter() {
        ullm()
            .args(["models", "sync", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("openrouter").or(predicate::str::contains("OpenRouter")));
    }

    // Step 5: prompt requires prompt text (errors when no prompt and stdin is tty)
    #[test]
    fn prompt_errors_without_prompt_text() {
        // assert_cmd provides no stdin by default (simulating a tty-like "empty pipe")
        // We pass an empty stdin to avoid tty detection
        ullm()
            .args(["prompt"])
            .write_stdin("")
            .assert()
            .failure()
            .stderr(predicate::str::contains("no prompt provided"));
    }

    // Step 9: stdin piping — reads from stdin when no prompt arg
    #[test]
    fn prompt_reads_from_stdin() {
        // This test verifies stdin is read, but will fail at the API call stage
        // since no API key is set. The error should NOT be "no prompt provided".
        let result = ullm()
            .args(["--no-dotenv", "prompt", "--no-stream", "-m", "test-model"])
            .write_stdin("hello from stdin")
            .assert()
            .failure();

        // Should NOT complain about missing prompt
        result.stderr(predicate::str::contains("no prompt provided").not());
    }

    // Step 9b: stdin + arg concatenation
    #[test]
    fn prompt_concatenates_stdin_and_arg() {
        // Same as above — verifies it doesn't error on "no prompt"
        let result = ullm()
            .args(["--no-dotenv", "prompt", "--no-stream", "-m", "test-model", "summarize this"])
            .write_stdin("some input text")
            .assert()
            .failure();

        result.stderr(predicate::str::contains("no prompt provided").not());
    }

    // Step 10: -o option parsing
    #[test]
    fn prompt_rejects_bad_option_format() {
        ullm()
            .args(["prompt", "-o", "bad_option", "hello"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("expected key=value"));
    }

    // Step 6/7/8: Integration tests gated behind API key
    #[test]
    #[ignore = "requires API key"]
    fn prompt_no_stream_generates_response() {
        ullm()
            .args(["prompt", "--no-stream", "-m", "claude-sonnet-4-5", "Say just the word 'hello'"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    #[ignore = "requires API key"]
    fn prompt_stream_generates_response() {
        ullm()
            .args(["prompt", "-m", "claude-sonnet-4-5", "Say just the word 'hello'"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    #[test]
    #[ignore = "requires API key"]
    fn prompt_usage_shows_tokens() {
        ullm()
            .args(["prompt", "--no-stream", "-u", "-m", "claude-sonnet-4-5", "Say just the word 'hello'"])
            .assert()
            .success()
            .stderr(predicate::str::contains("Tokens:"));
    }

    #[test]
    fn prompt_schema_rejects_invalid_json() {
        ullm()
            .args(["--no-dotenv", "prompt", "--no-stream", "-m", "test-model", "--schema", "not json", "hello"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--schema must be valid JSON"));
    }

    #[test]
    #[ignore = "requires API key"]
    fn prompt_schema_no_stream_generates_json() {
        let assert = ullm()
            .args([
                "prompt", "--no-stream", "-m", "claude-sonnet-4-5",
                "--schema", r#"{"type":"object","properties":{"greeting":{"type":"string"}},"required":["greeting"]}"#,
                "Return a JSON object with a greeting field set to hello",
            ])
            .assert()
            .success();

        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
        assert!(parsed.get("greeting").is_some(), "expected 'greeting' key in output");
    }

    #[test]
    #[ignore = "requires API key"]
    fn prompt_schema_stream_generates_json() {
        let assert = ullm()
            .args([
                "prompt", "-m", "claude-sonnet-4-5",
                "--schema", r#"{"type":"object","properties":{"greeting":{"type":"string"}},"required":["greeting"]}"#,
                "Return a JSON object with a greeting field set to hello",
            ])
            .assert()
            .success();

        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
        assert!(parsed.get("greeting").is_some(), "expected 'greeting' key in output");
    }
}
