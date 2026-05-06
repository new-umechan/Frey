# Clock Control Undo State

## Status

Accepted

## 背景

`clock` と `control` は配列ではないが、
`WorldCoreChangeSet` では raw clone をそのまま保持していた。

`clock` は field 単位で before-values を持てる。
`control` も `geology_params` 以外は scalar なので、
small-state 用の undo state に揃えられる。

## 提案

- `ClockUndoState` を追加し、tick/era/budgets/transition を before-values で持つ
- `ControlUndoState` を追加し、`geology_params` 変更時だけ full fallback、
  それ以外は scalar before-values を持つ

## 成功条件

- `WorldCoreChangeSet.clock/control` が raw clone ではなく undo state になる
- `application::world_` テストと build が通る
