use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use walkdir::WalkDir;

use super::spa_check::check_spa_asset_budgets;
use super::workspace_root;

const DEFAULT_ASSET_BUDGET_BYTES: u64 = 15 * 1024 * 1024;
const DEFAULT_PAYLOAD_BUDGET_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Args)]
pub(crate) struct SpaRefreshArgs {
    /// Repository root containing apps/fabro-web and lib/crates/fabro-spa.
    #[arg(long, hide = true)]
    root: Option<PathBuf>,
    /// Override the raw asset budget.
    #[arg(long, hide = true, default_value_t = DEFAULT_ASSET_BUDGET_BYTES)]
    pub(super) asset_budget_bytes: u64,
    /// Override the estimated gzip payload budget.
    #[arg(long, hide = true, default_value_t = DEFAULT_PAYLOAD_BUDGET_BYTES)]
    pub(super) payload_budget_bytes: u64,
}

pub(crate) fn spa_refresh(args: SpaRefreshArgs) -> Result<()> {
    let root = args.root.unwrap_or_else(workspace_root);
    spa_refresh_root_with_budgets(&root, args.asset_budget_bytes, args.payload_budget_bytes)
}

pub(crate) fn spa_refresh_root(root: &Path) -> Result<()> {
    spa_refresh_root_with_budgets(
        root,
        DEFAULT_ASSET_BUDGET_BYTES,
        DEFAULT_PAYLOAD_BUDGET_BYTES,
    )
}

#[expect(
    clippy::print_stdout,
    reason = "dev spa refresh command reports progress directly"
)]
fn spa_refresh_root_with_budgets(
    root: &Path,
    asset_budget_bytes: u64,
    payload_budget_bytes: u64,
) -> Result<()> {
    let web_dir = root.join("apps/fabro-web");
    let dist_dir = web_dir.join("dist");
    let asset_dir = root.join("lib/crates/fabro-spa/assets");

    println!("Running bun run build in apps/fabro-web...");
    run_bun_build(&web_dir)?;

    refresh_from_dist(
        root,
        &dist_dir,
        &asset_dir,
        asset_budget_bytes,
        payload_budget_bytes,
    )?;
    println!("Refreshed lib/crates/fabro-spa/assets");

    Ok(())
}

fn refresh_from_dist(
    root: &Path,
    dist_dir: &Path,
    asset_dir: &Path,
    asset_budget_bytes: u64,
    payload_budget_bytes: u64,
) -> Result<()> {
    let staging = TempDir::new(root, "refresh")?;
    mirror_dist(dist_dir, staging.path())?;
    check_spa_asset_budgets(staging.path(), asset_budget_bytes, payload_budget_bytes)?;
    mirror_dist(staging.path(), asset_dir)?;

    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "dev spa refresh intentionally runs a synchronous Bun subprocess"
)]
pub(super) fn run_bun_build(web_dir: &Path) -> Result<()> {
    let status = Command::new("bun")
        .args(["run", "build"])
        .current_dir(web_dir)
        .status()
        .with_context(|| format!("running bun run build in {}", web_dir.display()))?;
    if !status.success() {
        bail!("bun run build failed with {status}");
    }

    Ok(())
}

pub(super) fn mirror_dist(dist_dir: &Path, asset_dir: &Path) -> Result<()> {
    if !dist_dir.is_dir() {
        bail!("apps/fabro-web/dist is missing; run `bun run build` before mirroring SPA assets");
    }

    let plan = mirror_plan(dist_dir)?;
    remove_stale_entries(asset_dir, &plan)?;
    std::fs::create_dir_all(asset_dir)
        .with_context(|| format!("creating {}", asset_dir.display()))?;

    for relative_dir in &plan.dirs {
        let destination = asset_dir.join(relative_dir);
        std::fs::create_dir_all(&destination)
            .with_context(|| format!("creating {}", destination.display()))?;
    }

    for source_file in &plan.files {
        copy_if_changed(&source_file.source, &asset_dir.join(&source_file.relative))?;
    }

    write_if_changed(&asset_dir.join(".gitkeep"), b"")?;

    Ok(())
}

