# Remove Local Worktree Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `sandbox` the only user-facing workspace selector by removing local worktree mode and `--in-place`.

**Architecture:** `local` becomes direct execution in the resolved working directory, still passing through the normal generic sandbox safety wrappers. Docker and Daytona remain Fabro-managed clone sandboxes. `WorktreeSandbox` stays as internal machinery for parallel-node isolation only.

**Tech Stack:** Rust workspace (`fabro-cli`, `fabro-config`, `fabro-types`, `fabro-workflow`, `fabro-server`, `fabro-api`), OpenAPI/progenitor, generated TypeScript API client, React web UI.

---

## Summary

This is a greenfield breaking change. Do not preserve compatibility for old configs, CLI flags, or API fields.

The target model is:

- `--sandbox local`: run directly in the resolved working directory.
- `--sandbox docker`: create a Docker workspace and clone GitHub origin when available.
- `--sandbox daytona`: create a Daytona workspace and clone GitHub origin when available.
- Local isolation is user-managed: create or enter a clone/worktree manually, then run `fabro run --sandbox local`.
- Parallel nodes may still create internal git worktrees.

Out of scope: new explicit branching/committing settings. Local direct runs should not get Fabro-managed branch checkout, git checkpoint push, or auto-PR behavior in this PR.

## Key Changes

- Remove `--in-place` from CLI args, manifest argument generation, help snapshots, and docs.
- Remove `[run.sandbox.local]`, `LocalSandboxSettings`, `worktree_mode`, and `WorktreeMode` from defaults, config layers, resolved settings, OpenAPI, generated Rust API types, generated TypeScript API client, docs, and web UI references. Do not leave an empty local settings object behind.
- Remove `in_place` from `RunSpec`, `RunCreated`, run summaries/list items, OpenAPI, generated clients, CLI list output, and web UI code.
- Delete the local-run worktree planning path in workflow initialization: no `resolve_worktree_plan`, no local `WorktreeSandbox` wrapper, and no worktree-skipped notice.
- Keep `WorktreeSandbox` and `WorktreeOptions` for parallel branch execution only.
- Keep the post-initialization sandbox git setup path for clone-based providers. Docker and Daytona should still create/push `fabro/run/{run_id}` branches as they do today.
- Remove the local-run `WorktreeSandbox` wrapper, but keep generic wrappers such as `ReadBeforeWriteSandbox` unless compile/test feedback proves they are no longer needed. Because `LocalSandbox::setup_git` is the default no-op, local runs should have no Fabro-created `run_branch`, no git checkpoint commits, no branch push, and no auto-PR until future explicit git settings are added.
- Extract provider-specific dirty-worktree warning logic before removing worktree planning. For Docker/Daytona, warn that uncommitted local changes are not included in the remote sandbox. For local, do not warn that dirty changes are excluded.
- Treat file lists in this plan as starting points, not exhaustive ownership. After each task, run the acceptance grep for `worktree_mode`, `WorktreeMode`, `--in-place`, and `in_place`, then remove all non-historical product references found in nearby files.

## Implementation Tasks

### Task 1: Remove Public Config and CLI Knobs

**Files:**
- Modify: `lib/crates/fabro-types/src/settings/run.rs`
- Modify: `lib/crates/fabro-config/src/defaults.toml`
- Modify: `lib/crates/fabro-config/src/layers/run.rs`
- Modify: `lib/crates/fabro-config/src/resolve/run.rs`
- Modify: `lib/crates/fabro-cli/src/args.rs`
- Modify: `lib/crates/fabro-cli/src/manifest_builder.rs`

