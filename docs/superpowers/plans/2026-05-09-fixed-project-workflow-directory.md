# Fixed Project Workflow Directory Plan

**Summary**
Remove the `[project].directory` feature as an effective setting so project workflows are always discovered, created, listed, and resolved from `<repo_root>/.fabro/workflows/*`. Configs that still contain `[project] directory = ...` should continue parsing, but the field is deprecated and ignored. User-level workflows under `~/.fabro/workflows` remain unchanged as an additional fallback/list section.

**Key Changes**
- Remove `directory` from the resolved settings/API shape while tolerating old config files:
  - Keep `ProjectLayer::directory` as a deprecated parse-only field so existing `project.toml` files with the field do not fail schema validation.
  - Delete `ProjectNamespace::directory`.
  - Remove `[project] directory = "."` from built-in defaults.
  - Update project resolver, `fabro-types` fixtures, OpenAPI schema, and generated TypeScript client so resolved `[project]` settings only carry `name`, `description`, and `metadata`.
- Make project Fabro root fixed:
  - Delete `resolve_fabro_root`; it only exists to apply `project.directory`.
  - Delete `load_project_config` and project-root path normalization code if they have no remaining callers after `resolve_fabro_root` is removed.
  - At workflow discovery/create/list call sites, use the discovered config path's parent directory as the Fabro root and append `workflows`.
  - Do not add a new project-config validation step for workflow directory discovery; settings validation remains the responsibility of settings-loading paths.
- Update docs and hints:
  - Remove public references to `project.directory` and replace the old `fabro.root -> project.directory` rename hint with guidance that project workflows now live under `.fabro/workflows`.
  - Add/keep explicit docs that project workflows live at `<repo_root>/.fabro/workflows/<name>/`.

**Test Plan**
- Config tests:
  - Empty settings still resolve project metadata defaults.
  - `[project] directory = "..."` still parses but is ignored in resolved settings.
  - Project workflow root calculation uses the `.fabro` directory containing `project.toml`, regardless of any deprecated `project.directory` value.
- CLI integration tests:
  - Replace custom-root workflow create/list tests with fixed-root assertions.
  - Confirm `fabro workflow create <name>` writes both `.fabro/workflows/<name>/workflow.fabro` and `.fabro/workflows/<name>/workflow.toml`.
  - Confirm named workflow resolution reads `.fabro/workflows/<name>/workflow.toml`.
- API/client checks:
  - `cargo build -p fabro-api`
  - `cd lib/packages/fabro-api-client && bun run generate`
  - `cargo nextest run -p fabro-config -p fabro-api -p fabro-cli`
  - `cd apps/fabro-web && bun run typecheck`

**Assumptions**
- Backwards compatibility is parse-only: old configs with `[project].directory` should not fail, but the value has no effect and is not exposed in resolved settings or the API.
- This change only fixes the project workflow directory. User workflows remain available unless a separate decision removes them later.
- GitHub retrieval itself is out of scope here; this refactor makes the repo path deterministic for that future work.
