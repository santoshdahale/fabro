# Arc API Design — Draft

Comprehensive API to support all data needs of `arc-web`.

## Current state

**Existing Rust API** (`crates/arc-api/src/server.rs`) exposes 10 endpoints under `/pipelines`.
The OpenAPI spec (`openapi/arc-api.yaml`) uses `/runs`. The router needs to be renamed to match.

**arc-web** uses zero real API calls today. Every page renders hardcoded mock data from:
- `app/data/runs.ts` — 10 runs in 4 kanban columns
- `app/data/retros.ts` — 5 retros with full stage/learning/friction data
- `app/data/verifications.ts` — 8 categories, 30 controls, performance metrics, recent results
- `routes/workflow-detail.tsx` — 4 workflow definitions (config TOML + graph DOT)
- `routes/run-stages.tsx` — hardcoded conversation turns (system/assistant/tool)
- `routes/run-files-changed.tsx` — hardcoded file diffs + checkpoints
- `routes/run-usage.tsx` — hardcoded token/cost usage per stage
- `routes/start.tsx` — hardcoded projects, branches, session history
- `routes/session-detail.tsx` — 3 sessions with chat turns
- `routes/insights.tsx` — saved SQL queries + history
- `routes/settings.tsx` — 5 setting groups with fields

---

## Endpoint inventory

### 1. Runs

Already partially exists. Needs enrichment to carry the data the UI actually renders.

| Method | Path | Description | Status |
|--------|------|-------------|--------|
| `GET` | `/runs` | List runs (board view data) | **extend** |
| `POST` | `/runs` | Start a new run | exists |
| `GET` | `/runs/{id}` | Full run detail | **extend** |
| `POST` | `/runs/{id}/cancel` | Cancel a running run | exists |
| `GET` | `/runs/{id}/events` | SSE event stream | exists |
| `GET` | `/runs/{id}/questions` | Pending questions | exists |
| `POST` | `/runs/{id}/questions/{qid}/answer` | Submit answer | exists |
| `GET` | `/runs/{id}/checkpoint` | Checkpoint data | exists |
| `GET` | `/runs/{id}/context` | Context key-value map | exists |
| `GET` | `/runs/{id}/graph` | Workflow graph SVG | exists |
| `GET` | `/runs/{id}/retro` | Retrospective | exists |
| `GET` | `/runs/{id}/stages` | List stages with status/duration | **new** |
| `GET` | `/runs/{id}/stages/{stageId}/turns` | Conversation transcript for a stage | **new** |
| `GET` | `/runs/{id}/files` | File diffs grouped by checkpoint | **new** |
| `GET` | `/runs/{id}/usage` | Token/cost breakdown by stage + model | **new** |
| `GET` | `/runs/{id}/verifications` | Verification results for this run | **new** |
| `GET` | `/runs/{id}/configuration` | Run configuration (TOML) | **new** |
| `POST` | `/runs/{id}/steer` | Submit steering guidance on a file line | **new** |

#### `GET /runs` response shape

```jsonc
[
  {
    "id": "run-1",
    "repo": "api-server",
    "title": "Add rate limiting to auth endpoints",
    "workflow": "implement",
    "status": "working",          // working | pending | review | merge
    "number": null,               // PR number, if opened
    "additions": null,
    "deletions": null,
    "checks": [                   // CI check runs
      { "name": "lint", "status": "success", "duration_secs": 23 }
    ],
    "elapsed_secs": 420,
    "elapsed_warning": false,
    "resources": "4 CPU / 8 GB",
    "comments": 0,
    "question": null,             // pending human-in-the-loop question
    "sandbox_id": "sb-a1b2c3d4"
  }
]
```

#### `GET /runs/{id}/stages` response shape

```jsonc
[
  {
    "id": "detect-drift",
    "name": "Detect Drift",
    "status": "completed",        // completed | running | pending | failed
    "duration_secs": 72,
    "dot_id": "detect"            // node ID in workflow graph (for annotations)
  }
]
```

#### `GET /runs/{id}/stages/{stageId}/turns` response shape

```jsonc
[
  { "kind": "system", "content": "You are a drift detection agent..." },
  { "kind": "assistant", "content": "I'll start by loading..." },
  {
    "kind": "tool",
    "tools": [
      { "tool_name": "read_file", "args": "{ \"path\": \"...\" }", "result": "..." }
    ]
  }
]
```

#### `GET /runs/{id}/files?checkpoint=all` response shape

