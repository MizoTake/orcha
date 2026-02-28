# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test --lib         # Run all unit tests (24 tests)
cargo test <test_name>   # Run a single test by name
cargo run -- init        # Run the CLI (e.g., orcha init)
```

There is no linter or formatter configured beyond `cargo check`. No CI/CD pipeline exists yet.

## Project Overview

Orcha is a Rust CLI tool for orchestrating multiple AI agents through a development task lifecycle. It uses a **Markdown-first** approach: all state lives in `.orcha/` as markdown files with YAML frontmatter, making workflows human-readable and observable by external tools.

The design specification lives in `spec.md`.

## Architecture

Six top-level modules (`src/lib.rs`):

- **`cli/`** — clap-derive command handlers (`init`, `run`, `status`, `profile`, `explain`). The global `--orch-dir` flag defaults to `.orch`.
- **`core/`** — Domain logic: `StatusFile` (YAML frontmatter + markdown), `Task` (table parsing/rendering), `Phase`/`CycleDecision` cycle engine, `Profile`/`ProfileRules`, three gate types (security/unblock/size), `HealthStatus`, append-only logging, handoff inbox/outbox, and `OrchaError` (thiserror).
- **`agent/`** — `Agent` trait (`respond(&self, &AgentContext) -> AgentResponse`) with four backends in `backend/`: `local_llm` (OpenAI-compatible), `anthropic` (Claude), `gemini`, `codex`. The `router` selects which agent to use based on phase, profile, and gate evaluation. The `verifier` runs shell commands from `goal.md`.
- **`phase/`** — Seven phase implementations: briefing → plan → impl → review → fix → verify → decide. Each phase reads context, calls an agent (or runs shell commands for verify), and produces output that updates status.
- **`markdown/`** — Generic YAML frontmatter parser (`frontmatter.rs`) and template generators (`template.rs`) for all `.orcha/` scaffold files.
- **`config/`** — `AppConfig::from_env()` reads LLM endpoints/keys/models from environment variables (with `.env` via dotenvy).

### Key execution flow

`main.rs` → loads `.env`, inits tracing, parses CLI → dispatches to `cli::run::execute` → loads `StatusFile` → checks locks → calls phase handler → phase uses `agent::router` to pick agent → agent backend makes API call → response updates `status.md` → `CycleDecision` determines next step.

### Cycle model

A run goes through up to `MAX_CYCLES` (5) iterations of the 7-phase sequence. The `Decide` phase evaluates whether to loop (`NextCycle`), finish (`Done`), or stop (`Blocked`/`Escalate`).

### Gate system

Gates override agent selection: **Security gate** detects auth/crypto/security keywords and escalates to Claude. **Unblock gate** triggers on consecutive verify failures. **Size gate** recommends review for diffs >400 lines.

### Profile system

Four profiles control agent selection defaults and escalation rules: `local_only`, `cheap_checkpoints`, `quality_gate`, `unblock_first`.

## Environment Variables

| Variable | Default |
|---|---|
| `LOCAL_LLM_ENDPOINT` | `http://localhost:11434/v1` |
| `LOCAL_LLM_MODEL` | `llama3.2` |
| `ANTHROPIC_API_KEY` | (none) |
| `ANTHROPIC_MODEL` | `claude-sonnet-4-20250514` |
| `GEMINI_API_KEY` | (none) |
| `GEMINI_MODEL` | `gemini-2.0-flash` |
| `OPENAI_API_KEY` | (none) |
| `CODEX_MODEL` | `gpt-4.1` |

## Conventions

- All I/O is async (tokio). File operations use `tokio::fs`.
- Error handling: `anyhow::Result` at boundaries, `OrchaError` (thiserror) for domain errors.
- Tests are in-file `#[cfg(test)] mod tests` blocks; no separate test directory.
- The `Agent` trait uses `async-trait`. All backends implement `Agent: Send + Sync`.
- Markdown frontmatter is generic over `serde::Deserialize` types.
