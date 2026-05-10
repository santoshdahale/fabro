# Sandbox Details Tab Plan

## Summary

Add a provider-neutral `SandboxDetails` API model and a new `Sandbox` tab on the run detail page before `Terminal`.

The tab shows live sandbox identity, state, placement, image/snapshot, resources, labels, and timestamps for the run-owned sandbox. It does not add lifecycle controls, terminal/VNC/file-browser capabilities, metrics, traces, or logs. Those are separate follow-on work.

## Scope

- Add `GET /api/v1/runs/{id}/sandbox` returning `SandboxDetails`.
- Source live details from the sandbox provider where possible:
  - Daytona: use the existing Daytona reconnect path and SDK sandbox data.
  - Docker: inspect the managed container through Docker/Bollard.
  - Local: return a minimal unsupported/local detail only if the run has a local sandbox record; otherwise keep missing sandbox as `404`.
- Add a `Sandbox` tab at `/runs/:id/sandbox`, gated by the same run sandbox presence check as `Terminal`.
- Show the data in a compact panel layout consistent with the run detail UI.
- Ignore lifecycle settings by design: no auto-stop, auto-archive, auto-delete rows.

## API Model

Create the canonical Rust model in `lib/crates/fabro-types/src/sandbox_details.rs` and re-export it from `lib/crates/fabro-types/src/lib.rs`.

`SandboxDetails`:

- `provider: String`
- `name: Option<String>`
- `id: Option<String>`
- `state: SandboxState`
- `native_state: Option<String>`
- `region: Option<String>`
- `image: Option<String>`
- `resources: SandboxResources`
- `labels: BTreeMap<String, String>`
- `timestamps: SandboxTimestamps`

`SandboxResources`:

- `cpu_cores: Option<f64>`
- `memory_bytes: Option<u64>`
- `disk_bytes: Option<u64>`

`SandboxTimestamps`:

- `created_at: Option<DateTime<Utc>>`
- `last_activity_at: Option<DateTime<Utc>>`

`SandboxState` should be the UI/control-plane normalized state:

- `unknown`
- `provisioning`
- `starting`
- `running`
- `stopping`
- `stopped`
- `paused`
- `deleting`
- `deleted`
- `archived`
- `restoring`
- `resizing`
- `error`

Keep `native_state` so provider-specific truth is visible and debugging does not require expanding the normalized enum forever.

## Provider Mapping

Add a sandbox-details inspection function in `fabro-sandbox`, not on the existing `Sandbox` trait unless implementation proves the trait boundary is the simplest fit. A focused free function keeps this as control-plane inspection instead of execution behavior.

Suggested location:

- `lib/crates/fabro-sandbox/src/details.rs`
- exported from `lib/crates/fabro-sandbox/src/lib.rs`

Suggested interface:

- `sandbox_details(record: &SandboxRecord, daytona_api_key: Option<String>, daytona_organization_id: Option<String>, run_id: Option<RunId>) -> Result<SandboxDetails>`

Provider behavior:

- Daytona:
  - Reuse `DaytonaSandbox::reconnect(...)` or a lower-level SDK client helper.
  - Map Daytona `name` to `name`, UUID/id to `id` if available, `snapshot` to `image`, target/region to `region`, SDK resources to `resources`, SDK labels to `labels`, and SDK timestamps to `timestamps`.
  - Normalize Daytona states into `SandboxState` and preserve the original state string in `native_state`.
- Docker:
  - Reuse `DockerSandbox::reconnect(...)` and inspect the container with Bollard.
  - Map Docker container name to `name`, container id to `id`, image to `image`, labels to `labels`, `created` to `timestamps.created_at`, and state status to `native_state`.
  - Compute CPU cores from `HostConfig.cpu_quota / cpu_period` when present; otherwise leave null.
  - Use memory limit when present and non-zero; leave disk null unless Docker inspect provides a reliable configured limit.
  - Set `region` to null; the UI renders Docker as local.
- Local:
  - Return `provider = "local"`, `state = "running"` or `unknown`, `name = null`, `id = null`, `region = null`, `image = null`, empty labels, null resources/timestamps.

## Server Changes