```jsonc
{
  "checkpoints": [
    { "id": "all", "label": "All changes" },
    { "id": "cp-4", "label": "Checkpoint 4 — Apply Changes" }
  ],
  "files": [
    {
      "old_file": { "name": "src/commands/run.ts", "contents": "..." },
      "new_file": { "name": "src/commands/run.ts", "contents": "..." }
    }
  ],
  "stats": { "additions": 567, "deletions": 234 }
}
```

#### `GET /runs/{id}/usage` response shape

```jsonc
{
  "stages": [
    {
      "stage": "Detect Drift",
      "model": "Opus 4.6",
      "input_tokens": 12480,
      "output_tokens": 3210,
      "runtime_secs": 72,
      "cost": 0.48
    }
  ],
  "totals": {
    "runtime_secs": 389,
    "input_tokens": 71540,
    "output_tokens": 21080,
    "cost": 2.26
  },
  "by_model": [
    { "model": "Opus 4.6", "stages": 2, "input_tokens": 33780, "output_tokens": 9690, "cost": 1.35 }
  ]
}
```

#### `GET /runs/{id}/verifications` response shape

```jsonc
[
  {
    "name": "Traceability",
    "question": "Do we understand what this change is and why we're making it?",
    "status": "pass",
    "controls": [
      {
        "name": "Motivation",
        "description": "Origin of proposal identified",
        "type": "ai",            // ai | automated | analysis | ai-analysis | null
        "status": "pass"         // pass | fail | na
      }
    ]
  }
]
```

---

### 2. Workflows

Entirely new. Supports the `/workflows` list page, detail page with definition/diagram/runs tabs.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/workflows` | List all workflows |
| `GET` | `/workflows/{name}` | Workflow detail (config + graph DOT) |
| `POST` | `/workflows/{name}/runs` | Trigger a run for this workflow |
| `GET` | `/workflows/{name}/runs` | List runs filtered to this workflow |

#### `GET /workflows` response shape

```jsonc
[
  {
    "name": "Fix Build",
    "slug": "fix_build",
    "filename": "fix_build.dot",
    "last_run": "2 hours ago",
    "schedule": null,             // e.g. "Daily at 09:00"
    "next_run": null              // e.g. "Starts in 3 hours"
  }
]
```

#### `GET /workflows/{name}` response shape

```jsonc
{
  "title": "Fix Build",
  "slug": "fix_build",
  "filename": "fix_build.dot",
  "description": "Automatically diagnoses and fixes CI build failures...",
  "config": "version = 1\ntask = ...",    // raw TOML
  "graph": "digraph fix_build { ... }"     // raw DOT source
}
```

---

### 3. Verifications

Entirely new. Supports the `/verifications` list and `/verifications/:slug` detail pages.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/verifications` | List all verification categories + controls |
| `GET` | `/verifications/{slug}` | Control detail (performance, evaluations, control detail, recent results) |

#### `GET /verifications` response shape

```jsonc
[
  {
    "name": "Traceability",
    "question": "Do we understand what this change is and why we're making it?",
    "controls": [
      {
        "name": "Motivation",
        "slug": "motivation",
        "description": "Origin of proposal identified",
        "type": "ai",
        "mode": "active",                  // active | evaluate | disabled
        "f1": 0.87,
        "pass_at_1": 0.82,
        "evaluations": ["pass", "pass", "fail", "pass", ...]
      }
    ]
  }
]
```

#### `GET /verifications/{slug}` response shape

```jsonc
{
  "control": {
    "name": "Motivation",
    "slug": "motivation",
    "description": "Origin of proposal identified",
    "type": "ai",
    "category": "Traceability"
  },
  "performance": {
    "mode": "active",
    "f1": 0.87,
    "pass_at_1": 0.82,
    "evaluations": ["pass", "pass", "fail", ...]
  },
  "control_detail": {
    "description": "Verifies that every change traces back to a clear origin...",
    "checks": ["PR body or linked issue explains why...", ...],
    "pass_example": "PR links to JIRA-1234...",
    "fail_example": "PR description is empty..."
  },
  "recent_results": [
    {
      "run_id": "run-047",
      "run_title": "PR #312 — Add OAuth2 PKCE flow",
      "workflow": "code_review",
      "result": "pass",
      "timestamp": "2h ago"
    }
  ],
  "siblings": [
    { "name": "Specifications", "slug": "specifications", "type": "ai", "mode": "active" }
  ]
}
```

---

### 4. Retros

