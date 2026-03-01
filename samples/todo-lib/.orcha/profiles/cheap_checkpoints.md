# Profile: cheap_checkpoints (default)

## Description

Normal operations use local LLM. Claude review before PR. Codex on repeated failures.

## Rules

- **Default agent**: local_llm
- **Review agent**: claude (pre-PR)
- **Escalation**: 2 failures → codex
- **Security gate**: Enabled
- **Size gate**: Enabled (>400 lines → paid review recommended)
