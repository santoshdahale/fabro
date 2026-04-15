use assert_cmd::Command;
use fabro_test::{fabro_snapshot, test_context};

fn remove_provider_env(cmd: &mut Command) -> &mut Command {
    cmd.env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .env_remove("KIMI_API_KEY")
        .env_remove("ZAI_API_KEY")
        .env_remove("MINIMAX_API_KEY")
        .env_remove("INCEPTION_API_KEY")
}

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["model", "test", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Test model availability by sending a simple prompt

    Usage: fabro model test [OPTIONS]

    Options:
          --json                 Output as JSON [env: FABRO_JSON=]
          --server <SERVER>      Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --debug                Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
      -p, --provider <PROVIDER>  Filter by provider
      -m, --model <MODEL>        Test a specific model
          --no-upgrade-check     Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --deep                 Run a multi-turn tool-use test (catches reasoning round-trip bugs)
          --quiet                Suppress non-essential output [env: FABRO_QUIET=]
          --verbose              Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help                 Print help
    ----- stderr -----
    ");
}

#[test]
fn model_test_unknown_model_errors() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["model", "test", "--model", "nonexistent-model-xyz"]);

    fabro_snapshot!(context.filters(), cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----
    ----- stderr -----
    Testing nonexistent-model-xyz... done
    error: Unknown model: nonexistent-model-xyz
    ");
}

#[test]
fn single_model_skip_exits_nonzero() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["model", "test", "--model", "gemini-3.1-pro-preview"]);
    remove_provider_env(&mut cmd);

    fabro_snapshot!(context.filters(), cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----
    MODEL                   PROVIDER  ALIASES     CONTEXT          COST     SPEED  RESULT         
     gemini-3.1-pro-preview  gemini    gemini-pro       1m  $2.0 / $12.0  85 tok/s  not configured
    ----- stderr -----
    Testing gemini-3.1-pro-preview... done
    error: 1 model(s) failed
    ");
}

#[test]
fn bulk_skip_exits_zero_and_prints_summary() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["model", "test"]);
    remove_provider_env(&mut cmd);

    let output = cmd.output().expect("command should execute");
    assert!(
        output.status.success(),
        "bulk skip should exit 0:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Skipped"),
        "should report skipped models:\n{stderr}"
    );
}

#[test]
fn json_output_includes_skipped_models() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args([
        "model",
        "test",
        "--model",
        "gemini-3.1-pro-preview",
        "--json",
    ]);
    remove_provider_env(&mut cmd);

    let output = cmd.output().expect("failed to execute model test");
    assert!(
        !output.status.success(),
        "expected single-model skip to exit non-zero:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");

    assert_eq!(json["failures"], 1);
    assert_eq!(json["skipped"], 1);
    assert_eq!(json["results"][0]["result"], "skip");
}