struct MirrorPlan {
    dirs:  BTreeSet<PathBuf>,
    files: Vec<SourceFile>,
}

struct SourceFile {
    source:   PathBuf,
    relative: PathBuf,
}

fn mirror_plan(dist_dir: &Path) -> Result<MirrorPlan> {
    let mut dirs = BTreeSet::new();
    let mut files = Vec::new();

    for entry in WalkDir::new(dist_dir) {
        let entry = entry.context("walking apps/fabro-web/dist")?;
        let source = entry.path();
        let relative = source
            .strip_prefix(dist_dir)
            .with_context(|| format!("{} is not under {}", source.display(), dist_dir.display()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }

        if entry.file_type().is_dir() {
            dirs.insert(relative.to_path_buf());
            continue;
        }

        if source.extension().and_then(|ext| ext.to_str()) == Some("map") {
            continue;
        }

        for ancestor in relative.ancestors().skip(1) {
            if ancestor.as_os_str().is_empty() {
                break;
            }
            dirs.insert(ancestor.to_path_buf());
        }

        files.push(SourceFile {
            source:   source.to_path_buf(),
            relative: relative.to_path_buf(),
        });
    }

    files.sort_by(|left, right| left.relative.cmp(&right.relative));

    Ok(MirrorPlan { dirs, files })
}

fn remove_stale_entries(asset_dir: &Path, plan: &MirrorPlan) -> Result<()> {
    if !asset_dir.exists() {
        return Ok(());
    }

    let desired_files = plan
        .files
        .iter()
        .map(|source| source.relative.clone())
        .chain([PathBuf::from(".gitkeep")])
        .collect::<BTreeSet<_>>();

    for entry in WalkDir::new(asset_dir).contents_first(true) {
        let entry = entry.context("walking fabro-spa assets")?;
        let path = entry.path();
        let relative = path
            .strip_prefix(asset_dir)
            .with_context(|| format!("{} is not under {}", path.display(), asset_dir.display()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }

        if entry.file_type().is_dir() {
            if !plan.dirs.contains(relative) {
                std::fs::remove_dir(path)
                    .with_context(|| format!("removing {}", path.display()))?;
            }
        } else if !desired_files.contains(relative) {
            std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
        }
    }

    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "dev spa refresh mirrors build output with synchronous filesystem operations"
)]
fn copy_if_changed(source: &Path, destination: &Path) -> Result<()> {
    if destination.is_file() && files_match(source, destination)? {
        return Ok(());
    }

    if destination.exists() {
        remove_destination(destination)?;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::copy(source, destination)
        .with_context(|| format!("copying {} to {}", source.display(), destination.display()))?;
    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "dev spa refresh writes marker files synchronously while mirroring build output"
)]
fn write_if_changed(destination: &Path, contents: &[u8]) -> Result<()> {
    if destination.is_file()
        && std::fs::read(destination)
            .with_context(|| format!("reading {}", destination.display()))?
            == contents
    {
        return Ok(());
    }

    if destination.exists() {
        remove_destination(destination)?;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(destination, contents)
        .with_context(|| format!("writing {}", destination.display()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "dev spa refresh compares generated asset bytes synchronously before mirroring"
)]
fn files_match(left: &Path, right: &Path) -> Result<bool> {
    let left_len = left
        .metadata()
        .with_context(|| format!("reading metadata for {}", left.display()))?
        .len();
    let right_len = right
        .metadata()
        .with_context(|| format!("reading metadata for {}", right.display()))?
        .len();
    if left_len != right_len {
        return Ok(false);
    }

    Ok(
        std::fs::read(left).with_context(|| format!("reading {}", left.display()))?
            == std::fs::read(right).with_context(|| format!("reading {}", right.display()))?,
    )
}

fn remove_destination(path: &Path) -> Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).with_context(|| format!("removing {}", path.display()))
    } else {
        std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))
    }
}

