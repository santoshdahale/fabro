use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::Serialize;
use tracing::{debug, info, warn};

#[derive(Args)]
pub struct RunFilterArgs {
    /// Only include runs started before this date (YYYY-MM-DD prefix match)
    #[arg(long)]
    pub before: Option<String>,

    /// Filter by workflow name (substring match)
    #[arg(long)]
    pub workflow: Option<String>,

    /// Filter by label (KEY=VALUE, repeatable, AND semantics)
    #[arg(long = "label", value_name = "KEY=VALUE")]
    pub label: Vec<String>,

    /// Include orphan directories (no manifest.json)
    #[arg(long)]
    pub orphans: bool,
}

#[derive(Args)]
pub struct RunsListArgs {
    #[command(flatten)]
    pub filter: RunFilterArgs,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct RunsPruneArgs {
    #[command(flatten)]
    pub filter: RunFilterArgs,

    /// Actually delete (default is dry-run)
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunInfo {
    pub run_id: String,
    pub dir_name: String,
    pub workflow_name: String,
    pub status: String,
    pub start_time: String,
    pub labels: HashMap<String, String>,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub is_orphan: bool,
}

/// Scan a logs base directory and return info about each run.
pub fn scan_runs(base: &Path) -> Result<Vec<RunInfo>> {
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let mut runs = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();

        debug!(dir = %dir_name, "scanning run directory");

        let manifest_path = path.join("manifest.json");
        if let Ok(manifest) = crate::manifest::Manifest::load(&manifest_path) {
            debug!(dir = %dir_name, "reading manifest");

            let run_id = manifest.run_id;
            let workflow_name = manifest.workflow_name;
            let start_time = manifest.start_time.to_rfc3339();
            let labels = manifest.labels;

            let status = read_status(&path);

            runs.push(RunInfo {
                run_id,
                dir_name,
                workflow_name,
                status,
                start_time,
                labels,
                path,
                is_orphan: false,
            });
        } else {
            // Orphan directory — no manifest.json
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.to_rfc3339()
                })
                .unwrap_or_default();

            runs.push(RunInfo {
                run_id: dir_name.clone(),
                dir_name,
                workflow_name: "[no manifest]".to_string(),
                status: "unknown".to_string(),
                start_time: mtime,
                labels: HashMap::new(),
                path,
                is_orphan: true,
            });
        }
    }

    // Sort by start_time descending (newest first)
    runs.sort_by(|a, b| b.start_time.cmp(&a.start_time));
    Ok(runs)
}

fn read_status(run_dir: &Path) -> String {
    if let Ok(conclusion) = crate::conclusion::Conclusion::load(&run_dir.join("conclusion.json")) {
        return conclusion.status.to_string();
    }
    if run_dir.join("run.pid").exists() {
        return "running".to_string();
    }
    "unknown".to_string()
}

/// Filter runs by criteria. Orphans are excluded unless `include_orphans` is true.
pub fn filter_runs(
    runs: &[RunInfo],
    before: Option<&str>,
    workflow: Option<&str>,
    labels: &[(String, String)],
    include_orphans: bool,
) -> Vec<RunInfo> {
    runs.iter()
        .filter(|r| {
            if r.is_orphan && !include_orphans {
                return false;
            }
            if let Some(before) = before {
                if !r.start_time.is_empty() && r.start_time.as_str() >= before {
                    return false;
                }
            }
            if let Some(pat) = workflow {
                if !r.workflow_name.contains(pat) {
                    return false;
                }
            }
            for (k, v) in labels {
                match r.labels.get(k) {
                    Some(val) if val == v => {}
                    _ => return false,
                }
            }
            true
        })
        .cloned()
        .collect()
}

fn parse_label_filters(label_args: &[String]) -> Vec<(String, String)> {
    label_args
        .iter()
        .filter_map(|s| s.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".arc")
}

pub(crate) fn default_logs_base() -> PathBuf {
    default_data_dir().join("logs")
}

