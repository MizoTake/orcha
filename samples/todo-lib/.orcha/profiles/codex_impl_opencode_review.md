# Profile: codex_impl_opencode_review

## Description

Use codex for implementation and opencode(local_llm) for review.

## Rules

- **Default agent**: codex
- **Review agent**: local_llm
- **Escalation**: 2 failures → codex
- **Security gate**: Disabled
- **Size gate**: Disabled
