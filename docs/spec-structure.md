# Spec Format Reference

Common format shared by `specs/unified-llm-spec.md`, `specs/coding-agent-loop-spec.md`, and `specs/attractor-spec.md`.

---

## 1. Title and Opening Paragraph

Each spec starts with a level-1 heading (`# <Name> Specification`) followed immediately by a one-sentence summary paragraph that describes what the spec is, states it is **language-agnostic**, and says it is **designed to be implementable from scratch by any developer or coding agent in any programming language**. This framing signals the intended audience: an AI coding agent or a human developer doing a greenfield implementation.

## 2. Horizontal Rule + Table of Contents

A `---` separator follows the summary, then a `## Table of Contents` section with a numbered list of all top-level sections, each as a markdown anchor link (e.g., `[Overview and Goals](#1-overview-and-goals)`).

## 3. Numbered Top-Level Sections

All sections use the pattern `## N. Section Name` where N is a sequential integer. Subsections use `### N.M` (e.g., `### 2.3 Session Lifecycle`). This gives every concept a unique coordinate (e.g., "Section 5.4") for cross-referencing.

## 4. Section 1: Overview and Goals

Always the first section. Contains these standard subsections:

### 4.1 Problem Statement (1.1)

A prose description of the problem being solved. Written in concrete, opinionated terms. Explains *why* this thing needs to exist by describing the pain of not having it.

### 4.2 Design Principles (1.2)

A bulleted list of named principles, each formatted as **`Bold keyword.`** followed by an explanation. Examples: "Provider-agnostic.", "Streaming-first.", "Declarative pipelines.", "Hackable." These are prescriptive statements about how the system should behave, not aspirational goals.

### 4.3 Reference Open-Source Projects (1.3 or 1.4)

A list of existing projects that solve related problems. Each entry includes the project name, URL, language, and a description of what patterns to study from it. Explicitly stated as "not dependencies" -- inspiration sources only.

### 4.4 Architecture Diagram

An ASCII art box diagram showing the layers/components and how they connect. All three specs include at least one.

### 4.5 Relationship to Companion Specs

When the spec depends on another (coding-agent-loop depends on unified-llm; attractor depends on coding-agent-loop), it states this explicitly with the types it imports and how the layering works.

## 5. Core Technical Sections

The middle sections define the system's data model, algorithms, and contracts using a consistent notation.

### 5.1 Pseudocode and Type Definitions

All code is written in a language-neutral pseudocode style:

- **Records** use `RECORD Name:` with indented fields as `field_name : Type -- comment`
- **Enums** use `ENUM Name:` with indented values
- **Interfaces** use `INTERFACE Name:` with method signatures
- **Functions** use `FUNCTION name(params) -> ReturnType:` with indented body
- **Control flow** uses `IF`, `ELSE`, `FOR EACH`, `WHILE`, `LOOP`, `BREAK`, `CONTINUE`, `RETURN`, `TRY/CATCH`
- Keywords are UPPERCASE: `APPEND`, `AWAIT_ALL`, `YIELD`, `NONE`

### 5.2 Type Convention

A standard set of type primitives is used across all three specs:

- `String`, `Integer`, `Float`, `Boolean`, `Bytes`, `Dict`
- `List<T>` for ordered collections
- `T | None` for optional values
- `T | U` for union types
- `Map<K, V>` for key-value stores

### 5.3 Tables

Heavy use of markdown tables for:

- **Attribute reference tables** with columns: Key, Type, Default, Description
- **Provider mapping tables** showing how one concept translates per-provider (OpenAI / Anthropic / Gemini)
- **Enum value tables** with Value and Meaning columns

### 5.4 Design Decision Rationale

When a non-obvious choice is made, it's explained inline with **bold "Why..." questions**. For example: "**Why two methods, not one.**" or "**Why provider-aligned toolsets instead of a universal tool set?**" These appear immediately after the design they justify, or collected in an appendix.

## 6. Out of Scope / Nice-to-Haves (optional)

A section explicitly listing features that are *intentionally excluded*, with an explanation of why each is out of scope and where in the architecture it could be added later. Prevents scope creep and signals extensibility points. (Present in coding-agent-loop-spec; not all specs include this.)

## 7. Definition of Done

Always the **last numbered section**. Opens with the standard sentence: "This section defines how to validate that an implementation of this spec is complete and correct. An implementation is done when every item is checked off."

### 7.1 Subsections by Feature Area

Each subsection (e.g., "Core Infrastructure", "DOT Parsing", "Provider Adapters") contains a markdown checklist (`- [ ]`) of specific, verifiable assertions.

### 7.2 Cross-Provider/Feature Parity Matrix

A markdown table where rows are test cases and columns are providers (or a single "Pass" column). Every cell is `[ ]`. Serves as a validation matrix ensuring nothing is missed.

### 7.3 Integration Smoke Test

The final subsection. Contains a pseudocode end-to-end test that exercises the major codepaths with real APIs/backends. Written as executable assertions (`ASSERT`), not prose. This is the "if this passes, you're done" test.

## 8. Appendices

After the Definition of Done, labeled as `## Appendix A/B/C/D: Title`. Used for:

- Reference material too detailed for the main spec (e.g., the `apply_patch` v4a format grammar)
- Complete attribute reference tables
- Error category taxonomies
- Design decision rationale (when there's a lot of it)

## 9. Cross-cutting Patterns

Several patterns repeat across all three specs:

**Escape hatches over false abstractions.** Each spec defines a clean unified model, then provides explicit escape hatches (`provider_options`, `type` attribute overrides, `CodergenBackend` interface) for cases the unified model doesn't cover. The escape hatches are documented, not hidden.

**Concrete defaults with override points.** Every configurable value has a stated default (e.g., `max_tool_rounds_per_input = 200`, `default_command_timeout_ms = 10000`). Nothing is left as "implementation-defined."

**Provider-specific mapping tables.** When behavior differs per LLM provider, a table shows the exact field/header/API mapping for each.

**Event-driven observability.** All three systems emit typed events for external consumption. Event kinds are defined as enums with clear semantics.

**Separation of concerns via interfaces.** Key extension points are defined as interfaces (`ProviderAdapter`, `ExecutionEnvironment`, `Handler`, `CodergenBackend`, `Interviewer`) that decouple the core from implementations.
