/// Template content for `orcha init`.

pub fn goal_md() -> &'static str {
    r#"# Goal

## Background

<!-- Describe the background and motivation for this goal -->

## Acceptance Criteria

- [ ] Criterion 1
- [ ] Criterion 2

## Constraints

- Constraint 1

## Verification Commands

Execution commands are defined in `orcha.yml` under:

```yaml
execution:
  verification:
    commands:
      - "cargo test"
```

## Quality Priority

<!-- speed / cost / quality -->
cost
"#
}

pub fn task_md(id: &str, title: &str) -> String {
    format!(
        r#"---
id: {id}
title: '{title}'
owner: ''
created: '{timestamp}'
---

## Description

<!-- Describe what this task should accomplish -->

## Evidence

<!-- Filled in when the task is completed -->

## Notes

<!-- Additional notes -->
"#,
        id = id,
        title = title.replace('\'', "''"),
        timestamp = chrono::Utc::now().to_rfc3339(),
    )
}

pub fn orcha_yml() -> &'static str {
    r#"version: 1

agents:
  local_llm:
    mode: "cli" # http | cli
    model: null         # optional, e.g. "openai/gpt-4.1"
    cli:
      command: "opencode-cli"
      args: ["run"]      # e.g. ["run", "--format", "json"]
      prompt_via_stdin: false
      model_arg: "-m"    # appended as: -m <model>
      ensure_no_permission_flags: true # codex/claude は no-permission 向けフラグ、opencode は OPENCODE_PERMISSION を自動付与
      timeout_seconds: 21600 # CLI呼び出しのタイムアウト（秒）
  claude: # legacy alias: anthropic
    api_key_env: "ANTHROPIC_API_KEY"
    model: "claude-sonnet-4-20250514"
  gemini:
    api_key_env: "GEMINI_API_KEY"
    model: "gemini-2.0-flash"
  codex: # legacy alias: openai
    api_key_env: "OPENAI_API_KEY"
    model: "gpt-4.1"

execution:
  profile: "cheap_checkpoints" # built-ins: local_only | cheap_checkpoints | quality_gate | unblock_first | opencode_impl_no_review | opencode_impl_claude_review | opencode_impl_codex_review | claude_impl_opencode_review | codex_impl_opencode_review (or .orcha/profiles/<name>.md)
  profile_strategy:
    alternating: []    # e.g. ["cheap_checkpoints", "quality_gate"] (cycleごとに交互切替)
    every_n_cycles: [] # e.g. [{ interval: 3, profile: "unblock_first", offset: 0 }]
    mixins: []         # e.g. [{ from: "quality_gate", fields: ["review_agent", "security_gate"] }]
  cli_limit:
    disable_agent_on_limit: true # true: claude/codex CLI が limit/quota エラー時にその run 中は無効化
  max_cycles: 0 # run全体の最大サイクル数（0は無制限）
  phase_timeout_seconds: 21600 # 各phaseのタイムアウト（秒）
  max_consecutive_verify_failures: 3 # verify連続失敗で停止
  human_escalation:
    on_consecutive_failures: 0 # >0 で連続失敗時に人間へ委譲（0は無効）
    on_ambiguous_spec: false # true で --spec 時の曖昧点検出で停止して確認要求
    channel: "terminal" # terminal | slack など（現状はinbox通知に記録）
  acceptance_criteria:
    - "Criterion 1"
    - "Criterion 2"
  verification:
    commands:
      - "echo \"replace with actual verification commands\""
"#
}

pub fn status_md(run_id: &str, profile: &str) -> String {
    format!(
        r#"---
run_id: {run_id}
profile: {profile}
cycle: 0
phase: briefing
last_update: '{timestamp}'
budget:
  paid_calls_used: 0
  paid_calls_limit: 20
locks:
  writer: null
  active_task: null
---

## Goal

(See goal.md)

## Overall

- **Cycle**: 0
- **Phase**: briefing
- **Health**: green

## Blocking

None.

## Next Actions

1. Configure goal.md with your objective
2. Run `orcha run` to start

## Escalation Rules

See active profile for escalation rules.

## Latest Notes

Initialized.
"#,
        run_id = run_id,
        profile = profile,
        timestamp = chrono::Utc::now().to_rfc3339(),
    )
}

pub fn status_log_md() -> &'static str {
    "# Status Log\n\n<!-- Append-only. Do not edit existing entries. -->\n"
}

pub fn role_planner_md() -> &'static str {
    r#"# Role: Planner

## Mission

Analyze the goal and current status to create or update a task plan. Break down the goal into concrete, actionable tasks.

## Checklist

- [ ] Read goal.md and understand acceptance criteria
- [ ] Review current status and completed tasks
- [ ] Identify remaining work
- [ ] Create/update task table with clear, atomic tasks
- [ ] Assign priorities and owners

## Output Format

Return an updated task table in markdown format:

```
| ID | Title | State | Owner | Evidence | Notes |
|---|---|---|---|---|---|
| T1 | Task title | issue | agent | | description |
```

Also provide a brief summary of the plan rationale.
"#
}

pub fn role_implementer_md() -> &'static str {
    r#"# Role: Implementer

## Mission

Execute the assigned task by writing or modifying code. Follow the task description and constraints.

## Checklist

- [ ] Read the task description and acceptance criteria
- [ ] Understand the existing codebase context
- [ ] Implement the changes
- [ ] Ensure changes are minimal and focused
- [ ] Provide evidence of completion

## Output Format

Return:
1. Summary of changes made
2. Files modified/created
3. Any issues encountered
4. Evidence of completion (test output, etc.)
"#
}

pub fn role_reviewer_md() -> &'static str {
    r#"# Role: Reviewer

## Mission

Review the implementation changes for correctness, security, and quality.

## Checklist

- [ ] Review all changed files
- [ ] Check for security issues
- [ ] Check for correctness
- [ ] Check for edge cases
- [ ] Determine if paid review is needed

## Output Format

```
Findings: High / Med / Low
Must-fix:
- item 1
- item 2

paid_review_required: yes/no
reason: explanation
```
"#
}

pub fn role_verifier_md() -> &'static str {
    r#"# Role: Verifier

## Mission

Run verification commands to confirm that the implementation meets the acceptance criteria.

## Checklist

- [ ] Run all verification commands from orcha.yml (`execution.verification.commands`)
- [ ] Capture output and exit codes
- [ ] Report pass/fail status

## Output Format

For each command:
```
Command: <command>
Exit code: <code>
Status: PASS / FAIL
Output: <truncated output>
```

Overall: PASS / FAIL
"#
}

pub fn role_scribe_md() -> &'static str {
    r#"# Role: Scribe

## Mission

Prepare briefing context for the current cycle. Summarize the current state, recent changes, and what needs attention.

## Checklist

- [ ] Read goal.md
- [ ] Read status.md
- [ ] Read recent status_log entries
- [ ] Check inbox for external messages
- [ ] Summarize current situation

## Output Format

Provide a briefing document with:
1. Goal summary
2. Current progress
3. Outstanding issues
4. Inbox messages (if any)
5. Recommended focus for this cycle
"#
}

pub fn role_sample_planner_backlog_md() -> &'static str {
    r#"# Role Sample: Planner (Backlog-Driven)

## Mission

Produce a stable backlog aligned to acceptance criteria.

## Rules

- Preserve task IDs once created
- Split tasks to be completed within one cycle
- Keep at least one explicit verification task
"#
}

pub fn role_sample_planner_risk_first_md() -> &'static str {
    r#"# Role Sample: Planner (Risk-First)

## Mission

Order tasks by failure impact, not by implementation convenience.

## Rules

- Prioritize auth/data-loss/security risks first
- Add rollback or mitigation task for each high-risk change
- Mark risky tasks with clear notes
"#
}

pub fn role_sample_implementer_tdd_md() -> &'static str {
    r#"# Role Sample: Implementer (TDD)

## Mission

Implement using red-green-refactor discipline.

## Rules

- Write or update a failing test first
- Implement minimum code to pass
- Refactor while preserving behavior
"#
}

pub fn role_sample_implementer_surgical_md() -> &'static str {
    r#"# Role Sample: Implementer (Surgical)

## Mission

Make the smallest safe diff that solves the task.

## Rules

- Touch only required files
- Avoid interface changes unless explicitly requested
- Include concise evidence for each change
"#
}

pub fn role_sample_reviewer_security_md() -> &'static str {
    r#"# Role Sample: Reviewer (Security-First)

## Mission

Detect vulnerabilities and privilege boundary regressions.

## Rules

- Audit authn/authz logic and secret handling
- Flag dangerous defaults and broad permissions
- Treat ambiguous security behavior as must-fix
"#
}

pub fn role_sample_reviewer_regression_md() -> &'static str {
    r#"# Role Sample: Reviewer (Regression-First)

## Mission

Prevent behavior regressions and missing edge-case coverage.

## Rules

- Check critical user paths end-to-end
- Verify backward compatibility expectations
- Require tests for every fixed defect
"#
}

pub fn role_sample_verifier_fast_md() -> &'static str {
    r#"# Role Sample: Verifier (Fast Feedback)

## Mission

Give fast and repeatable pass/fail feedback.

## Rules

- Run smoke checks first
- Stop early on first deterministic failure
- Report the exact failing command and output
"#
}

pub fn role_sample_verifier_release_md() -> &'static str {
    r#"# Role Sample: Verifier (Release Gate)

## Mission

Apply full pre-release verification before marking done.

## Rules

- Run lint, unit, integration, and packaging checks
- Ensure reproducibility on clean state
- Block release on flaky or nondeterministic tests
"#
}