Already partial (per-run). Needs a top-level list endpoint.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/retros` | List all retros across runs |
| `GET` | `/runs/{id}/retro` | Retro for a specific run (exists) |

#### `GET /retros` response shape

```jsonc
[
  {
    "run_id": "run-1",
    "pipeline_name": "implement",
    "goal": "Add rate limiting to auth endpoints",
    "timestamp": "2026-02-28T14:32:00Z",
    "smoothness": "smooth",
    "stats": {
      "total_duration_ms": 389000,
      "total_cost": 2.78,
      "total_retries": 0,
      "files_touched": [...],
      "stages_completed": 4,
      "stages_failed": 0
    },
    "friction_point_count": 0
  }
]
```

---

### 5. Sessions

Entirely new. Supports the `/start` and `/sessions/:id` pages (chat-like interaction).

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sessions` | List sessions grouped by recency |
| `POST` | `/sessions` | Create a new session |
| `GET` | `/sessions/{id}` | Session detail with full turn history |
| `POST` | `/sessions/{id}/messages` | Send a user message |
| `GET` | `/sessions/{id}/events` | SSE stream for live assistant responses |

#### `GET /sessions` response shape

```jsonc
[
  {
    "label": "Today",
    "sessions": [
      { "id": "s1", "title": "Add rate limiting to auth endpoints", "repo": "api-server", "time": "2h ago" }
    ]
  }
]
```

#### `GET /sessions/{id}` response shape

```jsonc
{
  "id": "s1",
  "title": "Add rate limiting to auth endpoints",
  "repo": "api-server",
  "model": "Opus 4.6",
  "turns": [
    { "kind": "user", "content": "Add rate limiting...", "date": "Feb 28" },
    { "kind": "assistant", "content": "I'll implement..." },
    { "kind": "tool", "tools": [{ "tool_name": "read_file", "args": "...", "result": "..." }] }
  ]
}
```

---

### 6. Insights

Entirely new. Supports the `/insights` SQL query editor.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/insights/queries` | List saved queries |
| `POST` | `/insights/queries` | Save a query |
| `PUT` | `/insights/queries/{id}` | Update a saved query |
| `DELETE` | `/insights/queries/{id}` | Delete a saved query |
| `POST` | `/insights/execute` | Execute a SQL query, return results |
| `GET` | `/insights/history` | Query execution history |

#### `POST /insights/execute` request/response

```jsonc
// Request
{ "sql": "SELECT workflow_name, COUNT(*) FROM runs GROUP BY 1" }

// Response
{
  "columns": ["workflow_name", "count"],
  "rows": [["implement", 42], ["fix_build", 18]],
  "elapsed": 0.342,
  "row_count": 6
}
```

---

### 7. Settings

Entirely new. Supports the `/settings` page.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/settings` | Get all setting groups with current values |

#### `GET /settings` response shape

```jsonc
[
  {
    "id": "general",
    "name": "General",
    "description": "Core platform settings and defaults.",
    "fields": [
      {
        "key": "org_name",
        "label": "Organization name",
        "value": "Acme Corp",
        "type": "text"
      },
      {
        "key": "timezone",
        "label": "Timezone",
        "value": "America/New_York",
        "type": "select",
        "options": ["America/New_York", "UTC", ...]
      }
    ]
  }
]
```

---

### 8. Projects (for Start page)

Supports the project/branch picker on `/start`.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/projects` | List available projects |
| `GET` | `/projects/{id}/branches` | List branches for a project |

---

## Summary: total endpoints

| Domain | Existing | New | Total |
|--------|----------|-----|-------|
| Runs | 10 | 7 | 17 |
| Workflows | 0 | 4 | 4 |
| Verifications | 0 | 2 | 2 |
| Retros | 0 | 1 | 1 |
| Sessions | 0 | 5 | 5 |
| Insights | 0 | 6 | 6 |
| Settings | 0 | 1 | 1 |
| Projects | 0 | 2 | 2 |
| **Total** | **10** | **28** | **38** |

## Priority order

1. **Runs** — extend existing endpoints to carry full board data (status columns, checks, diffs, usage, stages)
2. **Workflows** — needed for the core workflow management UI
3. **Sessions** — the primary interaction model (chat UX)
4. **Verifications** — central to the quality assurance story
5. **Retros** — lightweight list endpoint on top of existing per-run retro
6. **Insights** — SQL query interface (requires query engine backend)
7. **Settings** — configuration CRUD
8. **Projects** — start page pickers

## Open questions

- Should `GET /runs` support filtering by status column, repo, workflow? The UI has search + repo filter + view toggle.
- Should file diffs be fetched per-checkpoint or all at once with checkpoint metadata?
- Is the insights SQL query engine in-process (SQLite) or a separate service?
- Should sessions use SSE for streaming assistant responses, or WebSocket?
- How should verification criteria definitions be managed — API-editable or config-file driven?
- Should the stage turn transcript be paginated for very long conversations?
