use fabro_test::{fabro_snapshot, test_context};

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["upgrade", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Upgrade fabro to the latest version

    Usage: fabro upgrade [OPTIONS]

    Options:
          --json               Output as JSON [env: FABRO_JSON=]
          --version <VERSION>  Target version (e.g. "0.5.0", "v0.5.0", or "v0.177.0-alpha.1")
          --debug              Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --force              Upgrade even if already on the target version
          --dry-run            Preview what would happen without making changes
          --no-upgrade-check   Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet              Suppress non-essential output [env: FABRO_QUIET=]
          --verbose            Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help               Print help
    ----- stderr -----
    "#);
}

#[test]
fn upgrade_invalid_version_errors() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["upgrade", "--version", "not-a-semver"]);

    fabro_snapshot!(context.filters(), cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----
    ----- stderr -----
    error: invalid version: not-a-semver
      > unexpected character 'n' while parsing major version number
    ");
}

#[test]
fn upgrade_already_on_current_version_short_circuits() {
    let context = test_context!();
    let mut filters = context.filters();
    filters.push((
        regex::escape(env!("CARGO_PKG_VERSION")),
        "[VERSION]".to_string(),
    ));
    let mut cmd = context.command();
    cmd.args(["upgrade", "--version", env!("CARGO_PKG_VERSION")]);

    fabro_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    ----- stderr -----
    Already on version [VERSION]
    ");
}

#[test]
fn upgrade_dry_run_prefers_latest_stable_release_for_gh_backend() {
    let context = test_context!();
    let fake_bin = context.temp_dir.join("fake-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();

    let fake_gh = fake_bin.join("gh");
    std::fs::write(
        &fake_gh,
        r#"#!/bin/sh
set -eu

case "$1" in
  --version)
    echo "gh version 2.89.0"
    ;;
  auth)
    test "$2" = "status"
    ;;
  api)
    test "$2" = "repos/fabro-sh/fabro/releases/latest"
    test "$3" = "--jq"
    test "$4" = ".tag_name"
    echo "v0.176.3"
    ;;
  release)
    test "$2" = "view"
    echo "v0.177.0-alpha.1"
    ;;
  *)
    echo "unexpected gh invocation: $*" >&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&fake_gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut filters = context.filters();
    filters.push((
        regex::escape(env!("CARGO_PKG_VERSION")),
        "[VERSION]".to_string(),
    ));
    filters.push((
        "(aarch64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)".to_string(),
        "[TARGET]".to_string(),
    ));

    let path = format!("{}:{}", fake_bin.display(), std::env::var("PATH").unwrap());
    let mut cmd = context.command();
    cmd.env("PATH", path).args(["upgrade", "--dry-run"]);

    fabro_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    ----- stderr -----
    Would upgrade fabro from [VERSION] to 0.176.3
      tag: v0.176.3
      target: [TARGET]
    ");
}
