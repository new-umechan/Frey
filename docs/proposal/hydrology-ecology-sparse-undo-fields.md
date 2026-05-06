# Hydrology/Ecology Sparse Undo Fields

## Status

Accepted

## 背景

`geology` / `climate` / `glaciology` の主要連続値列は sparse undo へ寄せられたが、
`hydrology` の sink 系列と `ecology` はまだ subsystem 全体コピーへ落ちやすい。
特に `hydrology` は複数の大きな SoA 列を持ち、
`ecology` も `biome` と 4 本の連続値列が毎 tick 変化し得る。

## 目的

- `hydrology` の full copy fallback を sink 系列まで減らす
- `ecology` を mixed-type sparse undo へ移す
- rewind のメモリ量を state 全体コピーから field 単位へ寄せる

## 提案概要

次を next stage の sparse undo 対象に追加する。

- `hydrology.is_lake`
- `hydrology.sink_id`
- `hydrology.sink_route_next`
- `hydrology.sink_member_offsets`
- `hydrology.sink_member_cells`
- `hydrology.sink_spill_cell`
- `hydrology.sink_spill_to`
- `hydrology.sink_spill_level`
- `hydrology.sink_capacity_total`
- `hydrology.sink_capacity_remaining`
- `hydrology.sink_storage_water`
- `hydrology.sink_storage_sediment`
- `hydrology.sink_overflow_active`
- `ecology.biome`
- `ecology.tree_cover`
- `ecology.ground_cover`
- `ecology.disturbance`
- `ecology.soil_fertility`

`hydrology.river_downstream` と `ecology_internal` は可変長・複合構造なので、
引き続き full subsystem copy fallback の条件として扱う。

## スコープ

- `SparseBoolPatch` / `SparseU8Patch` / `SparseU32Patch` の追加
- `HydrologyUndoState` の selected field 拡張
- `EcologyUndoState` の追加
- finalize 時の sink 系 / ecology 圧縮
- rewind 時の sparse patch 適用
- architecture docs 更新

## 成功条件

- sink 系列だけの変化で hydrology 全体コピーを避けられる
- ecology の公開 state だけの変化で full copy を避けられる
- 既存 rewind 等価性テストが維持される

## リスクとトレードオフ

- patch 型が増えて適用分岐も増える
- ただし型ごとの helper にまとめれば重複は抑えられる

## 実施計画

1. mixed-type sparse patch 型を追加する
2. hydrology / ecology の undo state を拡張する
3. finalize / rewind の selected field 処理を追加する
4. tests と architecture docs を更新する
