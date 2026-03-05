---
name: changelog
description: Generate and update the product changelog in Mintlify docs. Use when the user asks to update the changelog, add a changelog entry, document recent changes, or write release notes. Reads git history on main, filters to user-facing changes, and writes dated MDX files to docs/changelog/.
---

# Changelog

Generate user-facing changelog entries from git history and write them as Mintlify MDX files.

## Writing principles

Write for the person who uses the product, not the person who built it.

- **Each major feature gets its own H2 heading.** Don't collapse features into bullet lists under category headers. Let each feature breathe with its own section.
- **Narrative over bullets for features.** 2-4 sentences explaining what it does, why it matters, and what was painful before. Bad: "**Model failover**: Configure fallback models at the provider level." Good: a full section explaining the before/after with a config example.
- **Include code examples.** CLI commands, config snippets, or API calls that show how to use the feature. Users should be able to copy-paste something immediately.
- **Explain the "before".** What was painful, impossible, or manual before this change? Then show what's now possible.
- **Name features the way users know them.** Use the UI label or docs term, not internal module/crate names.
- **Be specific about fixes.** "Fixes an issue where long-running stages could timeout during checkpoint saves" tells users whether this affected them. "Bug fixes" tells them nothing.
- **Minor improvements and fixes go at the bottom** as a flat bullet list after a `---` separator. No category headers needed.
- **Breaking changes first** in a `<Warning>` callout at the top of the entry, with migration steps.
- **Most important change first** — don't bury the lede.

## Workflow

### 1. Determine date range

Read filenames in `docs/changelog/` to find the most recent entry date. If no entries exist, the changelog starts from 2025-02-19 (first commit).

### 2. Gather changes

Run `git log --oneline --no-merges main` for commits since the last entry date. Read commit messages and changed files to understand the actual user-facing impact — don't just reword commit messages.

### 3. Filter and group by date

Group commits by their commit date. Each date that has user-facing changes gets its own entry file. Dates with only internal changes get no entry.

Include only changes visible to end users:
- New features and capabilities
- Bug fixes that affected users
- Breaking changes or behavioral changes
- New integrations or provider support
- Performance improvements users would notice
- UI/UX changes

Exclude:
- Internal refactors with no behavior change
- Test-only changes
- CI/CD pipeline changes
- Dependency bumps (unless they fix a user-facing issue)
- Code style or linting changes

If there are no user-facing changes in the entire range, tell the user and stop.

### 4. Write changelog entries

Create one file per date at `docs/changelog/YYYY-MM-DD.mdx`, using the commit date (not today's date). See [references/format.md](references/format.md) for the exact MDX format.

Within each date's entry:
- **Each major feature gets its own H2 heading** with 2-4 sentences of narrative and a code example where relevant
- **Batch related commits** into a single feature section (e.g., multiple hook-related commits become one "Lifecycle hooks" section)
- **Breaking changes** go at the top in a `<Warning>` callout with migration steps
- **Minor improvements and fixes** go at the bottom after a `---` horizontal rule as a flat bullet list — no category headers

### 5. Update docs/docs.json

Add all new pages to the Changelog tab's pages array in `docs/docs.json`. List entries most recent first. The page path is `changelog/YYYY-MM-DD` (no `.mdx` extension).

### 6. Clean up legacy single-file changelog

If `docs/changelog.mdx` still exists as the old single-file changelog, delete it and remove its reference from `docs/docs.json`.
