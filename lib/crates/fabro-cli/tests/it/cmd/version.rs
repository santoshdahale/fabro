use fabro_test::{fabro_snapshot, test_context};
use serde_json::Value;

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["version", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Show client and server version information

    Usage: fabro version [OPTIONS]

    Options:
          --json              Output as JSON [env: FABRO_JSON=]
          --server <SERVER>   Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --debug             Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --no-upgrade-check  Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet             Suppress non-essential output [env: FABRO_QUIET=]
          --verbose           Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help              Print help
    ----- stderr -----
    ");
}

#[test]
fn client_info_always_prints() {
    let context = test_context!();
    let output = context
        .command()
        .args(["version"])
        .output()
        .expect("command should run");

    assert!(output.status.success(), "version failed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        stdout.contains("Client:"),
        "missing client section: {stdout}"
    );
    assert!(
        stdout.contains("Server:"),
        "missing server section: {stdout}"
    );
}

#[test]
fn json_output() {
    let context = test_context!();
    let output = context
        .command()
        .args(["--json", "version"])
        .output()
        .expect("command should run");

    assert!(output.status.success(), "json version failed");
    let value: Value = serde_json::from_slice(&output.stdout).expect("json should parse");
    assert!(value["client"]["version"].is_string());
    assert!(value["server"]["version"].is_string());
}

#[test]
fn http_unreachable_shows_error() {
    let context = test_context!();
    let output = context
        .command()
        .args(["version", "--server", "http://127.0.0.1:1"])
        .output()
        .expect("command should run");

    assert!(output.status.success(), "version should still succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        stdout.contains("Client:"),
        "missing client section: {stdout}"
    );
    assert!(stdout.contains("Error:"), "missing error section: {stdout}");
}

#[test]
fn http_unreachable_json() {
    let context = test_context!();
    let output = context
        .command()
        .args(["--json", "version", "--server", "http://127.0.0.1:1"])
        .output()
        .expect("command should run");

    assert!(output.status.success(), "version should still succeed");
    let value: Value = serde_json::from_slice(&output.stdout).expect("json should parse");
    assert!(value["client"]["version"].is_string());
    assert!(value["server"]["address"].is_string());
    assert!(value["server"]["error"].is_string());
    assert!(value["server"].get("version").is_none());
}

#[test]
fn invalid_server_target_fails() {
    let context = test_context!();
    let output = context
        .command()
        .args(["version", "--server", "not-a-url"])
        .output()
        .expect("command should run");

    assert!(!output.status.success(), "version should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(!stderr.is_empty(), "expected error output");
}
