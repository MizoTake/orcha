# Profile: opencode_claude

## Description

Use opencode(local_llm) by default and route review/escalation to claude.

## Rules

- **Default agent**: local_llm
- **Review agent**: claude
- **Escalation**: 2 failures → claude
- **Security gate**: Enabled
- **Size gate**: Enabled
