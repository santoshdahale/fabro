# Mintlify Changelog MDX Format

Each changelog entry is a separate `.mdx` file in `docs/changelog/`.

## Template

```mdx
---
title: "Benefit-oriented title of what shipped"
date: "YYYY-MM-DD"
---

## Feature name

**Date or context line if needed**

One paragraph explaining what this enables and why it matters. Start with the user pain or limitation that existed before, then explain what's now possible. Give the feature room to breathe — 2-3 sentences minimum.

If there's a CLI command or config snippet that shows how to use it, include it:

```bash
arc run start --ssh my-workflow.dot
```

Or a config example:

```toml
[execution]
environment = "daytona"
```

## Another feature name

Another narrative section. Each major feature gets its own H2 heading. Don't bury features inside bullet lists — let them stand on their own.

---

Smaller improvements and fixes go at the bottom as a flat list, separated by a horizontal rule. No category headers needed.

- Improvements and minor enhancements as bullets
- Fixes an issue where [specific symptom] when [specific trigger]
- Another fix or minor improvement

<Warning>
**Breaking change description.** Previous behavior X now behaves as Y.

To migrate:
1. Step one
2. Step two
</Warning>
```

## Style guide

- **Each major feature gets its own H2** — don't collapse features into bullet lists
- **Narrative over bullets for features** — 2-4 sentences explaining what, why, and how
- **Include code examples** — CLI commands, config snippets, or API calls that show how to use the feature
- **Explain the "before"** — what was painful or impossible before this change
- **Fixes and minor improvements go at the bottom** as a flat bullet list after a `---` separator
- **No rigid category headers** — skip "Features", "Improvements", "Fixes" headers. Let the H2 feature names be the structure
- **Breaking changes** always in a `<Warning>` callout with migration steps

## Good vs. bad examples

Bad (bulleted feature dump):

```mdx
## Features

- **Lifecycle hooks**: Attach hooks to workflow events that execute before or after stages
- **HTTP hooks**: Call external HTTP endpoints from hooks with env var interpolation
- **Model failover**: Configure fallback models at the provider level
```

Good (each feature gets its own section with narrative and code):

```mdx
## Lifecycle hooks

Workflows can now trigger actions at key moments — before a stage starts, after it completes, or when a run fails. Use hooks to notify external systems, enforce policies, or gate execution on custom conditions.

Hooks are defined in your workflow config and support three executor types: shell commands, HTTP endpoints, and LLM-based evaluation.

## HTTP hooks

Hook executors can call external HTTP endpoints with full environment variable interpolation in request bodies. TLS mode is configurable per-hook, supporting strict validation for production and permissive mode for development.

```toml
[[hooks]]
event = "stage.before"
executor = "http"
url = "https://api.example.com/webhook"
tls_mode = "strict"
```

## Model failover

If your primary model is unavailable or rate-limited, runs now automatically switch to a backup. Configure fallback models at the provider level in your server config:

```toml
[providers.anthropic]
failover = ["openai", "gemini"]
```

Previously, a provider outage would fail the entire run. Now it retries with the next provider in the list.
```

## Rules

- `title`: short, benefit-oriented, no date in the title
- `date`: ISO 8601 format (YYYY-MM-DD)
- Filename must match the date: `YYYY-MM-DD.mdx`
- If multiple entries share a date, append a slug: `YYYY-MM-DD-feature-name.mdx`
- Breaking changes always go in a `<Warning>` callout with migration steps
