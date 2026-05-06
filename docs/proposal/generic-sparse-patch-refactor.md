# Generic Sparse Patch Refactor

## Status

Accepted

## 背景

undo log の sparse patch は `f32` / `i32` / `u32` / `u8` / `bool` / `Biome`
ごとに別 struct と helper を持っている。
構造は同一で、違うのは `values` の要素型だけである。

## 目的

- sparse patch の型定義重複を減らす
- patch 生成と適用 helper を汎用化する
- `UndoState` の field 構造は維持したまま内部実装だけを整理する

## 提案概要

- `SparsePatch<T>` を導入する
- 既存の `SparseF32Patch` などは type alias に置き換える
- `sparse_*_patch` を `build_sparse_patch<T>` に統合する
- `apply_sparse_*_patch` を `apply_sparse_patch<T>` に統合する

## スコープ

- `world_runtime.rs` の sparse patch 定義と build helper
- `world_use_cases.rs` の patch 適用 helper

## 成功条件

- `UndoState` の public field 名を変えずにコンパイルが通る
- `application::world_` テストが維持される
