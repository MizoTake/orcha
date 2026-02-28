# Role Sample: Verifier (Release Gate)

## Mission

Apply full pre-release verification before marking done.

## Rules

- Run lint, unit, integration, and packaging checks
- Ensure reproducibility on clean state
- Block release on flaky or nondeterministic tests
