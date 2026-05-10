# Sandbox Capabilities Follow-On Plan

## Summary

After the `SandboxDetails` tab lands, add the interactive sandbox capabilities that are useful without OTLP: terminal, file browser, and browser-controlled VNC.

This is a follow-on PR plan. It deliberately excludes logs, traces, and metrics. It also excludes Daytona Dashboard embedding; Fabro should own the UI and call provider APIs through the Fabro server.

The current `Sandbox` tab already combines sandbox details and terminal access. It uses a two-column layout: the left column shows sandbox information, and the right column currently always shows the terminal. This follow-on changes the right column into a mode surface with a toggle for `Terminal`, `Filesystem`, and `VNC`.

## Scope

- Keep `Terminal` available inside the existing `Sandbox` tab, building on the existing `/runs/{id}/terminal` WebSocket route and xterm UI.
- Add a provider-neutral sandbox file browser as a right-column `Sandbox` tab mode that uses existing run-scoped sandbox file APIs.
- Add Daytona-backed VNC as a right-column `Sandbox` tab mode using a signed preview URL for the sandbox noVNC service.
- Keep provider credentials server-side only.
- Do not add OTLP plumbing, logs, traces, metrics, or provider dashboard iframes.

## Sandbox Tab Layout

The top-level run detail tabs should remain focused. Do not add new top-level `Terminal`, `Filesystem`, or `VNC` tabs for this work. The run page should keep a single sandbox workspace entry:

- `Overview`
- `Stages`
- `Files Changed`
- `Sandbox`
- `Billing`

The `Sandbox` tab requires a run sandbox. Inside the `Sandbox` tab:

- Left column: persistent `SandboxDetails` panel.
- Right column: interactive workspace mode.
- Right-column modes:
  - `Terminal`
  - `Filesystem`
  - `VNC`

The right-column mode control should be a segmented control or tablist local to the `Sandbox` page, not top-level app navigation. `Terminal` should remain the default mode.

Routing options:

- Preferred simple route: store the active mode in query state, e.g. `/runs/:id/sandbox?mode=filesystem`.
- Acceptable alternative: nested sandbox routes, e.g. `/runs/:id/sandbox/filesystem`, if React Router integration is cleaner.

Keep `/runs/:id/files` reserved for the existing diff-oriented `Files Changed` tab.

## Terminal

The embedded terminal already exists in the codebase:

- Frontend: `apps/fabro-web/app/routes/run-terminal.tsx`
- Server: `GET /api/v1/runs/{id}/terminal`
- Sandbox adapter: `lib/crates/fabro-sandbox/src/terminal.rs`

Follow-on work should move or adapt the existing terminal UI into the right column of `apps/fabro-web/app/routes/run-sandbox.tsx` unless implementation shows a cleaner shared-component extraction.

- Preserve the existing terminal transport and WebSocket protocol.
- Keep the terminal gated by run sandbox presence through the containing `Sandbox` route.
- Align error/empty states with the new sandbox capability pages.
- Do not move terminal transport into OpenAPI; the WebSocket protocol remains hand-authored.
- Remove any now-redundant top-level `Terminal` tab/route only if the current implementation still exposes one after `Sandbox` absorbed it.

## Filesystem

Build a true sandbox filesystem browser in the right column of the `Sandbox` tab, separate from `Files Changed`.

Existing server/API pieces:

- `GET /api/v1/runs/{id}/sandbox/files`
- `GET /api/v1/runs/{id}/sandbox/file`
- `PUT /api/v1/runs/{id}/sandbox/file`
- Types: `SandboxFileEntry`, `SandboxFileListResponse`
- Server handler: `lib/crates/fabro-server/src/server/handler/sandbox.rs`

Frontend work:

- Prefer a right-column component such as `apps/fabro-web/app/routes/run-sandbox/filesystem-panel.tsx` or a nearby component under the existing sandbox route.
- Add generated-client query/mutation hooks in `apps/fabro-web/app/lib/queries.ts` or a focused route-local data layer if the interactions are too stateful for generic hooks.
- The filesystem mode itself can use a two-pane layout within the right column:
  - Left side of the mode: directory tree/list with refresh and path navigation.
  - Right side of the mode: file preview or empty state.
- First version capabilities:
  - Browse directories.
  - Preview text files with a size cap.
  - Download files through the existing file endpoint.
  - Upload/replace a file through the existing `PUT` endpoint.
- Defer destructive and complex editing actions unless needed immediately:
  - Delete
  - Rename/move
  - chmod/chown
  - search/replace

Server/API follow-up if needed:

- Add file metadata fields if the current `SandboxFileEntry` is too thin for the UI.
- Add explicit directory creation/delete/move endpoints only when the UI implements those actions.
- Keep path validation and provider access server-side.

