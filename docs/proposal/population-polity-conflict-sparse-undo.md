# Population Polity Conflict Sparse Undo

## Status

Accepted

## 背景

`population` / `settlement` / `polity` / `conflict` は
full clone fallback のままだったが、
保持しているのは連続値列か `Option<Id>` 列なので sparse patch 化しやすい。

## 提案

- `PopulationUndoState`
- `SettlementUndoState`
- `PolityUndoState`
- `ConflictUndoState`

を追加し、before-values sparse patch を使う。

## 成功条件

- 上記 subsystem が full clone ではなく sparse patch で巻き戻せる
- `application::world_` テストと build が通る
