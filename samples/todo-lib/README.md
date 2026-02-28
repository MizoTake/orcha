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
