# DOT Documentation Examples Test Checklist

## Summary
- 36 .dot files (29 extracted from full workflows + 7 assembled from snippets)
- Covers 29 doc pages, 118 DOT code blocks (29 full, 89 snippets)
- Skipped: changelog/2026-02-27 (deprecated `handler=codergen` syntax)

## Phase 1: Validate (`arc validate`)

| # | File | Status | Notes |
|---|------|--------|-------|
| 1 | agents/outputs/output-patterns.dot | PASS | assembled |
| 2 | agents/prompts/pipeline.dot | PASS | added start/exit |
| 3 | changelog/2026-03-05/new-features.dot | PASS | assembled, added fallback |
| 4 | core-concepts/agents/backend-demo.dot | PASS | assembled |
| 5 | core-concepts/models/example.dot | PASS | added start/exit + wiring |
| 6 | core-concepts/workflows/my-workflow.dot | PASS | |
| 7 | examples/clone-substack/clone-substack.dot | PASS | 578 lines |
| 8 | examples/definition-of-done/spec-dod-multimodel.dot | PASS | fixed condition quoting + fallbacks |
| 9 | examples/definition-of-done/spec-dod.dot | PASS | fixed condition quoting + fallbacks |
| 10 | examples/nlspec-conformance/n-l-spec-conformance.dot | PASS | warning: goal_gate without retry_target |
| 11 | examples/semantic-port/semantic-port.dot | PASS | added fallback edges |
| 12 | examples/solitaire/build-solitaire.dot | PASS | warning: missing retry_target |
| 13 | execution/context/example.dot | PASS | added start/exit |
| 14 | execution/failures/example.dot | PASS | added start/exit |
| 15 | execution/failures/example-02.dot | PASS | added start/exit |
| 16 | execution/failures/example-03.dot | PASS | added start/exit |
| 17 | execution/failures/example-04.dot | PASS | added start/exit |
| 18 | execution/interviews/default-choice.dot | PASS | assembled |
| 19 | execution/run-configuration/c-i.dot | PASS | added start/exit, has run.toml |
| 20 | getting-started/why-arc/plan-implement.dot | PASS | |
| 21 | reference/dot-language/implement-feature.dot | PASS | |
| 22 | reference/dot-language/my-workflow.dot | PASS | |
| 23 | tutorials/branch-loop/branch-loop.dot | PASS | |
| 24 | tutorials/ensemble/ensemble.dot | PASS | |
| 25 | tutorials/hello-world/hello.dot | PASS | |
| 26 | tutorials/hello-world/sub-agent.dot | PASS | |
| 27 | tutorials/hello-world/tool-use.dot | PASS | |
| 28 | tutorials/multi-model/multi-model.dot | PASS | |
| 29 | tutorials/parallel-review/parallel.dot | PASS | |
| 30 | tutorials/plan-implement/plan-implement.dot | PASS | has @prompt stub |
| 31 | workflows/human-in-the-loop/hitl-patterns.dot | PASS | assembled |
| 32 | workflows/stages-and-nodes/all-node-types.dot | PASS | assembled, 15 nodes |
| 33 | workflows/stylesheets/example.dot | PASS | |
| 34 | workflows/transitions/transition-patterns.dot | PASS | assembled, added fallbacks |
| 35 | workflows/variables/check.dot | PASS | has run.toml |
| 36 | workflows/variables/example.dot | PASS | added start/exit |

## Phase 2: Dry Run (`arc run start --dry-run --auto-approve`)

| # | File | Status | Notes |
|---|------|--------|-------|
| 1-36 | (all) | | |

## Phase 3: Haiku (`arc run start --model claude-haiku-4-5 --auto-approve`)

| # | File | Status | Notes |
|---|------|--------|-------|
| 1-36 | (all) | | |

## Phase 4: Full (`arc run start --auto-approve`)

| # | File | Status | Notes |
|---|------|--------|-------|
| 1-36 | (all) | | |

## Issues Found During Validation (fixed in test DOTs)

1. **Condition parser doesn't support multi-word values** — `preferred_label=More fixes needed` fails parse. Fixed by using underscored values (`more_fixes_needed`). Affects: definition-of-done examples. **This is a docs bug** — the source DOTs in docs/examples/ use multi-word condition values that won't parse.

2. **Several "full" digraphs in docs lack start/exit nodes** — 9 extracted DOTs were minimal digraph wrappers showing graph-level attributes without start/exit nodes or wiring. Fixed by adding them in test DOTs.

3. **All-conditional edges need unconditional fallback** — Validator requires at least one fallback edge when a node has only conditional outgoing edges. Fixed by adding fallback edges. Affects: semantic-port, definition-of-done examples, and assembled snippet DOTs.

## Commands

```bash
# Run each phase:
./test/docs/run_tests.sh validate
./test/docs/run_tests.sh dry-run
./test/docs/run_tests.sh haiku
./test/docs/run_tests.sh full
```
