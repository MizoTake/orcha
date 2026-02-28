# Profile: unblock_first

## Description

Aggressive unblocking. Codex on first verify failure. Claude diagnosis on continued failure.

## Rules

- **Default agent**: local_llm
- **Review agent**: local_llm
- **Escalation**: 1 failure → codex, continued → claude diagnosis
- **Security gate**: Enabled
- **Size gate**: Enabled
