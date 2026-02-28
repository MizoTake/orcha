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
- `.orcha/agentworkspace/`: エージェント応答と状態ファイル（`status.md`, `status_log.md`）の保存先
- `.orcha/templates/configs/profile-patterns.yml`: profile/profile_strategy の指定パターンサンプル
- `.orcha/templates/configs/local-llm-cli-codex.yml`: `codex` を CLI 実行する設定サンプル
- `.orcha/templates/roles/*_colloquial_ja.md`: 口語トーンの role サンプル
