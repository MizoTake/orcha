# Profile: quality_gate

## Description

Claude required for auth/crypto/public API changes. Claude for large diffs.

## Rules

- **Default agent**: local_llm
- **Review agent**: claude
- **Escalation**: 2 failures → claude
- **Security gate**: Enabled (auth/crypto/public API → claude required)
- **Size gate**: Enabled (>400 lines → claude required)
