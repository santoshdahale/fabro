# Example Workflows

Use these as starting points. Choose the simplest topology that fits the requirements.

## 1. Linear Pipeline (simplest)

One-shot prompt, no tools:

```dot
digraph Hello {
    graph [goal="Write a haiku about software workflows"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    compose [label="Compose", prompt="Write a haiku (5-7-5 syllable) about software workflows. Output only the haiku, nothing else.", shape=tab, reasoning_effort="low"]

    start -> compose -> exit
}
```

## 2. Command-Then-Analyze Pipeline

Shell command feeds into LLM analysis:

```dot
digraph Pipeline {
    graph [goal="Analyze the current directory and suggest improvements"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    scan    [label="Scan Files", shape=parallelogram, script="find . -maxdepth 2 -type f | head -30"]
    analyze [label="Analyze", prompt="Review the file listing from the previous step. Identify what kind of project this is and summarize its structure in 3-4 bullet points.", shape=tab, reasoning_effort="low"]
    suggest [label="Suggest", prompt="Based on the analysis, suggest 3 concrete improvements to the project structure. Be specific and actionable.", shape=tab, reasoning_effort="low"]

    start -> scan -> analyze -> suggest -> exit
}
```

## 3. Implement-Test-Fix Loop

Agent writes code, command validates, conditional routes back on failure:

```dot
digraph BranchLoop {
    graph [goal="Create a Python script that passes its test suite"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    plan      [label="Plan", prompt="Plan a small Python script (fizzbuzz.py) and a test file (test_fizzbuzz.py) using pytest. Describe what you will create.", shape=tab, reasoning_effort="low"]
    implement [label="Implement", prompt="Create fizzbuzz.py and test_fizzbuzz.py as planned. Write the files to disk."]
    validate  [label="Validate", shape=parallelogram, script="python3 -m pytest test_fizzbuzz.py -v 2>&1 || true"]
    gate      [shape=diamond, label="Tests passing?"]

    start -> plan -> implement -> validate -> gate
    gate -> exit      [label="Pass", condition="outcome=success"]
    gate -> implement [label="Fix"]
}
```

## 4. Human Approval Gate

Draft, get human approval, then apply:

```dot
digraph HumanGate {
    graph [goal="Propose and implement a README improvement"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    draft   [label="Draft Proposal", prompt="Read the README.md (or note its absence). Propose a specific improvement. Describe your proposed changes clearly but do NOT make any changes yet.", shape=tab]
    approve [label="Approve Changes?", shape=hexagon]
    apply   [label="Apply Changes", prompt="Apply the proposed README changes that were approved."]
    skip    [label="Skip", prompt="Acknowledged. No changes made.", shape=tab, reasoning_effort="low"]

    start -> draft -> approve
    approve -> apply [label="[A] Approve"]
    approve -> skip  [label="[S] Skip"]
    apply -> exit
    skip -> exit
}
```

## 5. Plan-Approve-Implement with Revision Loop

```dot
digraph PlanImplement {
    graph [goal="Plan, approve, implement, and simplify a change"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    plan      [label="Plan", prompt="Analyze the goal and codebase. Write a clear, step-by-step implementation plan to plan.md. Include what files will change and why.", reasoning_effort="high"]
    approve   [shape=hexagon, label="Approve Plan"]
    implement [label="Implement", prompt="Read plan.md and implement every step. Make all the code changes described in the plan."]
    simplify  [label="Simplify", prompt="Review the changes just made. Simplify and clean up the code without changing behavior."]

    start -> plan -> approve
    approve -> implement [label="[A] Approve"]
    approve -> plan      [label="[R] Revise"]
    implement -> simplify -> exit
}
```

## 6. Parallel Fan-Out Review

Multiple independent analyses merged into a synthesis:

```dot
digraph Parallel {
    graph [goal="Perform a multi-perspective code review"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    fork [label="Fork Analysis", shape=component, join_policy="wait_all", error_policy="continue"]

    security     [label="Security Audit", prompt="Examine the codebase for security concerns: hardcoded secrets, injection risks, unsafe dependencies. List findings as bullet points.", shape=tab, reasoning_effort="low"]
    architecture [label="Architecture Review", prompt="Assess the codebase architecture: separation of concerns, dependency structure, modularity. List findings as bullet points.", shape=tab, reasoning_effort="low"]
    quality      [label="Code Quality", prompt="Check code quality: naming conventions, dead code, test coverage gaps, error handling. List findings as bullet points.", shape=tab, reasoning_effort="low"]

    merge  [label="Merge Findings", shape=tripleoctagon]
    report [label="Final Report", prompt="Synthesize the security, architecture, and code quality findings into a prioritized summary report with top 5 action items.", shape=tab]

    start -> fork
    fork -> security
    fork -> architecture
    fork -> quality
    security -> merge
    architecture -> merge
    quality -> merge
    merge -> report -> exit
}
```

