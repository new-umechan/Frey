# Selected Sparse Undo Fields

## Status

Superseded

## Replaced by

`../decisions/260503-selected-sparse-undo-fields.md`

## 背景

`TickUndoLog` は subsystem 粒度の before-values に圧縮されたが、
`geology` や `hydrology` が少しでも変わると subsystem 全体コピーになる。

逆再生時のメモリ効率を改善するには、更新頻度が高く列長も大きい値から
sparse diff へ落とす必要がある。

## 目的

- subsystem 全体コピーから部分的に脱却する
- 実装範囲を広げすぎず、効果の大きい列だけ先に sparse 化する

## 提案概要

- `geology.height`
- `hydrology.river_flow`
- `hydrology.river_next`

上記 3 列を first stage の sparse undo 対象にする。

対象 subsystem では、selected field 以外に変更がない tick は
full subsystem copy を持たず、`changed_indices + before_values` を保存する。

## スコープ

- `SparseF32Patch` / `SparseI32Patch` の追加
- `GeologyUndoState` / `HydrologyUndoState` の追加
- `rewind_world_by_ticks` の sparse patch 適用

今回は次をスコープ外とする。

- `climate` の sparse 化
- `entity` / `relations` の patch 化
- bool / enum / smallvec 系の sparse patch 一般化

## 成功条件

- `height` のみ変化する tick では `geology` 全体コピーを避けられる
- `river_flow` / `river_next` だけの変化でも `hydrology` 全体コピーを避けられる
- 既存の rewind 等価性テストが維持される

## リスクとトレードオフ

- subsystem 内で sparse 対象と full copy 対象が混在し、構造は少し複雑になる
- ただし最も大きい列から先に切り出すことで、段階的に効果を出せる

## 実施計画

1. sparse patch 型を追加する
2. geology / hydrology の undo state を full-or-sparse に分解する
3. finalize 時に selected field 専用比較を入れる
4. rewind とテストを更新する

## 未解決事項

- 次に sparse 化する列を `erosion_rate` / `deposition_rate` / `temperature` のどれにするか
