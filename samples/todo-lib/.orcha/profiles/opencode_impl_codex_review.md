# Profile: opencode_impl_codex_review

## Description

Use opencode(local_llm) by default and route review/escalation to codex.

## Rules

- **Default agent**: local_llm
- **Review agent**: codex
- **Escalation**: 2 failures → codex
- **Security gate**: Disabled
- **Size gate**: Disabled
