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

```
echo "replace with actual verification commands"
```

## Quality Priority

<!-- speed / cost / quality -->
cost
"#
}

pub fn team_md() -> &'static str {
    r#"# Team

## Principles

- Local-first: use local LLM for all standard operations
- Escalate only when necessary

## Members

### local_llm

- **Strength**: Fast, free, no rate limits
- **Weakness**: Lower quality for complex reasoning
- **Cost level**: Free
- **Use when**: Default for all standard tasks

### codex

- **Strength**: Good at code generation and debugging
- **Weakness**: Paid API calls
- **Cost level**: Medium
- **Use when**: Local LLM fails repeatedly (unblock gate)

### claude

- **Strength**: Excellent reasoning, security review, architecture
- **Weakness**: Higher cost, rate limits
- **Cost level**: High
- **Use when**: Security gate, quality gate, complex review

### gemini

- **Strength**: Large context window, good general reasoning
- **Weakness**: Paid API calls
- **Cost level**: Medium
- **Use when**: Large codebase analysis, alternative review
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

## Task Table

| ID | Title | State | Owner | Evidence | Notes |
|---|---|---|---|---|---|

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
| T1 | Task title | todo | agent | | description |
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

- [ ] Run all verification commands from goal.md
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

pub fn inbox_md() -> &'static str {
    "# Inbox\n\nNo pending messages.\n\n<!-- External tools: append messages here. Do not edit status.md directly. -->\n"
}

pub fn outbox_md() -> &'static str {
    "# Outbox\n\nNo pending messages.\n\n<!-- Orchestrator writes messages here for external tools to pick up. -->\n"
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