/// Find a run directory by prefix match against run IDs.
pub fn find_run_by_prefix(base: &Path, prefix: &str) -> Result<PathBuf> {
    let runs = scan_runs(base).context("Failed to scan runs")?;
    let matches: Vec<_> = runs
        .iter()
        .filter(|r| r.run_id.starts_with(prefix))
        .collect();

    match matches.len() {
        0 => {
            warn!(run_id = %prefix, "No matching run found");
            bail!("No run found matching prefix '{prefix}'")
        }
        1 => {
            let run = &matches[0];
            debug!(run_id = %prefix, matched = %run.run_id, "Resolved run by prefix");
            Ok(run.path.clone())
        }
        n => {
            let ids: Vec<&str> = matches.iter().map(|r| r.run_id.as_str()).collect();
            bail!(
                "Ambiguous prefix '{prefix}': {n} runs match: {}",
                ids.join(", ")
            )
        }
    }
}

pub fn list_command(args: &RunsListArgs) -> Result<()> {
    let base = default_logs_base();
    let runs = scan_runs(&base)?;
    let label_filters = parse_label_filters(&args.filter.label);
    let filtered = filter_runs(
        &runs,
        args.filter.before.as_deref(),
        args.filter.workflow.as_deref(),
        &label_filters,
        args.filter.orphans,
    );

    if args.json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
        return Ok(());
    }

    if filtered.is_empty() {
        eprintln!("No runs found.");
        return Ok(());
    }

    // Print table header
    let header = format!(
        "{:<30} {:<25} {:<10} {:<25} LABELS",
        "RUN ID", "WORKFLOW", "STATUS", "STARTED"
    );
    println!("{header}");
    println!("{}", "-".repeat(100));

    for run in &filtered {
        let labels_str = run
            .labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let run_id_display = if run.run_id.len() > 28 {
            format!("{}...", &run.run_id[..25])
        } else {
            run.run_id.clone()
        };
        let start_display = if run.start_time.len() > 23 {
            run.start_time[..23].to_string()
        } else {
            run.start_time.clone()
        };
        println!(
            "{:<30} {:<25} {:<10} {:<25} {}",
            run_id_display, run.workflow_name, run.status, start_display, labels_str
        );
    }
    eprintln!("\n{} run(s) listed.", filtered.len());
    Ok(())
}

#[derive(Args)]
pub struct DfArgs {
    /// Show per-run breakdown
    #[arg(short, long)]
    pub verbose: bool,
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub fn df_command(args: &DfArgs) -> Result<()> {
    let data_dir = default_data_dir();
    let logs_base = data_dir.join("logs");
    df_from(args, &data_dir, &logs_base)
}

pub fn df_from(args: &DfArgs, data_dir: &Path, logs_base: &Path) -> Result<()> {
    // --- Runs ---
    let runs = scan_runs(logs_base)?;
    let mut active_count = 0u64;
    let mut total_run_size = 0u64;
    let mut reclaimable_run_size = 0u64;

    struct RunSizeInfo {
        run_id: String,
        workflow_name: String,
        status: String,
        start_time: String,
        size: u64,
    }

    let mut run_details: Vec<RunSizeInfo> = Vec::new();

    for run in &runs {
        let size = dir_size(&run.path);
        total_run_size += size;
        let is_active = run.status == "running";
        if is_active {
            active_count += 1;
        } else {
            reclaimable_run_size += size;
        }
        if args.verbose {
            run_details.push(RunSizeInfo {
                run_id: run.run_id.clone(),
                workflow_name: run.workflow_name.clone(),
                status: run.status.clone(),
                start_time: run.start_time.clone(),
                size,
            });
        }
    }

    // --- Logs ---
    let mut log_count = 0u64;
    let mut total_log_size = 0u64;
    if let Ok(entries) = std::fs::read_dir(logs_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "log" {
                        if let Ok(meta) = path.metadata() {
                            log_count += 1;
                            total_log_size += meta.len();
                        }
                    }
                }
            }
        }
    }

    // --- Databases ---
    let mut db_count = 0u64;
    let mut total_db_size = 0u64;
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".db") || name.ends_with(".db-wal") || name.ends_with(".db-shm") {
                    if let Ok(meta) = path.metadata() {
                        db_count += 1;
                        total_db_size += meta.len();
                    }
                }
            }
        }
    }

    // --- Summary table ---
    let run_reclaim_pct = if total_run_size > 0 {
        (reclaimable_run_size as f64 / total_run_size as f64 * 100.0) as u64
    } else {
        0
    };
    let log_reclaim_pct = if total_log_size > 0 { 100 } else { 0 };

    println!(
        "{:<14}{:>5}{:>11}{:>12}{:>16}",
        "TYPE", "COUNT", "ACTIVE", "SIZE", "RECLAIMABLE"
    );
    println!(
        "{:<14}{:>5}{:>11}{:>12}{:>12} ({run_reclaim_pct}%)",
        "Runs",
        runs.len(),
        active_count,
        format_size(total_run_size),
        format_size(reclaimable_run_size),
    );
    println!(
        "{:<14}{:>5}{:>11}{:>12}{:>12} ({log_reclaim_pct}%)",
        "Logs",
        log_count,
        "-",
        format_size(total_log_size),
        format_size(total_log_size),
    );
    println!(
        "{:<14}{:>5}{:>11}{:>12}{:>12} (0%)",
        "Databases",
        db_count,
        "-",
        format_size(total_db_size),
        format_size(0),
    );

    println!();
    println!("Data directory: {}", data_dir.display());

    // --- Verbose per-run breakdown ---
    if args.verbose {
        println!();
        println!(
            "{:<30} {:<18} {:<10} {:>5} {:>12}",
            "RUN ID", "WORKFLOW", "STATUS", "AGE", "SIZE"
        );

        let now = chrono::Utc::now();
        for detail in &run_details {
            let run_id_display = if detail.run_id.len() > 28 {
                format!("{}...", &detail.run_id[..25])
            } else {
                detail.run_id.clone()
            };
            let workflow_display = if detail.workflow_name.len() > 16 {
                format!("{}...", &detail.workflow_name[..13])
            } else {
                detail.workflow_name.clone()
            };
            let age = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&detail.start_time) {
                let dur = now.signed_duration_since(dt);
                if dur.num_days() > 0 {
                    format!("{}d", dur.num_days())
                } else if dur.num_hours() > 0 {
                    format!("{}h", dur.num_hours())
                } else {
                    format!("{}m", dur.num_minutes().max(1))
                }
            } else {
                "-".to_string()
            };
            let reclaimable_marker = if detail.status != "running" { " *" } else { "" };
            println!(
                "{:<30} {:<18} {:<10} {:>5} {:>10}{}",
                run_id_display,
                workflow_display,
                detail.status,
                age,
                format_size(detail.size),
                reclaimable_marker,
            );
        }
        println!();
        println!("* = reclaimable");
    }

    Ok(())
}

