# Domesticates Internal Sparse Undo

## Status

Accepted

## 背景

`domesticates` の公開配列は sparse undo 化できているが、
`domesticates_internal` が変わる tick では full fallback していた。

`DomesticatesInternal` は `PartialEq + Clone` を持つ固定長配列中心の struct なので、
cell 単位の before-values patch に落とせる。

## 提案

- `DomesticatesUndoState` に `domesticates_internal` 用 sparse patch を追加する
- `domesticates` 全体 full fallback をやめ、内部状態変更も sparse patch で扱う

## 成功条件

- `domesticates_internal` 変更時でも `DomesticatesUndoState.full` に即フォールバックしない
- `application::world_` テストと build が通る