pub fn role_sample_scribe_compact_md() -> &'static str {
    r#"# Role Sample: Scribe (Compact)

## Mission

Write short, high-signal briefings.

## Rules

- Keep each section under 5 lines
- Highlight only blockers and next actions
- Omit repeated history
"#
}

pub fn role_sample_scribe_handoff_md() -> &'static str {
    r#"# Role Sample: Scribe (Handoff-Heavy)

## Mission

Optimize context for external-tool handoff.

## Rules

- Include decision rationale with evidence links
- Separate facts, assumptions, and requests explicitly
- End with a concrete handoff checklist
"#
}

pub fn inbox_md() -> &'static str {
    "# Inbox\n\nNo pending messages.\n\n<!-- External tools: append messages here. Do not edit agentworkspace/status.md directly. -->\n"
}

pub fn outbox_md() -> &'static str {
    "# Outbox\n\nNo pending messages.\n\n<!-- Orchestrator writes messages here for external tools to pick up. -->\n"
}

pub fn agent_workspace_readme_md() -> &'static str {
    "# Agent Workspace\n\nGenerated artifacts from agent responses are written here.\n`status.md` and `status_log.md` are also managed in this directory.\n\nDo not edit files in this directory manually while a run is active.\n"
}

pub fn profile_local_only_md() -> &'static str {
    r#"# Profile: local_only

## Description

All processing done by local LLM. No paid API calls. If stuck, mark as blocked.

## Rules

- **Default agent**: local_llm
- **Review agent**: local_llm
- **Escalation**: None (blocked when stuck)
- **Security gate**: Disabled
- **Size gate**: Disabled
"#
}

pub fn profile_cheap_checkpoints_md() -> &'static str {
    r#"# Profile: cheap_checkpoints (default)

## Description

Normal operations use local LLM. Claude review before PR. Codex on repeated failures.

## Rules

- **Default agent**: local_llm
- **Review agent**: claude (pre-PR)
- **Escalation**: 2 failures → codex
- **Security gate**: Enabled
- **Size gate**: Enabled (>400 lines → paid review recommended)
"#
}

pub fn profile_quality_gate_md() -> &'static str {
    r#"# Profile: quality_gate

## Description

Claude required for auth/crypto/public API changes. Claude for large diffs.

## Rules

- **Default agent**: local_llm
- **Review agent**: claude
- **Escalation**: 2 failures → claude
- **Security gate**: Enabled (auth/crypto/public API → claude required)
- **Size gate**: Enabled (>400 lines → claude required)
"#
}

pub fn profile_unblock_first_md() -> &'static str {
    r#"# Profile: unblock_first

## Description

Aggressive unblocking. Codex on first verify failure. Claude diagnosis on continued failure.

## Rules

- **Default agent**: local_llm
- **Review agent**: local_llm
- **Escalation**: 1 failure → codex, continued → claude diagnosis
- **Security gate**: Enabled
- **Size gate**: Enabled
"#
}

pub fn profile_opencode_impl_no_review_md() -> &'static str {
    r#"# Profile: opencode_impl_no_review

## Description

Use opencode(local_llm) for implementation only, with no dedicated review.

## Rules

- **Default agent**: local_llm
- **Review agent**: none
- **Escalation**: None
- **Security gate**: Disabled
- **Size gate**: Disabled
"#
}

pub fn profile_opencode_impl_claude_review_md() -> &'static str {
    r#"# Profile: opencode_impl_claude_review

## Description

Use opencode(local_llm) by default and route review/escalation to claude.

## Rules

- **Default agent**: local_llm
- **Review agent**: claude
- **Escalation**: 2 failures → claude
- **Security gate**: Enabled
- **Size gate**: Enabled
"#
}

pub fn profile_opencode_impl_codex_review_md() -> &'static str {
    r#"# Profile: opencode_impl_codex_review

## Description

Use opencode(local_llm) by default and route review/escalation to codex.

## Rules

- **Default agent**: local_llm
- **Review agent**: codex
- **Escalation**: 2 failures → codex
- **Security gate**: Disabled
- **Size gate**: Disabled
"#
}

pub fn profile_claude_impl_opencode_review_md() -> &'static str {
    r#"# Profile: claude_impl_opencode_review

## Description

Use claude for implementation and opencode(local_llm) for review.

## Rules

- **Default agent**: claude
- **Review agent**: local_llm
- **Escalation**: 2 failures → claude
- **Security gate**: Enabled
- **Size gate**: Enabled
"#
}

pub fn profile_codex_impl_opencode_review_md() -> &'static str {
    r#"# Profile: codex_impl_opencode_review

## Description

Use codex for implementation and opencode(local_llm) for review.

## Rules

- **Default agent**: codex
- **Review agent**: local_llm
- **Escalation**: 2 failures → codex
- **Security gate**: Disabled
- **Size gate**: Disabled
"#
}
