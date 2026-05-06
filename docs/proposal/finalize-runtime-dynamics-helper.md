# Finalize Runtime Dynamics Helper

## Status

Accepted

## 背景

`finalize_tick_undo_log` は core 側の small-struct 差分は helper 化されたが、
`hydrology_dynamics` / `geology_dynamics` / `applied_intervention_seq` は
まだ個別の記述になっている。

## 目的

- runtime 補助状態の差分記録を共通化する
- `finalize_tick_undo_log` の末尾をさらに簡潔にする

## 提案

- `record_runtime_optional_change_if_different` helper を追加する
- runtime 補助状態 3 種の差分記録に適用する

## 成功条件

- 挙動は不変
- `application::world_` テストが通る
