# `from_diff` の sparse 判定を `any_patch!` に統一する

## Status

Accepted

## Context

`from_diff` メソッド群は `Option` の `is_some()` 判定を個別に列挙していた。
この重複は保守時の見落としリスクを上げる。

## Decision

- `world_runtime` に `any_patch!` macro を追加する
- `has_sparse_patch` は macro で統一して記述する

## Consequences

利点:

- 判定ロジックが読みやすくなる
- フィールド追加時の更新箇所が明確になる

コスト:

- macro への理解が必要