pub(super) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(super) fn new(root: &Path, label: &str) -> Result<Self> {
        let base = root.join("tmp");
        std::fs::create_dir_all(&base).with_context(|| format!("creating {}", base.display()))?;

        for attempt in 0..100 {
            let path = base.join(format!(
                "fabro-dev-spa-{label}-{}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("creating {}", path.display()));
                }
            }
        }

        bail!(
            "could not create temporary SPA staging directory under {}",
            base.display()
        )
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests stage temporary SPA fixture files with sync std::fs operations"
)]
mod tests {
    use std::path::Path;

    use super::{mirror_dist, refresh_from_dist};

    fn write_file(root: &Path, path: &str, contents: impl AsRef<[u8]>) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("fixture path should have parent"))
            .expect("creating fixture parent directory");
        std::fs::write(path, contents).expect("writing fixture file");
    }

    fn read_bytes(root: &Path, path: &str) -> Vec<u8> {
        std::fs::read(root.join(path)).expect("reading fixture file")
    }

    #[cfg(unix)]
    #[test]
    fn mirror_dist_preserves_unchanged_assets() {
        use std::os::unix::fs::MetadataExt;

        let fixture = tempfile::tempdir().expect("creating fixture");
        write_file(fixture.path(), "dist/index.html", b"index");
        write_file(fixture.path(), "assets/index.html", b"index");

        let before_inode = std::fs::metadata(fixture.path().join("assets/index.html"))
            .expect("reading initial metadata")
            .ino();

        mirror_dist(&fixture.path().join("dist"), &fixture.path().join("assets"))
            .expect("mirroring dist");

        let after_inode = std::fs::metadata(fixture.path().join("assets/index.html"))
            .expect("reading mirrored metadata")
            .ino();

        assert_eq!(
            before_inode, after_inode,
            "unchanged asset files should not be deleted and recreated"
        );
    }

    #[test]
    fn mirror_dist_removes_stale_files_source_maps_and_keeps_directory_tracked() {
        let fixture = tempfile::tempdir().expect("creating fixture");
        write_file(fixture.path(), "dist/index.html", b"index");
        write_file(fixture.path(), "dist/assets/app.js", b"app");
        write_file(fixture.path(), "dist/assets/app.js.map", b"map");
        write_file(fixture.path(), "assets/stale.txt", b"stale");

        mirror_dist(&fixture.path().join("dist"), &fixture.path().join("assets"))
            .expect("mirroring dist");

        assert!(fixture.path().join("assets/index.html").is_file());
        assert!(fixture.path().join("assets/assets/app.js").is_file());
        assert!(fixture.path().join("assets/.gitkeep").is_file());
        assert!(!fixture.path().join("assets/assets/app.js.map").exists());
        assert!(!fixture.path().join("assets/stale.txt").exists());
    }

    #[test]
    fn mirror_dist_missing_source_errors_cleanly() {
        let fixture = tempfile::tempdir().expect("creating fixture");

        let error = mirror_dist(&fixture.path().join("dist"), &fixture.path().join("assets"))
            .expect_err("missing dist should fail");

        assert!(
            error
                .to_string()
                .contains("apps/fabro-web/dist is missing; run `bun run build`"),
            "missing dist should explain how to recover: {error:#}"
        );
    }

    #[test]
    fn refresh_budget_failure_leaves_assets_untouched() {
        let fixture = tempfile::tempdir().expect("creating fixture");
        write_file(fixture.path(), "apps/fabro-web/dist/index.html", b"hello");
        write_file(
            fixture.path(),
            "lib/crates/fabro-spa/assets/index.html",
            b"embedded",
        );

        let error = refresh_from_dist(
            fixture.path(),
            &fixture.path().join("apps/fabro-web/dist"),
            &fixture.path().join("lib/crates/fabro-spa/assets"),
            4,
            100,
        )
        .expect_err("budget failure should fail");

        assert!(
            error
                .to_string()
                .contains("fabro-spa embedded assets exceed budget: 5 > 4"),
            "budget failure should report raw byte overage: {error:#}"
        );
        assert_eq!(
            read_bytes(fixture.path(), "lib/crates/fabro-spa/assets/index.html"),
            b"embedded"
        );
    }
}
