//! End-to-end tests exercising full resolver pipeline with realistic devcontainer configs.
//! These tests verify the 4 critical gaps are wired correctly through the entire stack:
//!   1. onCreateCommand
//!   2. build.args
//!   3. containerEnv
//!   4. dockerComposeFile array

use arc_devcontainer::{Command, DevcontainerResolver};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Realistic Python project: Dockerfile + build.args + containerEnv + onCreateCommand + remoteEnv
/// Verifies all 4 gaps work together in a single config.
#[tokio::test]
async fn realistic_python_project() {
    let config = DevcontainerResolver::resolve(&fixture_path("realistic-python"))
        .await
        .unwrap();

    // Gap 2: build.args exposed for docker build --build-arg
    assert_eq!(
        config.build_args.get("PYTHON_VERSION").map(String::as_str),
        Some("3.12")
    );

    // Gap 3: containerEnv baked into Dockerfile as ENV directives
    assert!(config.dockerfile.contains("ENV PIP_NO_CACHE_DIR=1"));
    assert!(config.dockerfile.contains("ENV PYTHONDONTWRITEBYTECODE=1"));
    assert_eq!(
        config.container_env.get("PIP_NO_CACHE_DIR").map(String::as_str),
        Some("1")
    );

    // After fix: only containerEnv is baked into Dockerfile (remoteEnv is runtime-only)
    assert!(config.dockerfile.contains("ENV PYTHONUNBUFFERED=1"));
    // environment HashMap gets the remoteEnv value
    assert_eq!(
        config.environment.get("PYTHONUNBUFFERED").map(String::as_str),
        Some("yes")
    );

    // Gap 3: remoteEnv with variable substitution
    assert_eq!(
        config.environment.get("PYTHONPATH").map(String::as_str),
        Some("/workspaces/realistic-python/src")
    );

    // Gap 1: onCreateCommand parsed and exposed
    assert_eq!(config.on_create_commands.len(), 1);
    assert!(
        matches!(&config.on_create_commands[0], Command::Shell(s) if s == "pip install -r requirements.txt")
    );

    // Other lifecycle commands still work
    assert_eq!(config.post_create_commands.len(), 1);
    assert!(
        matches!(&config.post_create_commands[0], Command::Shell(s) if s == "python manage.py migrate")
    );
    assert_eq!(config.post_start_commands.len(), 1);
    assert!(
        matches!(&config.post_start_commands[0], Command::Shell(s) if s == "python manage.py runserver 0.0.0.0:8000")
    );

    // Dockerfile content is the actual file (not generated FROM line)
    assert!(config.dockerfile.contains("ARG PYTHON_VERSION=3.11"));
    assert!(config.dockerfile.contains("apt-get update"));

    // Standard fields
    assert_eq!(config.remote_user.as_deref(), Some("developer"));
    assert_eq!(config.forwarded_ports, vec![8000, 5432]);
    assert!(config.compose_files.is_empty());
}

