# Canonical RunProjection Simplification

## Summary

Refactor `RunProjection` into a stricter canonical run-state type with no compatibility shims. Because there are no production deployments, make the breaking JSON/API changes directly and remove old field names, tuple encodings, default-empty projection states, and generated/client traces.

The end state: a `RunProjection` always represents a real run initialized from `run.created`; it has required `spec`, `status`, `status_updated_at`, and `last_event_at`; checkpoint history is named records; graph source lives in `RunSpec`; terminal diff lives in `Conclusion`; checkpoint diff lives on checkpoint records.

## Key Type And API Changes

- Change `RunProjection`:
  - `spec: RunSpec`, not `Option<RunSpec>`.
  - `status: RunStatus`, not `Option<RunStatus>`.
  - `status_updated_at: DateTime<Utc>` and `last_event_at: DateTime<Utc>`, not optional.
  - Remove top-level `graph_source`, `checkpoint`, `final_patch`, and `diff_summary`.
  - Keep `start`, `sandbox`, `pending_control`, `conclusion`, `pull_request`, and `superseded_by` optional because those are genuinely conditional lifecycle facts.
- Change `RunSpec`:
  - Add `graph_source: Option<String>`.
  - Keep existing optional git/provenance/blob/fork fields as optional.
- Change `StartRecord`:
  - Remove redundant `run_id`; keep `start_time`, `run_branch`, `base_sha`.
- Add canonical types:
  - `CheckpointRecord { seq: u32, checkpoint: Checkpoint, diff: RunDiff }`.
  - `RunDiff { patch: Option<String>, summary: Option<DiffSummary> }`.
- Change `RunProjection.checkpoints`:
  - From `Vec<(u32, Checkpoint)>` to `Vec<CheckpointRecord>`.
  - Replace `current_checkpoint()` with `checkpoints.last().map(|record| &record.checkpoint)`.
- Change `Conclusion`:
  - Add `diff: RunDiff`.
  - Store terminal `final_patch` as `conclusion.diff.patch`.
  - Store terminal `diff_summary` as `conclusion.diff.summary`.
- Change `PendingInterviewRecord.started_at` to required `DateTime<Utc>`.
- Change `StageProjection.state` to required `StageState`; initialize new stage projections as `Running` unless immediately set to a terminal/skipped/retrying state.

## Implementation Changes

- Rework projection construction:
  - Remove `Default` from `RunProjection`.
  - Replace `RunProjection::default() + apply_event` with initialization from the first `run.created` event.
  - `apply_events([])` should error at the reducer level; store/cache code may still return `None` when a run has no events.
  - The first valid projection state is `Submitted`, with both timestamps set to the `run.created` timestamp.
  - `run.submitted` only attaches `definition_blob`; it should not be needed to make the projection valid.
- Rework incremental projection caches:
  - Store projection cache state as `Option<RunProjection>` until `run.created` arrives.
  - Applying any non-`run.created` event before initialization is an invalid event error.
- Update reducer mappings:
  - `run.created.workflow_source` -> `projection.spec.graph_source`.
  - `run.started` -> `StartRecord { start_time, run_branch, base_sha }`.
  - `checkpoint.completed` -> push `CheckpointRecord { seq, checkpoint, diff }`.
  - `run.completed` / `run.failed` -> set `conclusion.diff`.
  - Checkpoint diff summaries should be sourced from `checkpoints.last().diff.summary`; terminal summaries from `conclusion.diff.summary`.
- Keep `RunSummary.diff_summary` as a summary/list convenience field, derived from terminal conclusion diff when present, otherwise latest checkpoint diff. Do not reintroduce a top-level projection diff field.
- Update OpenAPI and generated clients:
  - Update `RunProjection`, `RunSpec`, `Conclusion`, `PendingInterviewRecord`, `StageProjection`.
  - Add `CheckpointRecord` and `RunDiff`.
  - Remove tuple checkpoint schema and generated `run-projection-checkpoints-inner-inner.ts`.
  - Regenerate Rust API and TypeScript client after schema changes.

## Cleanup: Leave No Trace

- Remove all code references to:
  - `RunProjection::default()`.
  - `projection.graph_source`.
  - `projection.checkpoint`.
  - `projection.final_patch`.
  - `projection.diff_summary`.
  - `state.status.unwrap_or(...)`.
  - tuple checkpoint destructuring like `(seq, checkpoint)`.
- Remove obsolete tests and snapshots that assert old raw projection JSON with `graph_source`, `checkpoint`, `final_patch`, nullable `spec`, nullable `status`, or tuple checkpoints.
- Remove compatibility aliases, legacy deserializers, serde aliases, and migration logic for the old projection shape.
- Remove stale OpenAPI schemas and generated TypeScript models produced solely by the old tuple/nullable shape.
- Update comments and docs that refer to `RunProjection.final_patch`; use `conclusion.diff.patch`.
- Update dump/export code so `run.json` uses the new canonical projection, graph source is read from `spec.graph_source`, and checkpoint dump entries iterate `CheckpointRecord`.

## Test Plan

- Rust type/API parity:
  - Update `fabro-api` round-trip tests for `RunProjection`, `StageProjection`, `PendingInterviewRecord`, and add tests for `CheckpointRecord` and `RunDiff`.
  - Ensure `fabro-api` generated types still reuse canonical `fabro_types` replacements.
- Reducer behavior:
  - Projection initializes only from `run.created`.
  - Empty/missing-created event sequences fail clearly.
  - Required timestamps/status/spec are always present after initialization.
  - Checkpoint history serializes as named records and `current_checkpoint()` derives from the last record.
  - Terminal events populate `conclusion.diff`.
  - Checkpoint events populate `CheckpointRecord.diff`.
  - `RunSummary.diff_summary` derives from terminal diff first, latest checkpoint diff otherwise.
- Server/API behavior:
  - `/api/v1/runs/{id}/state` returns the new non-null canonical shape.
  - `/api/v1/runs/{id}/checkpoint` still returns latest checkpoint or null.
  - start/resume checks use derived current checkpoint.
  - file fallback uses `conclusion.diff.patch`.
- Frontend/CLI behavior:
  - Update consumers of generated `RunProjection`.
  - CLI inspect/dump/rewind/fork tests use derived current checkpoint and new diff location.
  - Web run detail still shows diff summary via `RunSummary.diff_summary`.

## Verification

- Run `cargo build -p fabro-api` after OpenAPI changes.
- Regenerate TypeScript API client.
- Run focused tests for `fabro-types`, `fabro-store`, `fabro-api`, `fabro-server`, `fabro-cli`, and `apps/fabro-web`.
- Run `cargo nextest run --workspace`.
- Run `cd apps/fabro-web && bun test && bun run typecheck`.
- Finish with searches proving no old traces remain: `graph_source` only under `RunSpec`, no `RunProjection::default`, no `projection.checkpoint`, no `projection.final_patch`, no tuple checkpoint generated model.
