# Profile: claude_impl_opencode_review

## Description

Use claude for implementation and opencode(local_llm) for review.

## Rules

- **Default agent**: claude
- **Review agent**: local_llm
- **Escalation**: 2 failures → claude
- **Security gate**: Enabled
- **Size gate**: Enabled
