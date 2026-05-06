# Undo From-Diff Predicate Refactor

## Status

Accepted

## 背景

`*_UndoState::from_diff` に `has_sparse_patch` 判定が複数あり、
`Option` の `is_some()` を長く列挙している。

## 目的

- `from_diff` の可読性を上げる
- 判定ロジックの重複を減らす

## 提案

- `any_patch!` macro を導入する
- `has_sparse_patch` 判定を macro 呼び出しへ置き換える

## 成功条件

- 挙動は不変
- `application::world_` テストが通る
