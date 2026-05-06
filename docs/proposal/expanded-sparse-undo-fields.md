# Expanded Sparse Undo Fields

## Status

Superseded by `../decisions/260503-expanded-sparse-undo-fields.md`

## 背景

現在の sparse undo は `geology.height`、`hydrology.river_flow`、`hydrology.river_next` に限定されている。
これでも前進だが、気候系と河川侵食系の大きな列はまだ subsystem 全体コピーへ落ちやすい。

## 目的

- sparse undo の適用範囲を広げる
- 気候と河川の主要な連続値列で full subsystem copy を減らす

## 提案概要

次を second stage の sparse undo 対象に追加する。

- `climate.temperature`
- `climate.precipitation`
- `hydrology.erosion_rate`
- `hydrology.deposition_rate`

対象 subsystem では、selected field 以外に変更がない tick は sparse patch だけを保存する。

## スコープ

- `ClimateUndoState` の追加
- `HydrologyUndoState` の selected field 拡張
- rewind の sparse patch 適用拡張

## 成功条件

- `temperature` / `precipitation` だけの変化で climate 全体コピーを避けられる
- `erosion_rate` / `deposition_rate` だけの変化で hydrology 全体コピーを避けられる
- 既存 rewind 等価性テストが維持される

## リスクとトレードオフ

- sparse patch 対象が増えるぶん、undo 適用分岐は増える
- ただし SoA の大きな連続値列から順に切り出す方が費用対効果は高い

## 実施計画

1. climate / hydrology の undo state を拡張する
2. finalize 時に selected field 専用比較を追加する
3. rewind 時に sparse patch を適用する
4. テストを追加更新する

## 未解決事項

- wind / runoff / aridity を次段で sparse 化するか
- glaciology を selected field 型へ移すか