pub fn prune_command(args: &RunsPruneArgs) -> Result<()> {
    let base = default_logs_base();
    prune_from(args, &base)
}

pub fn prune_from(args: &RunsPruneArgs, base: &Path) -> Result<()> {
    let runs = scan_runs(base)?;
    let label_filters = parse_label_filters(&args.filter.label);
    let filtered = filter_runs(
        &runs,
        args.filter.before.as_deref(),
        args.filter.workflow.as_deref(),
        &label_filters,
        args.filter.orphans,
    );

    if filtered.is_empty() {
        eprintln!("No matching runs to prune.");
        return Ok(());
    }

    if args.yes {
        for run in &filtered {
            info!(run_id = %run.run_id, path = %run.path.display(), "deleting run");
            std::fs::remove_dir_all(&run.path)?;
        }
        eprintln!("{} run(s) deleted.", filtered.len());
    } else {
        for run in &filtered {
            debug!(run_id = %run.run_id, "would delete run (dry-run)");
            println!("would delete: {} ({})", run.dir_name, run.workflow_name);
        }
        eprintln!(
            "\n{} run(s) would be deleted. Pass --yes to confirm.",
            filtered.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_run_dir(
        base: &Path,
        dir_name: &str,
        manifest: Option<serde_json::Value>,
        conclusion_json: Option<serde_json::Value>,
        pid_file: bool,
    ) -> PathBuf {
        let dir = base.join(dir_name);
        fs::create_dir_all(&dir).unwrap();
        if let Some(m) = manifest {
            fs::write(
                dir.join("manifest.json"),
                serde_json::to_string_pretty(&m).unwrap(),
            )
            .unwrap();
        }
        if let Some(c) = conclusion_json {
            fs::write(
                dir.join("conclusion.json"),
                serde_json::to_string_pretty(&c).unwrap(),
            )
            .unwrap();
        }
        if pid_file {
            fs::write(dir.join("run.pid"), "12345").unwrap();
        }
        dir
    }

    #[test]
    fn scan_runs_reads_manifest_and_final() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        make_run_dir(
            base,
            "20260101-ABC123",
            Some(serde_json::json!({
                "run_id": "abc123",
                "workflow_name": "my-pipeline",
                "goal": "test goal",
                "start_time": "2026-01-01T12:00:00Z",
                "node_count": 2,
                "edge_count": 1,
                "labels": { "env": "prod" }
            })),
            Some(
                serde_json::json!({ "timestamp": "2026-01-01T12:01:00Z", "status": "success", "duration_ms": 60000 }),
            ),
            false,
        );

        make_run_dir(base, "arc-run-orphan", None, None, false);

        let runs = scan_runs(base).unwrap();
        assert_eq!(runs.len(), 2);

        let completed = runs.iter().find(|r| r.run_id == "abc123").unwrap();
        assert_eq!(completed.workflow_name, "my-pipeline");
        assert_eq!(completed.status, "success");
        assert_eq!(completed.labels.get("env").unwrap(), "prod");
        assert!(!completed.is_orphan);

        let orphan = runs.iter().find(|r| r.is_orphan).unwrap();
        assert_eq!(orphan.workflow_name, "[no manifest]");
        assert_eq!(orphan.status, "unknown");
    }

    #[test]
    fn scan_runs_detects_running_status() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        make_run_dir(
            base,
            "20260115-RUNNING1",
            Some(serde_json::json!({
                "run_id": "running-1",
                "workflow_name": "pipeline-a",
                "goal": "",
                "start_time": "2026-01-15T10:00:00Z",
                "node_count": 1,
                "edge_count": 0
            })),
            None,
            true,
        );

        let runs = scan_runs(base).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "running");
    }

    #[test]
    fn scan_runs_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = scan_runs(tmp.path()).unwrap();
        assert!(runs.is_empty());
    }

    #[test]
    fn scan_runs_missing_dir() {
        let runs = scan_runs(Path::new("/tmp/nonexistent-arc-test-dir")).unwrap();
        assert!(runs.is_empty());
    }

    #[test]
    fn filter_runs_before() {
        let runs = vec![
            RunInfo {
                run_id: "old".into(),
                dir_name: "d1".into(),
                workflow_name: "p".into(),
                status: "success".into(),
                start_time: "2025-06-01T00:00:00Z".into(),
                labels: HashMap::new(),
                path: PathBuf::from("/tmp/d1"),
                is_orphan: false,
            },
            RunInfo {
                run_id: "new".into(),
                dir_name: "d2".into(),
                workflow_name: "p".into(),
                status: "success".into(),
                start_time: "2026-03-01T00:00:00Z".into(),
                labels: HashMap::new(),
                path: PathBuf::from("/tmp/d2"),
                is_orphan: false,
            },
        ];
        let filtered = filter_runs(&runs, Some("2026-01-01"), None, &[], false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].run_id, "old");
    }

    #[test]
    fn filter_runs_workflow() {
        let runs = vec![
            RunInfo {
                run_id: "a".into(),
                dir_name: "d1".into(),
                workflow_name: "deploy-prod".into(),
                status: "success".into(),
                start_time: "2026-01-01T00:00:00Z".into(),
                labels: HashMap::new(),
                path: PathBuf::from("/tmp/d1"),
                is_orphan: false,
            },
            RunInfo {
                run_id: "b".into(),
                dir_name: "d2".into(),
                workflow_name: "test-suite".into(),
                status: "success".into(),
                start_time: "2026-01-01T00:00:00Z".into(),
                labels: HashMap::new(),
                path: PathBuf::from("/tmp/d2"),
                is_orphan: false,
            },
        ];
        let filtered = filter_runs(&runs, None, Some("deploy"), &[], false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].run_id, "a");
    }

    #[test]
    fn filter_runs_labels() {
        let runs = vec![
            RunInfo {
                run_id: "a".into(),
                dir_name: "d1".into(),
                workflow_name: "p".into(),
                status: "success".into(),
                start_time: "2026-01-01T00:00:00Z".into(),
                labels: HashMap::from([("env".into(), "prod".into())]),
                path: PathBuf::from("/tmp/d1"),
                is_orphan: false,
            },
            RunInfo {
                run_id: "b".into(),
                dir_name: "d2".into(),
                workflow_name: "p".into(),
                status: "success".into(),
                start_time: "2026-01-01T00:00:00Z".into(),
                labels: HashMap::from([("env".into(), "staging".into())]),
                path: PathBuf::from("/tmp/d2"),
                is_orphan: false,
            },
        ];
        let filtered = filter_runs(
            &runs,
            None,
            None,
            &[("env".to_string(), "prod".to_string())],
            false,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].run_id, "a");
    }

    #[test]
    fn filter_runs_orphans_excluded_by_default() {
        let runs = vec![RunInfo {
            run_id: "orphan".into(),
            dir_name: "d1".into(),
            workflow_name: "[no manifest]".into(),
            status: "unknown".into(),
            start_time: "".into(),
            labels: HashMap::new(),
            path: PathBuf::from("/tmp/d1"),
            is_orphan: true,
        }];
        let filtered = filter_runs(&runs, None, None, &[], false);
        assert!(filtered.is_empty());

        let filtered = filter_runs(&runs, None, None, &[], true);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn prune_dry_run_preserves_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        let dir = make_run_dir(
            base,
            "20250101-TOPRUNE",
            Some(serde_json::json!({
                "run_id": "to-prune",
                "workflow_name": "old-pipeline",
                "goal": "",
                "start_time": "2025-01-01T12:00:00Z",
                "node_count": 1,
                "edge_count": 0
            })),
            Some(
                serde_json::json!({ "timestamp": "2025-01-01T12:01:00Z", "status": "success", "duration_ms": 60000 }),
            ),
            false,
        );

        let args = RunsPruneArgs {
            filter: RunFilterArgs {
                before: Some("2026-01-01".into()),
                workflow: None,
                label: Vec::new(),
                orphans: false,
            },
            yes: false,
        };

        prune_from(&args, base).unwrap();
        assert!(dir.exists(), "dry-run should preserve directory");
    }

    #[test]
    fn prune_with_yes_deletes_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        let dir = make_run_dir(
            base,
            "20250101-TOPRUNE",
            Some(serde_json::json!({
                "run_id": "to-prune",
                "workflow_name": "old-pipeline",
                "goal": "",
                "start_time": "2025-01-01T12:00:00Z",
                "node_count": 1,
                "edge_count": 0
            })),
            Some(
                serde_json::json!({ "timestamp": "2025-01-01T12:01:00Z", "status": "success", "duration_ms": 60000 }),
            ),
            false,
        );

        // Also add a run that should NOT be pruned (too new)
        let keep_dir = make_run_dir(
            base,
            "20260301-KEEPTHIS",
            Some(serde_json::json!({
                "run_id": "keep-this",
                "workflow_name": "new-pipeline",
                "goal": "",
                "start_time": "2026-03-01T12:00:00Z",
                "node_count": 1,
                "edge_count": 0
            })),
            Some(
                serde_json::json!({ "timestamp": "2026-03-01T12:01:00Z", "status": "success", "duration_ms": 60000 }),
            ),
            false,
        );

        let args = RunsPruneArgs {
            filter: RunFilterArgs {
                before: Some("2026-01-01".into()),
                workflow: None,
                label: Vec::new(),
                orphans: false,
            },
            yes: true,
        };

        prune_from(&args, base).unwrap();
        assert!(!dir.exists(), "--yes should delete matching directory");
        assert!(
            keep_dir.exists(),
            "non-matching directory should be preserved"
        );
    }

    #[test]
    fn prune_orphans_with_yes() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        let orphan_dir = make_run_dir(base, "orphan-dir", None, None, false);

        let args = RunsPruneArgs {
            filter: RunFilterArgs {
                before: None,
                workflow: None,
                label: Vec::new(),
                orphans: true,
            },
            yes: true,
        };

        prune_from(&args, base).unwrap();
        assert!(!orphan_dir.exists(), "orphan directory should be deleted");
    }

    #[test]
    fn parse_label_filters_basic() {
        let args = vec!["env=prod".to_string()];
        let result = parse_label_filters(&args);
        assert_eq!(result, vec![("env".to_string(), "prod".to_string())]);
    }

    #[test]
    fn parse_label_filters_multiple() {
        let args = vec!["a=1".to_string(), "b=2".to_string()];
        let result = parse_label_filters(&args);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&("a".to_string(), "1".to_string())));
        assert!(result.contains(&("b".to_string(), "2".to_string())));
    }

    #[test]
    fn parse_label_filters_value_with_equals() {
        let args = vec!["key=a=b".to_string()];
        let result = parse_label_filters(&args);
        assert_eq!(result, vec![("key".to_string(), "a=b".to_string())]);
    }

    #[test]
    fn parse_label_filters_skips_no_equals() {
        let args = vec!["nope".to_string(), "a=1".to_string()];
        let result = parse_label_filters(&args);
        assert_eq!(result, vec![("a".to_string(), "1".to_string())]);
    }

    #[test]
    fn parse_label_filters_empty() {
        let args: Vec<String> = vec![];
        let result = parse_label_filters(&args);
        assert!(result.is_empty());
    }

    #[test]
    fn dir_size_works() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        // Create nested files with known sizes
        fs::write(base.join("a.txt"), "hello").unwrap(); // 5 bytes
        let sub = base.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("b.txt"), "world!").unwrap(); // 6 bytes

        assert_eq!(dir_size(base), 11);
    }

    #[test]
    fn df_reports_run_sizes() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        let logs_base = data_dir.join("logs");
        fs::create_dir(&logs_base).unwrap();

        // Running run
        make_run_dir(
            &logs_base,
            "20260308-RUNNING",
            Some(serde_json::json!({
                "run_id": "running-1",
                "workflow_name": "code-review",
                "goal": "",
                "start_time": "2026-03-08T10:00:00Z",
                "node_count": 1,
                "edge_count": 0
            })),
            None,
            true,
        );
        // Add a file to give it size
        fs::write(
            logs_base.join("20260308-RUNNING").join("data.bin"),
            vec![0u8; 100],
        )
        .unwrap();

        // Completed run
        make_run_dir(
            &logs_base,
            "20260307-DONE",
            Some(serde_json::json!({
                "run_id": "done-1",
                "workflow_name": "deploy",
                "goal": "",
                "start_time": "2026-03-07T10:00:00Z",
                "node_count": 1,
                "edge_count": 0
            })),
            Some(serde_json::json!({
                "timestamp": "2026-03-07T10:01:00Z",
                "status": "success",
                "duration_ms": 60000
            })),
            false,
        );
        fs::write(
            logs_base.join("20260307-DONE").join("data.bin"),
            vec![0u8; 200],
        )
        .unwrap();

        let args = DfArgs { verbose: false };
        // Should not panic
        df_from(&args, data_dir, &logs_base).unwrap();
    }

    #[test]
    fn df_reports_log_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        let logs_base = data_dir.join("logs");
        fs::create_dir(&logs_base).unwrap();

        fs::write(logs_base.join("cli-2026-03-08.log"), vec![0u8; 500]).unwrap();
        fs::write(logs_base.join("serve-2026-03-08.log"), vec![0u8; 300]).unwrap();

        let args = DfArgs { verbose: false };
        df_from(&args, data_dir, &logs_base).unwrap();
    }

    #[test]
    fn df_reports_database_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        let logs_base = data_dir.join("logs");
        fs::create_dir(&logs_base).unwrap();

        fs::write(data_dir.join("arc.db"), vec![0u8; 1024]).unwrap();
        fs::write(data_dir.join("arc.db-wal"), vec![0u8; 512]).unwrap();
        fs::write(data_dir.join("arc.db-shm"), vec![0u8; 32]).unwrap();

        let args = DfArgs { verbose: false };
        df_from(&args, data_dir, &logs_base).unwrap();
    }

    #[test]
    fn find_run_by_prefix_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_run_by_prefix(dir.path(), "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn find_run_by_prefix_single_match() {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = dir.path().join("20260101-ABC123");
        fs::create_dir_all(&run_dir).unwrap();
        fs::write(
            run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_id": "abc123-full-id",
                "workflow_name": "test",
                "goal": "",
                "start_time": "2026-01-01T12:00:00Z",
                "node_count": 1,
                "edge_count": 0
            }))
            .unwrap(),
        )
        .unwrap();

        let result = find_run_by_prefix(dir.path(), "abc123").unwrap();
        assert_eq!(result, run_dir);
    }

    #[test]
    fn find_run_by_prefix_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let subdirs = [("d1", "abc-111"), ("d2", "abc-222")];
        for (subdir, run_id) in subdirs {
            let run_dir = dir.path().join(subdir);
            fs::create_dir_all(&run_dir).unwrap();
            fs::write(
                run_dir.join("manifest.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "run_id": run_id,
                    "workflow_name": "test",
                    "goal": "",
                    "start_time": "2026-01-01T12:00:00Z",
                    "node_count": 1,
                    "edge_count": 0
                }))
                .unwrap(),
            )
            .unwrap();
        }

        let result = find_run_by_prefix(dir.path(), "abc");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Ambiguous"),
            "Should mention ambiguity"
        );
    }
}