- [ ] Remove `WorktreeMode`, `LocalSandboxSettings`, and the `RunSandboxSettings.local` field.
- [ ] Remove the `[run.sandbox.local] worktree_mode = "always"` default.
- [ ] Remove config-layer parsing/resolution for `[run.sandbox.local]` entirely.
- [ ] Remove `RunArgs::in_place`.
- [ ] Make `run_manifest_args` set `sandbox` only from `--sandbox`; do not synthesize local sandbox or `worktree_mode`.
- [ ] Make `preflight_manifest_args` stop carrying `worktree_mode`.
- [ ] Update compile errors in config tests and CLI tests by removing assertions that mention `WorktreeMode`, `worktree_mode`, or `--in-place`.
- [ ] Run `rg -n "worktree_mode|WorktreeMode|--in-place|in_place" lib apps/fabro-web/app docs/public -g '!lib/packages/fabro-api-client/src/**' -g '!lib/crates/fabro-spa/assets/**'` and remove newly exposed non-historical matches related to config and CLI knobs.

### Task 2: Remove `in_place` From Run State and API Surfaces

**Files:**
- Modify: `lib/crates/fabro-types/src/run.rs`
- Modify: `lib/crates/fabro-types/src/run_event/run.rs`
- Modify: `lib/crates/fabro-types/src/run_summary.rs`
- Modify: `lib/crates/fabro-workflow/src/operations/create.rs`
- Modify: `lib/crates/fabro-workflow/src/run_lookup.rs`
- Modify: `lib/crates/fabro-server/src/run_manifest.rs`
- Modify: `docs/public/api-reference/fabro-api.yaml`

- [ ] Remove `in_place` from `RunSpec` and all run-created/run-summary event props.
- [ ] Remove `PreparedManifest.in_place`; local/direct behavior is now implied by `settings.run.sandbox.provider == "local"`.
- [ ] Remove `CreateRunInput.in_place` and `PersistCreateOptions.in_place`.
- [ ] Remove server and CLI code that serializes, displays, or filters by `in_place`.
- [ ] Remove `in_place` from the OpenAPI schema.
- [ ] Regenerate Rust API types with `cargo build -p fabro-api`.
- [ ] Regenerate TypeScript client with `cd lib/packages/fabro-api-client && bun run generate`.
- [ ] Run `rg -n "worktree_mode|WorktreeMode|--in-place|in_place" lib apps/fabro-web/app docs/public -g '!lib/packages/fabro-api-client/src/**' -g '!lib/crates/fabro-spa/assets/**'` and remove newly exposed non-historical matches in event conversion, persistence, runtime store/test fixtures, run metadata, billing rollups, CLI run listing/server run wrappers, server demo data, and generated-schema call sites.

### Task 3: Simplify Workflow Initialization

**Files:**
- Modify: `lib/crates/fabro-workflow/src/pipeline/initialize.rs`
- Modify: `lib/crates/fabro-workflow/src/pipeline/types.rs`
- Modify: `lib/crates/fabro-workflow/src/operations/start.rs`
- Modify: `lib/crates/fabro-workflow/src/operations/fork.rs`

- [ ] Extract provider-specific dirty-worktree warning logic before deleting worktree planning: Docker/Daytona warn that uncommitted local changes are not included in the remote sandbox; local direct runs do not warn.
- [ ] Remove `worktree_mode` from initialization options.
- [ ] Delete `WorktreePlan`, `resolve_worktree_plan`, `resolve_worktree_base_sha`, and `worktree_skipped_notice`.
- [ ] Remove local `WorktreeSandbox` wrapping from `initialize`.
- [ ] Keep existing generic wrappers such as `ReadBeforeWriteSandbox`; this change only removes Fabro-managed git worktree materialization for local runs.
- [ ] Keep attach/resume reconnection for existing sandboxes.
- [ ] Keep normal sandbox build/initialize flow for new runs.
- [ ] Keep the generic `sandbox.setup_git(...)` block after sandbox initialization. This will continue to work for Docker/Daytona and continue to no-op for local.
- [ ] Remove fork validation that rejects `spec.in_place`; fork should now fail based on real missing prerequisites such as empty/missing checkpoint git SHA or missing repo origin.
- [ ] Run `rg -n "worktree_mode|WorktreeMode|--in-place|in_place" lib apps/fabro-web/app docs/public -g '!lib/packages/fabro-api-client/src/**' -g '!lib/crates/fabro-spa/assets/**'` and remove newly exposed non-historical matches related to initialize, fork, and resume.