## 7. Multi-Model with Stylesheet

Different models for different roles:

```dot
digraph MultiModel {
    graph [
        goal="Build and review a utility function using multiple models",
        model_stylesheet="
            * { model: claude-haiku-4-5;reasoning_effort: low; }
            .coding { model: claude-sonnet-4-6;reasoning_effort: high; }
            #review { model: claude-sonnet-4-6;reasoning_effort: high; }
        "
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    spec      [label="Write Spec", prompt="Write a brief spec for a TypeScript string utility module with 3 functions: slugify, truncate, and capitalize. Output the spec only.", shape=tab]
    implement [label="Implement", prompt="Implement the TypeScript string utility module from the spec. Write it to string-utils.ts.", class="coding"]
    test      [label="Write Tests", prompt="Write tests for the string utility module using Bun's test runner. Write to string-utils.test.ts.", class="coding"]
    review    [label="Code Review", prompt="Review the implementation and tests. Check for edge cases, type safety, and correctness. Provide a brief verdict.", shape=tab]

    start -> spec -> implement -> test -> review -> exit
}
```

## 8. Multi-Provider Ensemble

Independent opinions from multiple providers, then synthesize:

```dot
digraph Ensemble {
    graph [
        goal="Get independent opinions from multiple providers, then synthesize",
        model_stylesheet="
            #opus    { model: claude-opus-4-6;      }
            #gemini  { model: gemini-3.1-pro-preview;}
            #codex   { model: gpt-5.3-codex;       }
            #synth   { model: claude-opus-4-6;      reasoning_effort: high; }
        "
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    fork [label="Fan Out", shape=component, join_policy="wait_all", error_policy="continue"]

    opus   [label="Opus",   prompt="Analyze the goal. Provide your independent assessment and recommendations. Be thorough.", shape=tab]
    gemini [label="Gemini", prompt="Analyze the goal. Provide your independent assessment and recommendations. Be thorough.", shape=tab]
    codex  [label="Codex",  prompt="Analyze the goal. Provide your independent assessment and recommendations. Be thorough.", shape=tab]

    merge [label="Merge", shape=tripleoctagon]
    synth [label="Synthesize", prompt="You have received independent analyses from three different models. Compare their perspectives: identify consensus, highlight disagreements, and synthesize the strongest ideas into a single coherent recommendation.", shape=tab]

    start -> fork
    fork -> opus
    fork -> gemini
    fork -> codex
    opus   -> merge
    gemini -> merge
    codex  -> merge
    merge -> synth -> exit
}
```

## 9. Production Implement-and-Simplify with Verification

Full pipeline with toolchain checks, lint loops, and verification gates:

```dot
digraph ImplementAndSimplify {
    graph [
        goal="Implement and simplify",
        model_stylesheet="
            * { backend: api; model: claude-opus-4-6;}
        "
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    toolchain         [label="Toolchain", shape=parallelogram, script="command -v cargo >/dev/null || { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && sudo ln -sf $HOME/.cargo/bin/* /usr/local/bin/; }; cargo --version 2>&1", max_retries=0]
    preflight_compile [label="Preflight Compile", shape=parallelogram, script="cargo check 2>&1", max_retries=0]
    preflight_lint    [label="Preflight Lint", shape=parallelogram, script="cargo clippy -- -D warnings 2>&1", max_retries=0]
    fix_lints         [label="Fix Lints", prompt="The preflight lint step failed. Read the build output from context and fix all clippy lint warnings.", max_visits=3]
    implement         [label="Implement", prompt="Read the plan file referenced in the goal and implement every step. Make all the code changes described in the plan."]
    simplify          [label="Simplify", prompt="Review the changes just made. Simplify and clean up the code without changing behavior."]
    verify            [label="Verify", shape=parallelogram, script="cargo clippy -- -D warnings 2>&1 && cargo test 2>&1", goal_gate=true, retry_target="fixup"]
    fixup             [label="Fixup", prompt="The verify step failed. Read the build output from context and fix all clippy lint warnings and test failures.", max_visits=3]

    start -> toolchain
    toolchain -> preflight_compile [condition="outcome=success"]
    toolchain -> exit
    preflight_compile -> preflight_lint [condition="outcome=success"]
    preflight_compile -> exit
    preflight_lint -> implement [condition="outcome=success"]
    preflight_lint -> fix_lints
    fix_lints -> preflight_lint
    implement -> simplify -> verify
    verify -> exit  [condition="outcome=success"]
    verify -> fixup
    fixup -> verify
}
```

Paired TOML:

```toml
version = 1
graph = "workflow.dot"

[sandbox]
provider = "local"

[sandbox.local]
worktree_mode = "always"
```
