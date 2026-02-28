# Goal

## Background

This sample demonstrates a tiny, testable Rust domain module (`TodoList`) that can be used to validate orcha's cycle flow.
The objective is to keep the code small while still having clear behavior and verification.

## Acceptance Criteria

- [ ] `TodoList::add` assigns incremental IDs starting from `1`.
- [ ] `TodoList::complete` marks the target item as done and returns `false` for unknown IDs.
- [ ] `TodoList::pending_titles` returns only unfinished task titles.

## Constraints

- Keep the implementation in `samples/todo-lib/src/lib.rs`.
- Do not add external dependencies.
- Keep unit tests in the same file (`#[cfg(test)]`).

## Verification Commands

Runtime commands are managed in `.orcha/orcha.yml` (`execution.verification.commands`).

## Quality Priority

quality
