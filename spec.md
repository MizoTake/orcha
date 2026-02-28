# orcha SPEC (MVP)

## 1. Purpose

orcha は、特定のゴール達成まで複数のAIエージェント（local LLM / Codex / Claude / Gemini 等）を役割分担させ、サイクル型で実行・改善を行うオーケストレーションCLIである。

特徴:

* Markdown中心の状態管理（他ツールから可読）
* ローカルLLM中心で実行
* 節目のみ有料モデルへエスカレーション
* テンプレート選択による簡易設定
* 外部ツール（別モデル・別プロセス）とのファイル連携

---

## 2. Design Principles

### 2.1 Markdown as Source of Truth

すべての状態・意思決定は `.orcha/` 配下のMarkdownで管理する。

### 2.2 Local-first

基本処理はローカルLLMで実行し、以下の場合のみ有料モデルを使用する:

* セキュリティ/重要変更
* 大規模差分
* 連続失敗による詰まり

### 2.3 Template-driven Configuration

ユーザーは細かい設定を行わず、運用プロファイルを選択するのみ。

### 2.4 External Observability

他のAIツールは以下を読むことで状況を把握できる:

* goal.md
* status.md
* team.md
* roles/

---

## 3. Directory Structure

```
.orcha/
  goal.md
  team.md
  status.md
  status_log.md

  roles/
    planner.md
    implementer.md
    reviewer.md
    verifier.md
    scribe.md

  handoff/
    inbox.md
    outbox.md

  templates/
    profiles/
      local_only.md
      cheap_checkpoints.md
      quality_gate.md
      unblock_first.md
```

---

## 4. Goal Definition

### goal.md

必須項目:

* Background
* Acceptance Criteria (checkbox)
* Constraints
* Verification commands
* Quality priority (speed / cost / quality)

---

## 5. Team Definition

### team.md

記載内容:

* Team principles (例: Local-first)
* Members

  * local_llm
  * codex
  * claude
  * gemini
* 各メンバー:

  * Strength
  * Weakness
  * Cost level
  * Use when

---

## 6. Role Definition

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

---

## 7. Status Model (Single Source of Truth)

### status.md

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

## 8. Status Log

status_log.md は追記専用:

```
time [phase] role(agent): message
```

編集は禁止。

---

## 9. External Collaboration

### handoff/outbox.md

オーケストレータ → 外部モデル

### handoff/inbox.md

外部モデル → オーケストレータ

ルール:

* 外部ツールは status.md を直接編集しない
* inbox に追記する

---

## 10. Execution Cycle

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

## 11. Profiles (Operation Templates)

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

---

## 12. Gate Rules (MVP)

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

## 13. Agent Abstraction

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

## 14. CLI Commands

### orcha init

`.orcha/` をテンプレ生成

### orcha run

現在phaseから1ステップ実行

### orcha status

status.md をダッシュボード表示（カラー出力・タスクテーブル付き）

### orcha profile \<name\>

profile変更 (local_only / cheap_checkpoints / quality_gate / unblock_first)

### orcha explain

現在の意思決定理由を表示（プロファイルルール、ゲート状態、利用可能エージェント）

共通オプション:

* `--orcha-dir <PATH>` — .orcha ディレクトリのパス (default: `.orcha`)
* `-v, --verbose` — 詳細ログ出力

---

## 15. Health State

* green: verify pass
* yellow: warning / review issues
* red: verify fail / blocked

---

## 16. Stop Conditions

* cycle >= 5
* 同一失敗2回 + 有料不可
* profile=local_only で詰まり

---

## 17. Implementation

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

  cli/              # CLIコマンド (init, run, status, profile, explain)
  core/             # ドメインロジック (cycle, status, task, gate, profile, health)
  agent/            # Agent trait + 4バックエンド + verifier + router
  markdown/         # YAML frontmatter parser + テンプレート
  phase/            # 7フェーズ実装 (briefing〜decide)
  config/           # 環境変数ベース設定 (API key管理)
```

### 環境変数

| 変数名 | 用途 | デフォルト |
|---|---|---|
| LOCAL_LLM_ENDPOINT | ローカルLLMのAPIエンドポイント | http://localhost:11434/v1 |
| LOCAL_LLM_MODEL | ローカルLLMのモデル名 | llama3.2 |
| ANTHROPIC_API_KEY | Claude API key | (なし) |
| ANTHROPIC_MODEL | Claudeモデル名 | claude-sonnet-4-20250514 |
| GEMINI_API_KEY | Gemini API key | (なし) |
| GEMINI_MODEL | Geminiモデル名 | gemini-2.0-flash |
| OPENAI_API_KEY | OpenAI API key | (なし) |
| CODEX_MODEL | Codexモデル名 | gpt-4.1 |

---

## 18. Future Extensions (Out of Scope for MVP)

* GUI / TUI
* Parallel execution
* Custom YAML workflows
* Git worktree / auto PR
* Cost tracking per model
* Probabilistic routing
* Multi-agent parallel debate

---

## 19. Target Use Cases

* ローカルLLM主体の開発支援
* コスト制御付きAI開発ループ
* 複数AIの役割分担実験
* 外部AIとの手動/半自動協働