- Update `docs/public/api-reference/fabro-api.yaml`:
  - Add `GET /api/v1/runs/{id}/sandbox`.
  - Add schemas for `SandboxDetails`, `SandboxState`, `SandboxResources`, and `SandboxTimestamps`.
- Update `lib/crates/fabro-api/build.rs` with `with_replacement(...)` entries for the new `fabro-types` types.
- Add a type identity / JSON parity test under `lib/crates/fabro-api/tests/`, following `run_summary_round_trip.rs`.
- Add the handler to `lib/crates/fabro-server/src/server/handler/sandbox.rs`.
- Reuse `load_run_sandbox_record(...)` and current auth behavior from the existing sandbox routes.
- Return:
  - `404` when the run or sandbox record is missing.
  - `409` when the provider exists but inspection fails because the sandbox/container is gone or inaccessible.
  - `501` only for a provider that has a sandbox record but no details implementation.

## Frontend Changes

- Regenerate `lib/packages/fabro-api-client`.
- Add the generated API surface to `apps/fabro-web/app/lib/api-client.ts` if it lands on a new generated API class, or use the existing class if the operation groups with `HumanInTheLoopApi`.
- Add a SWR hook in `apps/fabro-web/app/lib/queries.ts`, with a stable key in `apps/fabro-web/app/lib/query-keys.ts`.
- Add route:
  - `apps/fabro-web/app/routes/run-sandbox.tsx`
  - router entry in `apps/fabro-web/app/router.tsx`: `route("sandbox", RunSandbox)`
- Update `apps/fabro-web/app/routes/run-detail.tsx`:
  - Add `{ name: "Sandbox", path: "/sandbox", requiresSandbox: true }`.
  - Place it before `Terminal`.
- UI layout:
  - Status strip: provider, normalized state, native state when different.
  - Overview panel: name, id, region/local, image.
  - Resources panel: CPU, memory, disk, with unavailable values rendered as muted em dashes.
  - Labels panel: key/value rows; empty state when none.
  - Timestamps panel: created at and last activity, nullable.
- Use existing run detail spacing, panel borders, text colors, and tab styles. Do not introduce a new design system primitive for this.

## Test Plan

- Rust unit tests:
  - Daytona state normalization covers representative active, provisioning, stopped/archived, and error states.
  - Docker state normalization covers `created`, `running`, `paused`, `restarting`, `removing`, `exited`, and `dead`.
  - Docker resource conversion handles quota/period, missing quota, zero memory, and configured memory.
  - `SandboxDetails` serializes with snake_case fields and nullable optional fields.
- Server tests:
  - Missing run returns `404`.
  - Run without sandbox record returns `404`.
  - Docker sandbox record calls Docker details adapter and returns normalized fields.
  - Daytona sandbox record calls Daytona details adapter and returns normalized fields.
  - Provider inspection failure returns `409` with a useful message.
- API tests:
  - Add `fabro-api` replacement parity test proving generated `SandboxDetails` is the shared `fabro_types::SandboxDetails`.
  - Run `cargo build -p fabro-api`.
- Frontend tests:
  - `run-detail.test.ts` verifies the `Sandbox` tab appears for sandbox-backed runs, is before `Terminal`, and is hidden when no sandbox is present.
  - `run-sandbox` route renders overview/resources/labels/timestamps and handles null fields without layout breakage.
  - Query-key test covers the new sandbox details key.
- Verification:
  - `cargo nextest run -p fabro-server`
  - `cargo nextest run -p fabro-api`
  - relevant `fabro-sandbox` tests
  - `cd lib/packages/fabro-api-client && bun run generate`
  - `cd apps/fabro-web && bun test && bun run typecheck`

## Assumptions

- `SandboxDetails` belongs in `fabro-types` because it is shared product vocabulary and should be reused by `fabro-api`.
- The existing persisted `SandboxRecord` remains the minimal reconnect record; do not overload it with live provider details.
- Docker disk size is nullable until there is a reliable configured/container-specific limit.
- `native_state` is for display/debugging only; UI behavior keys off normalized `state`.
- Lifecycle settings and actions are intentionally out of scope for this PR.

