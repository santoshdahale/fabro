# Integrating DevcontainerConfig with DaytonaSandbox

How to wire the parsed `DevcontainerConfig` into sandbox creation.

## Overview

`DevcontainerResolver::resolve(repo_path)` reads a repository's devcontainer.json and produces a `DevcontainerConfig` containing everything needed to build and configure a sandbox:

```rust
pub struct DevcontainerConfig {
    pub dockerfile: String,            // Generated Dockerfile content
    pub build_context: PathBuf,        // Directory for docker build
    pub build_args: HashMap<String, String>, // docker build --build-arg flags
    pub build_target: Option<String>,        // docker build --target
    pub initialize_commands: Vec<Command>,   // Host-side pre-build commands
    pub on_create_commands: Vec<Command>,    // Container after first creation
    pub post_create_commands: Vec<Command>,  // Container post-creation setup
    pub post_start_commands: Vec<Command>,   // Container on-each-start commands
    pub environment: HashMap<String, String>,    // remoteEnv merged
    pub container_env: HashMap<String, String>,  // containerEnv (also in Dockerfile)
    pub remote_user: Option<String>,   // Non-root user
    pub workspace_folder: String,      // Working directory inside container
    pub forwarded_ports: Vec<u16>,     // Ports to expose
    pub compose_files: Vec<PathBuf>,   // Compose file paths (empty if not compose mode)
    pub compose_service: Option<String>,
}
```

## Mapping Devcontainer to Daytona

### Dockerfile and Image Build

`config.dockerfile` contains the full Dockerfile content (not a path). For image-only configs, this is a single `FROM` line. For Dockerfile configs, it is the file content with feature layers appended.

- Build a Docker image from `config.dockerfile` using `config.build_context` as the build context directory.
- Use this image as the Daytona sandbox snapshot/base image.

### Environment Variables

`config.environment` contains the `remoteEnv` values (with variables already substituted). These are runtime-only environment variables, not baked into the Dockerfile. `config.container_env` contains `containerEnv` values (baked into the generated Dockerfile as `ENV` directives).

- Pass `config.environment` as runtime environment variables when starting the sandbox.
- `containerEnv` values are already in the Dockerfile; `config.container_env` is available for reference.

### Workspace Folder

`config.workspace_folder` defaults to `/workspaces/{repo-name}`.

- Set this as the sandbox working directory.
- Mount or clone the repository into this path.

### Remote User

`config.remote_user` specifies the non-root user for running dev tools.

- Use this as the sandbox exec user when running lifecycle commands and user sessions.
- Falls back to root if not set.

### Forwarded Ports

`config.forwarded_ports` lists ports to expose (first port = default preview).

- Use the first port as the default preview URL for the sandbox.
- Forward all listed ports from the sandbox to the user.

## Docker Compose DinD Flow

When `config.compose_files` is non-empty, the devcontainer uses Docker Compose mode.

### Strategy

Run Docker-in-Docker (DinD) inside the Daytona sandbox:

1. Create a sandbox using the extracted Dockerfile from the compose service.
2. Install Docker daemon inside the sandbox (or use a DinD-capable base image).
3. Copy the compose file and related context into the sandbox.
4. Run `docker compose up` inside the sandbox to start all services.
5. The compose service ports become available on localhost inside the sandbox.
6. Forward those ports from the sandbox to the user.

### Port Forwarding

Ports come from the compose service's `ports` configuration (parsed by `compose::parse_compose`). The compose parser extracts container-side ports from formats like `"8080:80"`, `"3000"`, and `5432`.

## Lifecycle Hook Execution Order

The devcontainer spec defines this execution order:

| Hook | Where | When | `DevcontainerConfig` field |
|---|---|---|---|
| `initializeCommand` | Host | Before build | `initialize_commands` |
| `onCreateCommand` | Container | After first creation | `on_create_commands` |
| `updateContentCommand` | Container | After create/content update | Not captured (not parsed) |
| `postCreateCommand` | Container | After create/content update | `post_create_commands` |
| `postStartCommand` | Container | On each start | `post_start_commands` |
| `postAttachCommand` | Container | On each attach | Not captured (not parsed) |

### Command Types

Each command is represented as a `Command` enum:

```rust
pub enum Command {
    Shell(String),                     // "npm install"
    Args(Vec<String>),                 // ["npm", "install"]
    Parallel(HashMap<String, String>), // {"install": "npm install", "build": "npm run build"}
}
```

- `Shell` -- execute via `sh -c "<command>"`
- `Args` -- execute directly as argv
- `Parallel` -- execute all values concurrently, wait for all to complete

### Execution in Sandbox

```
1. Run initialize_commands on HOST (before sandbox creation)
2. Build image from config.dockerfile (pass config.build_args as --build-arg flags)
3. Create sandbox from image
4. Run on_create_commands in sandbox (as remote_user if set)
5. Run post_create_commands in sandbox (as remote_user if set)
6. Run post_start_commands in sandbox (as remote_user if set)
```

## Example Integration Code

```rust
use arc_devcontainer::{DevcontainerResolver, DevcontainerConfig, Command};

async fn create_sandbox_from_devcontainer(repo_path: &Path) -> Result<Sandbox> {
    let config = DevcontainerResolver::resolve(repo_path).await?;

    // 1. Run host-side init commands
    for cmd in &config.initialize_commands {
        run_host_command(cmd).await?;
    }

    // 2. Build image and create sandbox
    let sandbox = if !config.compose_files.is_empty() {
        // Compose mode: build from extracted service Dockerfile, then run compose inside
        let sandbox = daytona.create_from_dockerfile(
            &config.dockerfile,
            &config.build_context,
        ).await?;
        setup_dind(&sandbox).await?;
        sandbox.exec("docker compose up -d").await?;
        sandbox
    } else {
        // Image/Dockerfile mode: build directly
        daytona.create_from_dockerfile(
            &config.dockerfile,
            &config.build_context,
        ).await?
    };

    // 3. Configure environment
    for (key, value) in &config.environment {
        sandbox.set_env(key, value).await?;
    }

    // 4. Set working directory
    sandbox.set_workdir(&config.workspace_folder).await?;

    // 5. Run lifecycle hooks
    let user = config.remote_user.as_deref();
    for cmd in &config.on_create_commands {
        sandbox.exec_command(cmd, user).await?;
    }
    for cmd in &config.post_create_commands {
        sandbox.exec_command(cmd, user).await?;
    }
    for cmd in &config.post_start_commands {
        sandbox.exec_command(cmd, user).await?;
    }

    // 6. Set up port forwarding
    if let Some(port) = config.forwarded_ports.first() {
        sandbox.set_preview_port(*port).await?;
    }

    Ok(sandbox)
}
```

## Edge Cases and Limitations

- **Features require `oras`**: Feature resolution shells out to `oras` CLI for OCI registry pulls. The resolver attempts auto-install if `oras` is not on PATH.
- **No `updateContentCommand`**: This lifecycle hook is not parsed.
- **No `postAttachCommand`**: Not parsed. Attach-time hooks would need to run on each user session connection.
- **`${containerEnv:VAR}` not supported**: Variable substitution only covers host-side variables. Container-side env vars require a running container.
- **Port forwarding**: Both numeric and string port formats (e.g., `"8080:80"`, `"9090"`) are supported in `forwardPorts`. In compose mode, `forwardPorts` are merged with compose service ports.
