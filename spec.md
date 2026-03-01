# orcha SPEC (MVP)

## 1. Purpose

orcha は、特定のゴール達成まで複数のAIエージェント（local LLM / Codex / Claude / Gemini 等）を役割分担させ、サイクル型で実行・改善を行うオーケストレーションCLIである。

特徴:

* Markdown + YAML の分離管理（文脈と実行設定を分離）
* ローカルLLM中心で実行
* 節目のみ有料モデルへエスカレーション
* テンプレート選択による簡易設定
* 外部ツール（別モデル・別プロセス）とのファイル連携

---

## 2. Design Principles

### 2.1 Dual Source of Truth

`.orcha/` 配下を以下の責務で分離する。

* Markdown: LLMが読む文脈・状態 (`goal.md`, `roles/*.md`, `agentworkspace/status.md`, `agentworkspace/status_log.md`)
* YAML: オーケストレータが実行判断に使う設定 (`orcha.yml`)

### 2.2 Local-first

基本処理はローカルLLMで実行し、以下の場合のみ有料モデルを使用する:

* セキュリティ/重要変更
* 大規模差分
* 連続失敗による詰まり

### 2.3 Template-driven Configuration

`orcha init` でテンプレートを生成し、必要箇所のみ編集して運用を開始できること。

### 2.4 External Observability

他のAIツールは以下を読むことで状況を把握できる:

* goal.md
* agentworkspace/status.md
* roles/

---

## 3. Directory Structure

```
.orcha/
  goal.md
  orcha.yml
  agentworkspace/
    status.md
    status_log.md

  roles/
    planner.md
    implementer.md
    reviewer.md
    verifier.md
    scribe.md
    samples/
      *.md

  profiles/
    local_only.md
    cheap_checkpoints.md
    quality_gate.md
    unblock_first.md

  handoff/
    inbox.md
    outbox.md
```

実行時の参照方針:
* `profiles/roles/handoff` は固定ファイル名ではなく、実際に存在する `.md` を基準に解決する。
* 同名の組み込み定義が存在しても、該当 `.md` があればその内容を優先する。

---

## 4. Goal Definition

### goal.md

必須項目:

* Background
* Acceptance Criteria (checkbox)
* Constraints
* Quality priority (speed / cost / quality)

※ Verification commands と実行用 acceptance criteria は `orcha.yml` に定義する。

### 4.1 orcha.yml (Machine Config)

主な項目:

* `agents.local_llm.mode`: `http` / `cli`
* `agents.local_llm.endpoint`, `agents.local_llm.model`
* `agents.local_llm.cli.command`, `args`, `prompt_via_stdin`, `model_arg`, `timeout_seconds`
* `agents.local_llm.cli.ensure_no_permission_flags`
* `agents.{claude,gemini,codex}.api_key_env`, `model`
* `execution.profile`（組み込み: `local_only` / `cheap_checkpoints` / `quality_gate` / `unblock_first` / `opencode_impl_no_review` / `opencode_impl_claude_review` / `opencode_impl_codex_review` / `claude_impl_opencode_review` / `codex_impl_opencode_review`。加えて `.orcha/profiles/<name>.md` があれば任意名を指定可能）
* `execution.profile_strategy.alternating`
* `execution.profile_strategy.every_n_cycles`
* `execution.profile_strategy.mixins` (`fields`: `default_agent` / `review_agent` / `escalation` / `security_gate` / `size_gate`)
* `execution.cli_limit.disable_agent_on_limit` (既定 `true`。`true` で claude/codex CLI が limit/quota エラー後に同一 run 中で無効化)
* `execution.max_cycles` (run全体の最大サイクル数)
* `execution.phase_timeout_seconds` (各phaseタイムアウト秒)
* `execution.max_consecutive_verify_failures` (verify連続失敗で停止)
* `execution.acceptance_criteria`
* `execution.verification.commands`

補足:
* `execution.profile` / `execution.profile_strategy` の profile 名は、実行時に `.orcha/profiles/<name>.md` を参照する。
* 組み込み profile 名は既定ルールのフォールバックであり、同名ファイルがある場合はファイル記述を優先する。
* 実行イベントは `.orcha/agentworkspace/events.jsonl` にJSON Lines形式で追記する。

`ensure_no_permission_flags: true` の場合:

* `command=codex` では `--ask-for-approval never` を自動付与（未指定時）
* `command=claude` 系では `--dangerously-skip-permissions` を自動付与（未指定時）
* `command=opencode` では `OPENCODE_PERMISSION={"*":"allow","doom_loop":"allow"}` を自動付与

プロファイル戦略例:

* alternating: cycleごとに `cheap_checkpoints` と `quality_gate` を交互適用
* every_n_cycles: 3サイクルごとに `unblock_first` を適用
* mixins: `quality_gate` の `review_agent` と `security_gate` だけを合成

後方互換:

* `agents.claude` 未指定時は `agents.anthropic` を自動割り当て
* `agents.codex` 未指定時は `agents.openai` を自動割り当て

---

## 5. Role Definition