## VNC

Add browser-controlled desktop access for Daytona sandboxes with VNC support.

Documented Daytona path:

- Start/manage VNC through Daytona Computer Use / VNC support.
- Expose browser noVNC through a signed Daytona preview URL.
- Use signed preview URLs for iframes because standard preview URLs require headers the browser iframe cannot set.

Backend endpoint:

- `POST /api/v1/runs/{id}/sandbox/vnc`

Response:

- `url: String`
- `expires_at: Option<DateTime<Utc>>`
- `provider: String`
- `port: u16`

Behavior:

- Load the run sandbox record.
- Only Daytona is supported in the first VNC PR.
- Reconnect to the Daytona sandbox.
- Start or verify Computer Use / VNC processes.
- Generate a signed preview URL for the configured noVNC port.
- Return `409` if the sandbox is not ready or VNC startup fails.
- Return `501` for Docker/local until there is a concrete provider-backed VNC story.

Configuration:

- Add a server/run setting for the Daytona noVNC port only if the port is not already stable in our images.
- Default to the image contract used by Fabro's VNC-capable snapshots.
- Keep the value provider-specific; do not pretend all providers have the same VNC port.

Frontend route:

- Add a right-column VNC component near the existing sandbox route, e.g. `apps/fabro-web/app/routes/run-sandbox/vnc-panel.tsx`.
- When the VNC mode is selected, call the VNC endpoint and render an iframe:
  - `src` is the signed preview URL.
  - `allow` includes clipboard and fullscreen permissions.
  - Provide refresh/reconnect action.
  - Show unsupported-provider, startup-failed, and expired-link states.
- Do not expose Daytona API keys, preview tokens, or provider connection metadata outside the signed URL.

Security notes:

- Treat the VNC iframe as remote desktop control of the sandbox.
- Keep it same run-auth gated as terminal and filesystem.
- Use signed preview URLs with short TTLs.
- Avoid embedding arbitrary provider dashboard URLs.

## API And Type Placement

- Add OpenAPI schemas only for HTTP endpoints:
  - VNC response/request types.
  - Any new filesystem metadata or mutation types.
- Do not put WebSocket terminal protocol into OpenAPI.
- Put reusable product/API DTOs in `fabro-types` only when they become shared vocabulary. Route-local response types can remain generated API types if there is no internal semantic owner.
- Regenerate both Rust and TypeScript clients for any OpenAPI changes:
  - `cargo build -p fabro-api`
  - `cd lib/packages/fabro-api-client && bun run generate`

## Test Plan

- Terminal:
  - Existing `run-terminal` tests continue to pass.
  - Sandbox route tests confirm `Terminal` is the default right-column mode.
  - If a top-level `Terminal` route/tab is removed, update or delete tests that asserted it as separate navigation.
- Filesystem server tests:
  - Existing list/download/upload tests remain green.
  - Add route tests for any new metadata or mutation endpoints.
  - Missing sandbox and unsupported provider states render as API errors, not panics.
- Filesystem frontend tests:
  - Selecting `Filesystem` switches only the right column and leaves sandbox details visible in the left column.
  - Browse root directory and nested directory.
  - Selecting a file requests file contents and renders a preview.
  - Large/binary file states do not try to render unsafe content inline.
  - Upload success refreshes the current directory.
- VNC server tests:
  - Missing run/sandbox returns `404`.
  - Docker/local VNC returns `501`.
  - Daytona path starts or verifies VNC and requests a signed preview URL for the configured port.
  - Daytona startup or preview failure returns `409`.
- VNC frontend tests:
  - Selecting `VNC` switches only the right column and leaves sandbox details visible in the left column.
  - Loading state while the signed URL is requested.
  - Successful response renders an iframe with the returned URL.
  - Unsupported and failure responses render actionable empty states.
  - Refresh action requests a fresh signed URL.
- Manual acceptance:
  - Daytona run with VNC-capable image: select the `VNC` mode inside the `Sandbox` tab, interact with desktop, type into a terminal/browser inside the desktop, refresh Fabro page, reconnect.
  - Daytona run filesystem: browse `/workspace`, preview a text file, upload a file, verify it appears from the terminal.
  - Confirm no Daytona secret appears in browser devtools except the intended short-lived signed preview URL.

## Assumptions

- Fabro-owned Daytona images already include VNC/noVNC support.
- The noVNC port is either stable by image contract or configurable before the VNC PR lands.
- The existing terminal implementation is the baseline; this follow-on PR should adapt its UI placement rather than replace its transport.
- Filesystem UI can ship incrementally with browse/preview/download/upload before destructive operations.
- Logs, traces, and metrics remain out of scope because we are avoiding OTLP plumbing.
