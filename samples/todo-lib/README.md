# todo-lib sample

`orcha` の動作確認用に作成した最小サンプルです。  
単純な `TodoList` を提供し、ユニットテストで基本挙動を確認できます。

## 実行例

```powershell
cargo test --manifest-path samples/todo-lib/Cargo.toml
```

## ファイル

- `src/lib.rs`: `TodoList` と `TodoItem` の実装
- `.orcha/`: サンプル用オーケストレーション状態ファイル群
- `.orcha/orcha.yml`: orcha が実行時に参照する機械設定（検証コマンド/完了条件/エージェント設定）
  - `agents.local_llm_codex` / `agents.local_llm_claude` / `agents.local_llm_opencode` に具体設定を記載
  - CLI例では `model` を省略し、各CLIツール側の既定モデルを利用する構成を記載
  - `agents.local_llm_codex_manual_flags` で `ensure_no_permission_flags: false` の指定例を記載
  - `agents.local_llm_opencode_json` で `args` の差分指定例を記載
  - `agents.local_llm` と `execution` が実際に使われる設定
- `.orcha/agentworkspace/`: エージェント応答と状態ファイル（`status.md`, `status_log.md`）の保存先
- `.orcha/templates/roles/*_colloquial_ja.md`: 口語トーンの role サンプル

## 設定の使い分け例

1. `agents.local_llm` を CLI にする場合  
   `.orcha/orcha.yml` の `agents.local_llm_codex` / `agents.local_llm_claude` / `agents.local_llm_opencode` の中身を `agents.local_llm` にコピーします。
2. 実行戦略を変える場合  
   `.orcha/orcha.yml` の `execution` を直接編集します。
3. profile切替だけ細かく調整する場合  
   `execution.profile_strategy` の `alternating` / `every_n_cycles` / `mixins`（`offset` と `every_n_cycles` を含む）を編集して調整します。
