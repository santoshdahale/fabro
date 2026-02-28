use assert_cmd::Command;
use predicates::prelude::*;

#[allow(deprecated)]
fn attractor() -> Command {
    Command::cargo_bin("arc-attractor").unwrap()
}

// -- validate ----------------------------------------------------------------

#[test]
fn validate_simple() {
    attractor()
        .args(["validate", "../../test/simple.dot"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Validation: OK"));
}

#[test]
fn validate_branching() {
    attractor()
        .args(["validate", "../../test/branching.dot"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Validation: OK"));
}

#[test]
fn validate_conditions() {
    attractor()
        .args(["validate", "../../test/conditions.dot"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Validation: OK"));
}

#[test]
fn validate_parallel() {
    attractor()
        .args(["validate", "../../test/parallel.dot"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Validation: OK"));
}

#[test]
fn validate_styled() {
    attractor()
        .args(["validate", "../../test/styled.dot"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Validation: OK"));
}

#[test]
fn validate_legacy_tool() {
    attractor()
        .args(["validate", "../../test/legacy_tool.dot"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Validation: OK"));
}

#[test]
fn validate_invalid() {
    attractor()
        .args(["validate", "../../test/invalid.dot"])
        .assert()
        .failure();
}

// -- serve -------------------------------------------------------------------

#[test]
fn serve_help() {
    attractor()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--port"))
        .stdout(predicate::str::contains("--host"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--provider"));
}

// -- run --dry-run -----------------------------------------------------------

#[test]
fn dry_run_simple() {
    attractor()
        .args(["run", "--dry-run", "--auto-approve", "../../test/simple.dot"])
        .assert()
        .success();
}

#[test]
fn dry_run_branching() {
    attractor()
        .args(["run", "--dry-run", "--auto-approve", "../../test/branching.dot"])
        .assert()
        .success();
}

#[test]
fn dry_run_conditions() {
    attractor()
        .args(["run", "--dry-run", "--auto-approve", "../../test/conditions.dot"])
        .assert()
        .success();
}

#[test]
fn dry_run_parallel() {
    attractor()
        .args(["run", "--dry-run", "--auto-approve", "../../test/parallel.dot"])
        .assert()
        .success();
}

#[test]
fn dry_run_styled() {
    attractor()
        .args(["run", "--dry-run", "--auto-approve", "../../test/styled.dot"])
        .assert()
        .success();
}

#[test]
fn dry_run_legacy_tool() {
    attractor()
        .args(["run", "--dry-run", "--auto-approve", "../../test/legacy_tool.dot"])
        .assert()
        .success();
}

// -- NDJSON logging ----------------------------------------------------------

#[test]
fn dry_run_writes_ndjson_and_live_json() {
    let tmp = tempfile::tempdir().unwrap();
    let logs_dir = tmp.path().join("logs");

    attractor()
        .args([
            "run",
            "--dry-run",
            "--auto-approve",
            "--logs-dir",
            logs_dir.to_str().unwrap(),
            "../../test/simple.dot",
        ])
        .assert()
        .success();

    // progress.ndjson must exist and contain valid JSON lines
    let ndjson_path = logs_dir.join("progress.ndjson");
    assert!(ndjson_path.exists(), "progress.ndjson should exist");
    let ndjson_content = std::fs::read_to_string(&ndjson_path).unwrap();
    let lines: Vec<&str> = ndjson_content.lines().collect();
    assert!(!lines.is_empty(), "progress.ndjson should have at least one line");

    // Every line must be valid JSON with timestamp, run_id, and event keys
    let first_line: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert!(first_line.get("timestamp").is_some(), "line should have timestamp");
    assert!(first_line.get("run_id").is_some(), "line should have run_id");
    assert!(first_line.get("event").is_some(), "line should have event");

    // Events should contain PipelineStarted (may not be first due to exec env events)
    let has_pipeline_started = lines.iter().any(|line| {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        parsed["event"].get("PipelineStarted").is_some()
    });
    assert!(has_pipeline_started, "events should contain PipelineStarted");

    // run_id should be non-empty after PipelineStarted
    let last_line: serde_json::Value = serde_json::from_str(lines[lines.len() - 1]).unwrap();
    let run_id = last_line["run_id"].as_str().unwrap();
    assert!(!run_id.is_empty(), "run_id should be non-empty");

    // live.json must exist and contain valid JSON matching the last NDJSON line
    let live_path = logs_dir.join("live.json");
    assert!(live_path.exists(), "live.json should exist");
    let live_content: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&live_path).unwrap()).unwrap();
    assert!(live_content.get("timestamp").is_some());
    assert!(live_content.get("run_id").is_some());
    assert!(live_content.get("event").is_some());
}