roles/*.md は以下を含む:

* Mission
* Checklist
* Output format

特に reviewer は以下を出力する:

```
Findings: High / Med / Low
Must-fix
paid_review_required: yes/no
reason
```

エージェント応答は `.orcha/agentworkspace/` にMarkdownで保存される。

---

## 6. Status Model (Single Source of Truth)

### agentworkspace/status.md

#### Frontmatter (machine readable)

```yaml
---
run_id:
profile:
cycle:
phase:
last_update:
budget:
  paid_calls_used:
  paid_calls_limit:
locks:
  writer:
  active_task:
---
```

#### Dashboard Sections

* Goal (one-line summary)
* Overall (cycle / phase / health)
* Blocking
* Next actions
* Escalation rules
* Task table
* Latest notes

### Task Table (fixed columns)

| ID | Title | State | Owner | Evidence | Notes |

State:

* todo
* doing
* done
* blocked

---

## 7. Status Log

agentworkspace/status_log.md は追記専用:

```
time [phase] role(agent): message
```

編集は禁止。

---

## 8. External Collaboration

### handoff/outbox.md

オーケストレータ → 外部モデル

### handoff/inbox.md

外部モデル → オーケストレータ

ルール:

* 外部ツールは agentworkspace/status.md を直接編集しない
* inbox に追記する

---

## 9. Execution Cycle

固定サイクル:

1. briefing
2. plan
3. impl
4. review
5. fix
6. verify
7. decide

最大 cycle: 5

同一エラー2回:
→ unblock gate

---

## 10. Profiles (Operation Templates)

### local_only

* 全処理ローカル
* 詰まり時は blocked

### cheap_checkpoints (default)

* 通常: local
* PR前: Claude review
* 失敗2回: Codex

### quality_gate

* auth / crypto / public API → Claude必須
* diff > threshold → Claude

### unblock_first

* verify失敗1回 → Codex
* 継続失敗 → Claude診断

### opencode_impl_no_review

* local_llm(opencode) のみを使用
* 詰まり時は blocked

### opencode_impl_claude_review

* 通常: local_llm(opencode)
* review/失敗時エスカレーション: Claude

### opencode_impl_codex_review

* 通常: local_llm(opencode)
* review/失敗時エスカレーション: Codex

### claude_impl_opencode_review

* 通常実装: Claude
* review: local_llm(opencode)

### codex_impl_opencode_review

* 通常実装: Codex
* review: local_llm(opencode)

---

## 11. Gate Rules (MVP)

### Security Gate

キーワード:
auth, crypto, security, public api
→ Claude review

### Unblock Gate

verify失敗 >= 2
→ Codex implement

### Size Gate

diff_lines > 400
→ paid review推奨

---

## 12. Agent Abstraction

MVPインターフェース (Rust trait):

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    async fn respond(&self, context: &AgentContext) -> Result<AgentResponse>;
    fn kind(&self) -> AgentKind;
}
```

Verifier:

```rust
pub async fn run(commands: &[String]) -> Result<VerifyResult>;
```

対応バックエンド:

* **LocalLlm** — OpenAI互換API (Ollama, LM Studio 等)
* **Anthropic** — Claude API
* **Gemini** — Google Gemini API
* **Codex** — OpenAI API

外部モデルは handoff で代替可能。

---

## 13. CLI Commands

### orcha init

`.orcha/` をテンプレ生成

### orcha run

現在状態から開始し、`Done` または停止条件に達するまでフェーズ/サイクルを継続実行

### orcha status

agentworkspace/status.md をダッシュボード表示（カラー出力・タスクテーブル付き）

### orcha profile \<name\>

profile変更 (local_only / cheap_checkpoints / quality_gate / unblock_first / opencode_impl_no_review / opencode_impl_claude_review / opencode_impl_codex_review / claude_impl_opencode_review / codex_impl_opencode_review)  
`agentworkspace/status.md` と `orcha.yml.execution.profile` の両方を更新する

### orcha explain

現在の意思決定理由を表示（プロファイルルール、ゲート状態、利用可能エージェント）

共通オプション:

* `--orcha-dir <PATH>` — .orcha ディレクトリのパス (default: `.orcha`)
* `-v, --verbose` — 詳細ログ出力

---

## 14. Health State

* green: verify pass
* yellow: warning / review issues
* red: verify fail / blocked

---

## 15. Stop Conditions

* cycle >= 5
* 同一失敗2回 + 有料不可
* profile=local_only で詰まり

---

## 16. Implementation

### 技術スタック

* **言語**: Rust (edition 2021)
* **CLI**: clap 4 (derive)
* **非同期**: tokio
* **HTTP**: reqwest (LLM API呼び出し)
* **シリアライズ**: serde / serde_json / serde_yaml
* **エラー**: anyhow + thiserror

### モジュール構成

```
src/
  main.rs           # エントリポイント
  lib.rs            # モジュール定義
  machine_config.rs # .orcha/orcha.yml の定義と読み込み

  cli/              # CLIコマンド (init, run, status, profile, explain)
  core/             # ドメインロジック (cycle, status, task, gate, profile, health)
  agent/            # Agent trait + 4バックエンド + verifier + router
  markdown/         # YAML frontmatter parser + テンプレート
  phase/            # 7フェーズ実装 (briefing〜decide)
  config/           # 実行設定の集約（orcha.yml + API key環境変数）
```

### 環境変数

`run` / `explain` は `.orcha/orcha.yml` を読み込む。  
環境変数は主に API キー解決に使い、変数名そのものは `orcha.yml` の `agents.*.api_key_env` で指定する。

| 変数名 | 用途 | デフォルト |
|---|---|---|
| ANTHROPIC_API_KEY | Claude API key（既定テンプレート） | (なし) |
| GEMINI_API_KEY | Gemini API key（既定テンプレート） | (なし) |
| OPENAI_API_KEY | OpenAI API key（既定テンプレート） | (なし) |

---

## 17. Future Extensions (Out of Scope for MVP)

* GUI / TUI
* Parallel execution
* Custom YAML workflows
* Git worktree / auto PR
* Cost tracking per model
* Probabilistic routing
* Multi-agent parallel debate

---

## 18. Target Use Cases

* ローカルLLM主体の開発支援
* コスト制御付きAI開発ループ
* 複数AIの役割分担実験
* 外部AIとの手動/半自動協働