/// Realistic compose project: multi-file compose + containerEnv + onCreateCommand + remoteEnv
/// Verifies gaps 1, 3, 4 work together in compose mode.
#[tokio::test]
async fn realistic_compose_project() {
    let config = DevcontainerResolver::resolve(&fixture_path("realistic-compose"))
        .await
        .unwrap();

    // Gap 4: multiple compose files resolved
    assert_eq!(config.compose_files.len(), 2);
    assert_eq!(config.compose_service.as_deref(), Some("app"));

    // Gap 4: image from base compose file (override doesn't change image)
    assert!(config.dockerfile.contains("FROM node:20-bookworm"));

    // Ports merged from both compose files (base: 3000, 9229; override: 4000) + forwardPorts (8080)
    assert!(config.forwarded_ports.contains(&3000));
    assert!(config.forwarded_ports.contains(&9229));
    assert!(config.forwarded_ports.contains(&4000));
    assert!(config.forwarded_ports.contains(&8080));
    assert_eq!(config.forwarded_ports.len(), 4);

    // Gap 4: environment merged from both compose files + remoteEnv
    assert_eq!(
        config.environment.get("NODE_ENV").map(String::as_str),
        Some("development")
    );
    assert_eq!(
        config.environment.get("DEBUG").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        config.environment.get("LOG_LEVEL").map(String::as_str),
        Some("verbose")
    );
    // remoteEnv values
    assert_eq!(
        config.environment.get("DATABASE_URL").map(String::as_str),
        Some("postgres://postgres:devpass@db:5432/myapp_dev")
    );
    assert_eq!(
        config.environment.get("REDIS_URL").map(String::as_str),
        Some("redis://redis:6379")
    );

    // Gap 3: containerEnv exposed on config
    assert_eq!(
        config.container_env.get("TERM").map(String::as_str),
        Some("xterm-256color")
    );
    assert_eq!(
        config.container_env.get("EDITOR").map(String::as_str),
        Some("vim")
    );

    // Gap 1: onCreateCommand in compose mode
    assert_eq!(config.on_create_commands.len(), 1);
    assert!(matches!(&config.on_create_commands[0], Command::Shell(s) if s == "npm ci"));

    // Other lifecycle commands
    assert_eq!(config.post_create_commands.len(), 1);
    assert!(
        matches!(&config.post_create_commands[0], Command::Shell(s) if s == "npm run db:migrate")
    );
    assert_eq!(config.post_start_commands.len(), 1);
    assert!(matches!(&config.post_start_commands[0], Command::Shell(s) if s == "npm run dev"));

    // User comes from compose (node) but remoteUser also set to node
    assert_eq!(config.remote_user.as_deref(), Some("node"));
    assert_eq!(config.workspace_folder, "/workspace");
}

/// All lifecycle commands in different forms: string, array, object, and the new onCreateCommand.
#[tokio::test]
async fn all_lifecycle_command_forms() {
    let config = DevcontainerResolver::resolve(&fixture_path("all-lifecycle"))
        .await
        .unwrap();

    // initializeCommand as string
    assert_eq!(config.initialize_commands.len(), 1);
    assert!(matches!(&config.initialize_commands[0], Command::Shell(s) if s == "echo pre-build"));

    // Gap 1: onCreateCommand as array
    assert_eq!(config.on_create_commands.len(), 1);
    assert!(matches!(&config.on_create_commands[0], Command::Args(args) if args == &["make", "setup"]));

    // postCreateCommand as object (parallel)
    assert_eq!(config.post_create_commands.len(), 1);
    assert!(matches!(&config.post_create_commands[0], Command::Parallel(map) if map.len() == 2));

    // postStartCommand as string
    assert_eq!(config.post_start_commands.len(), 1);
    assert!(matches!(&config.post_start_commands[0], Command::Shell(s) if s == "echo started"));
}

/// Verify containerEnv doesn't pollute the environment HashMap (which is remoteEnv only).
#[tokio::test]
async fn container_env_separate_from_environment() {
    let config = DevcontainerResolver::resolve(&fixture_path("realistic-python"))
        .await
        .unwrap();

    // container_env has containerEnv values
    assert!(config.container_env.contains_key("PYTHONDONTWRITEBYTECODE"));
    assert!(config.container_env.contains_key("PIP_NO_CACHE_DIR"));

    // environment only has remoteEnv values (not containerEnv-only keys)
    assert!(!config.environment.contains_key("PYTHONDONTWRITEBYTECODE"));
    assert!(!config.environment.contains_key("PIP_NO_CACHE_DIR"));
    // PYTHONUNBUFFERED is in both - environment gets remoteEnv value
    assert_eq!(
        config.environment.get("PYTHONUNBUFFERED").map(String::as_str),
        Some("yes")
    );
}

/// Verify build_args default to empty in non-dockerfile modes.
#[tokio::test]
async fn build_args_empty_in_image_and_compose_modes() {
    let image_config = DevcontainerResolver::resolve(&fixture_path("image-only"))
        .await
        .unwrap();
    assert!(image_config.build_args.is_empty());

    let compose_config = DevcontainerResolver::resolve(&fixture_path("compose-mode"))
        .await
        .unwrap();
    assert!(compose_config.build_args.is_empty());
}

