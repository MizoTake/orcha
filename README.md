# orcha

Rust製のAIオーケストレーションCLIです。  
`.orcha/` 配下の **Markdown（LLM向け文脈）** と **YAML（実行設定）** を使い、`briefing -> plan -> impl -> review -> fix -> verify -> decide` のサイクルを、完了または停止条件まで自動で進めます。

## このプロダクトは何か

orcha は「ゴール達成までの開発ループを、役割付きエージェントで回し続けるCLI」です。  
人が毎回プロンプトを組み立てなくても、`.orcha/` の状態を更新しながら次のアクションを進めます。

- 入力:
  - `goal.md`（背景・制約）
  - `roles/*.md`（役割ごとの指示書）
  - `orcha.yml`（モデル呼び出し・完了条件・検証コマンド）
- 処理:
  - 7フェーズを順番に実行し、`status.md` / `status_log.md` を更新
  - `decide` フェーズで完了判定。未完了なら次サイクルへ
- 出力:
  - 実装・レビュー・検証の結果をMarkdownに記録（外部ツール連携しやすい形式）

具体例:
1. `goal.md` に「Todoライブラリを完成させる」を記載
2. `orcha.yml` の `execution.verification.commands` に `cargo test` を設定
3. `orcha run` で、完了条件を満たすまでサイクルを継続

## 前提環境

- Rust（`cargo` が使えること）
- PowerShell（このリポジトリの補助スクリプトは `scripts/*.ps1`）
- `run` フェーズを実行する場合のみ、以下のどちらか
  - OpenAI互換のローカルLLMエンドポイント（`mode: "http"`）
  - ローカルCLIコマンド（`mode: "cli"`）

## クイックスタート

```powershell
./scripts/build.ps1
./scripts/test.ps1
./scripts/run.ps1 init
./scripts/run.ps1 status
```

`run` を実行する場合:

```powershell
./scripts/run.ps1 run
```

`run` は `Decide` フェーズで完了判定し、未完了なら次サイクルへ進みます。

`orcha init` 後は以下のサンプルテンプレートをそのまま参照できます。
- `.orcha/templates/roles/*.md`
- `.orcha/templates/profiles/*.md`

## 設定方針（重要）

- LLMに読ませる文脈: `goal.md`, `roles/*.md`, `status.md`（Markdown）
- orchaが実行判断に使う設定: `.orcha/orcha.yml`（YAML）

`orcha.yml` では以下を定義します。
- `agents.*`: エージェント呼び出し設定（endpoint/model/api_key_env）
- `execution.profile`: 実行時のプロファイル（`local_only` / `cheap_checkpoints` / `quality_gate` / `unblock_first`）
- `execution.acceptance_criteria`: 完了判定の基準
- `execution.verification.commands`: `verify` フェーズで実行するコマンド

### local_llm をCLI直実行する例

```yaml
agents:
  local_llm:
    mode: "cli"
    model: "gpt-oss-20b"
    cli:
      command: "opencode"
      args: ["chat", "--format", "markdown"]
      prompt_via_stdin: true
      model_arg: "--model"
      ensure_no_permission_flags: true
```

`mode: "http"` の場合は `endpoint` を使い、`mode: "cli"` の場合は `cli.command` を直接実行します。
`ensure_no_permission_flags: true` の場合、`command` が `codex` / `claude` なら以下を自動付与します。
- `codex`: `--ask-for-approval never`（未指定時）
- `claude`: `--dangerously-skip-permissions`（未指定時）

## scripts

- `scripts/build.ps1`
  - 目的: ビルド
  - 例: `./scripts/build.ps1` / `./scripts/build.ps1 -Release`
- `scripts/test.ps1`
  - 目的: 検証（`cargo check` + `cargo test --lib`）
  - 例: `./scripts/test.ps1` / `./scripts/test.ps1 -SkipCheck`
- `scripts/run.ps1`
  - 目的: CLIラッパー（`--orch-dir` を含めて実行）
  - 例:
    - `./scripts/run.ps1 init`
    - `./scripts/run.ps1 status -OrchDir target/demo`
    - `./scripts/run.ps1 profile -ProfileName quality_gate`
    - `./scripts/run.ps1 run`

## 主要コマンド（直接実行）

```powershell
cargo run -- --help
cargo run -- init
cargo run -- status
cargo run -- run
cargo test --lib
```

## 環境変数（必要に応じて）

- APIキーは `orcha.yml` の `agents.*.api_key_env` で指定した環境変数名から読み取ります。
- 既定テンプレートでは以下を利用します:
  - `ANTHROPIC_API_KEY`
  - `GEMINI_API_KEY`
  - `OPENAI_API_KEY`

`LOCAL_LLM` の endpoint/model は環境変数ではなく `orcha.yml` 側で定義します。

`.env` が存在すれば起動時に読み込みます。

## よくあるエラー

- `Local LLM HTTP 404 ... model 'llama3.2' not found`
  - 原因: ローカルLLMに対象モデルが無い
  - 対処: モデルを用意するか、`.orcha/orcha.yml` の `agents.local_llm.model` を利用可能なモデル名に変更

## ディレクトリ構成（抜粋）

```text
src/
  cli/      # コマンド実装
  core/     # 状態・サイクル・ゲートなどのドメイン
  agent/    # エージェント実装とルーティング
  phase/    # 7フェーズ実装
  markdown/ # frontmatter処理とテンプレート
scripts/    # build/test/run 補助スクリプト
```