### Task 4: Keep Parallel Worktrees Intact

**Files:**
- Modify only if compile errors require it: `lib/crates/fabro-workflow/src/handler/parallel.rs`
- Do not delete: `lib/crates/fabro-sandbox/src/worktree.rs`

- [ ] Verify `WorktreeSandbox` remains available to parallel-node code.
- [ ] Remove only imports that were used exclusively by local-run worktree setup.
- [ ] Ensure parallel branch tests still create isolated branch worktrees and fan back in as before.
- [ ] Run `rg -n "worktree_mode|WorktreeMode|--in-place|in_place" lib apps/fabro-web/app docs/public -g '!lib/packages/fabro-api-client/src/**' -g '!lib/crates/fabro-spa/assets/**'` and confirm remaining worktree references are only parallel-node internals or historical docs intentionally kept.

### Task 5: Update Docs and UI

**Files:**
- Modify: `docs/public/execution/run-configuration.mdx`
- Modify: `docs/public/reference/cli.mdx`
- Modify: `apps/fabro-web/app/routes/run-settings.tsx`
- Modify: `apps/fabro-web/app/routes/workflow-detail.tsx`

- [ ] Document local sandbox semantics as direct execution in the resolved working directory.
- [ ] Remove `--in-place` from CLI reference docs.
- [ ] Remove `[run.sandbox.local] worktree_mode` docs.
- [ ] Remove UI rendering/seed data references to `worktree_mode`.
- [ ] Keep docs clear that user-managed local isolation is done by entering a separate clone/worktree and running with `--sandbox local`.
- [ ] Run `rg -n "worktree_mode|WorktreeMode|--in-place|in_place" lib apps/fabro-web/app docs/public -g '!lib/packages/fabro-api-client/src/**' -g '!lib/crates/fabro-spa/assets/**'` and remove newly exposed non-historical docs/UI matches.

## Test Plan

- [ ] Run config/default tests:

```bash
cargo nextest run -p fabro-config
```

- [ ] Run CLI manifest and command help tests:

```bash
cargo nextest run -p fabro-cli
```

- [ ] Run workflow tests covering initialize, fork, checkpointing, parallel worktrees, and PR prerequisites:

```bash
cargo nextest run -p fabro-workflow
```

- [ ] Run server API tests for manifests, run summaries, PR endpoints, and OpenAPI conformance:

```bash
cargo nextest run -p fabro-server
```

- [ ] Regenerate and verify API clients:

```bash
cargo build -p fabro-api
cd lib/packages/fabro-api-client && bun run generate
```

- [ ] Typecheck web UI after generated-client and settings-shape changes:

```bash
cd apps/fabro-web && bun run typecheck
```

- [ ] Run formatting and lint checks:

```bash
cargo +nightly-2026-04-14 fmt --check --all
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

## Acceptance Criteria

- This product-source grep has no non-historical matches:

```bash
rg -n "worktree_mode|WorktreeMode|--in-place|in_place" lib apps/fabro-web/app docs/public \
  -g '!lib/packages/fabro-api-client/src/**' \
  -g '!lib/crates/fabro-spa/assets/**'
```

- Generated clients are checked separately after regeneration; `lib/packages/fabro-api-client/src` must not expose `worktree_mode`, `WorktreeMode`, or `in_place`.
- Historical changelog matches may remain only when intentionally kept.
- `fabro run --sandbox local ...` runs in the resolved working directory without creating a run-scoped worktree.
- Docker and Daytona runs still clone and checkout `fabro/run/{run_id}`.
- Local runs do not get a Fabro-created `run_branch`, do not auto-push a run branch, and do not auto-create PRs.
- Parallel-node worktree behavior remains unchanged.
- Generated Rust and TypeScript API clients no longer expose `worktree_mode` or `in_place`.
