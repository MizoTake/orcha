# Repository Guidelines

## Project Structure & Module Organization
The codebase is a Rust CLI application.

- `src/main.rs`: executable entrypoint (`orcha` binary).
- `src/lib.rs`: module wiring.
- `src/cli/`: command handlers (`init`, `run`, `status`, `profile`, `explain`).
- `src/core/`: domain logic (status, cycle, tasks, gates, profiles, health, logs).
- `src/agent/`: agent abstraction, routing, provider backends, verifier.
- `src/phase/`: lifecycle phases (`briefing` -> `plan` -> `impl` -> `review` -> `fix` -> `verify` -> `decide`).
- `src/markdown/`: frontmatter parser and `.orcha` template generation.
- `src/config/`: environment-based runtime config.

Unit tests are in-file (`#[cfg(test)]`) across modules; there is no separate `tests/` directory currently.

## Build, Test, and Development Commands
- `cargo build`: debug build.
- `cargo build --release`: optimized build.
- `cargo check`: fast type/lint sanity check without binary output.
- `cargo test --lib`: run library unit tests (currently 38 tests).
- `cargo test <name>`: run a specific test.
- `cargo run -- --help`: inspect CLI usage.
- `cargo run -- init --orch-dir .orcha`: scaffold workflow files.

## Coding Style & Naming Conventions
- Follow Rust defaults: 4-space indentation, `snake_case` for functions/modules, `CamelCase` for types/enums.
- Keep modules focused by responsibility (`cli`, `core`, `phase`, etc.).
- Prefer `Result`-based error propagation (`anyhow` at boundaries, domain errors in `core/error.rs`).
- Run `cargo fmt -- --check` before PR.  
  Note: as of current repository state, this command reports existing formatting diffs.

## Testing Guidelines
- Use unit tests near the implementation (`#[cfg(test)] mod tests`).
- Name tests by behavior (example: `phase_progression`, `extract_commands_from_goal`).
- Minimum local gate before PR: `cargo check` and `cargo test --lib`.
- For parser/phase changes, add regression tests that fail before the fix.

## Commit & Pull Request Guidelines
- **Assumption (limited history):** `git log` currently has one commit with an imperative, concise subject (`Implement ...`), no prefix/scope.
- Keep commits small and single-purpose (`1 commit = 1 intent`).
- PRs should include:
  - purpose and impacted modules (e.g., `src/core/status.rs`);
  - verification commands + results;
  - sample CLI output when behavior changes (`orcha status`, `orcha run`).

## Security & Configuration Tips
- Configure providers via environment variables (`ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `OPENAI_API_KEY`, etc.).
- Use `.env` locally; never commit secrets.
- Do not commit generated build artifacts (`target/`).
