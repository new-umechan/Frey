# Undo State Apply Methods

## Status

Accepted

## 背景

`SparsePatch<T>` は汎用化されたが、`apply_core_change_set` には
subsystem ごとの field 列挙がまだ大量に残っている。

## 目的

- sparse undo の適用責務を `UndoState` 側へ寄せる
- use case 層の field 列挙を減らす
- subsystem ごとの undo 構造を局所化する

## 提案概要

- `GeologyUndoState` / `ClimateUndoState` / `GlaciologyUndoState` /
  `HydrologyUndoState` / `EcologyUndoState` に `apply_to` を追加する
- generic `apply_sparse_patch` helper は runtime 側に置く
- `apply_core_change_set` は subsystem 単位の呼び出しに薄くする

## 成功条件

- `application::world_` テストが維持される
- `apply_core_change_set` から subsystem 内部の field 列挙が大きく減る
