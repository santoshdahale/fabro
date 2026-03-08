# Code-to-Doc Mapping

Which source files affect which doc pages. Use this as guidance — also apply judgment for unmapped files that clearly affect user-facing behavior.

| Source | Docs |
|--------|------|
| `crates/arc-cli/src/main.rs`, `crates/arc-workflows/src/cli/mod.rs`, `crates/arc-workflows/src/cli/run.rs` | `docs/reference/cli.mdx` |
| `crates/arc-cli/src/cli_config.rs` | `docs/reference/cli-configuration.mdx` |
| `crates/arc-llm/src/cli.rs` | `docs/reference/cli.mdx` |
| `crates/arc-api/src/serve.rs` | `docs/reference/cli.mdx` |
| `crates/arc-workflows/src/parser/*.rs` | `docs/reference/dot-language.mdx` |
| `crates/arc-workflows/src/condition.rs` | `docs/reference/dot-language.mdx` |
| `crates/arc-workflows/src/cli/validate.rs` | `docs/reference/dot-language.mdx` |
| `crates/arc-workflows/src/stylesheet.rs` | `docs/workflows/stylesheets.mdx` |
| `crates/arc-workflows/src/transform.rs` | `docs/workflows/variables.mdx` |
| `crates/arc-workflows/src/handler/*.rs` | `docs/workflows/stages-and-nodes.mdx`, `docs/reference/dot-language.mdx` |
| `crates/arc-workflows/src/handler/human.rs` | `docs/workflows/human-in-the-loop.mdx` |
| `crates/arc-workflows/src/cli/run_config.rs` | `docs/execution/run-configuration.mdx` |
| `crates/arc-workflows/src/engine.rs` | `docs/core-concepts/how-arc-works.mdx` |
| `crates/arc-workflows/src/context/*.rs` | `docs/execution/context.mdx` |
| `crates/arc-workflows/src/checkpoint.rs` | `docs/execution/checkpoints.mdx` |
| `crates/arc-workflows/src/retro.rs`, `crates/arc-workflows/src/retro_agent.rs` | `docs/execution/retros.mdx` |
| `crates/arc-workflows/src/interviewer/*.rs` | `docs/execution/interviews.mdx` |
| `crates/arc-workflows/src/hook/*.rs` | `docs/agents/hooks.mdx` |
| `crates/arc-workflows/src/daytona_sandbox.rs` | `docs/integrations/daytona.mdx`, `docs/execution/environments.mdx` |
| `crates/arc-agent/src/tools.rs`, `crates/arc-agent/src/tool_registry.rs`, `crates/arc-agent/src/tool_execution.rs` | `docs/agents/tools.mdx` |
| `crates/arc-agent/src/v4a_patch.rs` | `docs/agents/tools.mdx` |
| `crates/arc-agent/src/cli.rs` | `docs/agents/permissions.mdx` |
| `crates/arc-agent/src/subagent.rs` | `docs/agents/subagents.mdx` |
| `crates/arc-agent/src/mcp_integration.rs` | `docs/agents/mcp.mdx` |
| `crates/arc-llm/src/catalog.rs`, `crates/arc-llm/src/providers/*.rs` | `docs/core-concepts/models.mdx` |
| `crates/arc-exe/src/*.rs` | `docs/integrations/exe-dev.mdx`, `docs/execution/environments.mdx` |
| `crates/arc-devcontainer/src/*.rs` | `docs/execution/devcontainers.mdx` |
| `crates/arc-slack/src/*.rs` | `docs/integrations/slack.mdx` |
| `crates/arc-sprites/src/*.rs` | `docs/integrations/sprites.mdx` |
| `crates/arc-mcp/src/*.rs` | `docs/agents/mcp.mdx` |
| `crates/arc-api/src/*.rs` | `docs/api-reference/overview.mdx`, `docs/api-reference/demo-mode.mdx` |
| `crates/arc-api/src/server_config.rs` | `docs/administration/server-configuration.mdx` |