/// Verify build_target is None for image-only and compose modes.
#[tokio::test]
async fn build_target_none_in_image_and_compose_modes() {
    let image_config = DevcontainerResolver::resolve(&fixture_path("image-only"))
        .await
        .unwrap();
    assert!(image_config.build_target.is_none());

    let compose_config = DevcontainerResolver::resolve(&fixture_path("compose-mode"))
        .await
        .unwrap();
    assert!(compose_config.build_target.is_none());
}

/// Gap 1: remoteEnv values must NOT appear as ENV directives in the generated Dockerfile.
/// Only containerEnv should be baked in.
#[tokio::test]
async fn remote_env_excluded_from_dockerfile() {
    // image-only fixture has remoteEnv: {"EDITOR": "code"} and containerEnv: {"DEBIAN_FRONTEND": "noninteractive"}
    let config = DevcontainerResolver::resolve(&fixture_path("image-only"))
        .await
        .unwrap();

    // containerEnv IS in the Dockerfile
    assert!(config.dockerfile.contains("ENV DEBIAN_FRONTEND=noninteractive"));

    // remoteEnv is NOT in the Dockerfile
    assert!(!config.dockerfile.contains("EDITOR=code"));

    // remoteEnv IS in the environment HashMap (runtime-only)
    assert_eq!(
        config.environment.get("EDITOR").map(String::as_str),
        Some("code")
    );
}

/// Gap 2: forwardPorts in compose mode are merged with compose service ports, with deduplication.
#[tokio::test]
async fn forward_ports_merged_and_deduped_in_compose() {
    // compose-mode fixture has compose ports [3000, 9229] and forwardPorts [3000, 5173]
    let config = DevcontainerResolver::resolve(&fixture_path("compose-mode"))
        .await
        .unwrap();

    // 3000 appears in both compose ports and forwardPorts — should NOT be duplicated
    assert_eq!(config.forwarded_ports, vec![3000, 9229, 5173]);
}

/// Gap 3: build.target is parsed and exposed in dockerfile mode.
#[tokio::test]
async fn build_target_in_dockerfile_mode() {
    let config = DevcontainerResolver::resolve(&fixture_path("dockerfile-mode"))
        .await
        .unwrap();

    assert_eq!(config.build_target.as_deref(), Some("dev"));
}

/// Gap 4: forwardPorts string formats ("host:container", "port") are parsed correctly.
#[tokio::test]
async fn forward_ports_string_formats() {
    // image-only fixture has forwardPorts: [3000, "8080:80", "9090"]
    let config = DevcontainerResolver::resolve(&fixture_path("image-only"))
        .await
        .unwrap();

    // 3000 is a plain number
    assert!(config.forwarded_ports.contains(&3000));
    // "8080:80" extracts container port 80
    assert!(config.forwarded_ports.contains(&80));
    // "9090" is parsed as a plain port number
    assert!(config.forwarded_ports.contains(&9090));
    // host port 8080 should NOT appear (only container port matters)
    assert!(!config.forwarded_ports.contains(&8080));

    assert_eq!(config.forwarded_ports, vec![3000, 80, 9090]);
}

/// Verify compose_files is empty for non-compose modes.
#[tokio::test]
async fn compose_files_empty_in_non_compose_modes() {
    let image_config = DevcontainerResolver::resolve(&fixture_path("image-only"))
        .await
        .unwrap();
    assert!(image_config.compose_files.is_empty());

    let df_config = DevcontainerResolver::resolve(&fixture_path("dockerfile-mode"))
        .await
        .unwrap();
    assert!(df_config.compose_files.is_empty());
}

/// Verify on_create_commands defaults to empty when not specified.
#[tokio::test]
async fn on_create_commands_empty_when_not_specified() {
    let config = DevcontainerResolver::resolve(&fixture_path("variables"))
        .await
        .unwrap();
    assert!(config.on_create_commands.is_empty());
}

/// Verify container_env defaults to empty when not specified.
#[tokio::test]
async fn container_env_empty_when_not_specified() {
    let config = DevcontainerResolver::resolve(&fixture_path("variables"))
        .await
        .unwrap();
    assert!(config.container_env.is_empty());
}
