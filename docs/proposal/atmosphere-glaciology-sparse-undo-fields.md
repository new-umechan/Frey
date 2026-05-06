# Atmosphere/Glaciology Sparse Undo Fields

## Status

Accepted

## 背景

second stage で `climate.temperature` / `precipitation` と
`hydrology.erosion_rate` / `deposition_rate` まで sparse undo を広げた。
しかし climate にはまだ多くの `Vec<f32>` が残っており、
glaciology も依然として subsystem 全体コピーに落ちる。

## 目的

- climate の full copy fallback をさらに減らす
- glaciology を subsystem copy から sparse patch 主体へ移す
- rewind のメモリ効率を SoA 構造に合わせて改善する

## 提案概要

次を third stage の sparse undo 対象に追加する。

- `climate.evapotranspiration`
- `climate.runoff`
- `climate.aridity`
- `climate.ocean_temperature`
- `climate.precipitable_water`
- `climate.cloud_water`
- `climate.wind_u`
- `climate.wind_v`
- `climate.moisture_flux_u`
- `climate.moisture_flux_v`
- `glaciology.ice_thickness`
- `glaciology.ice_load`
- `glaciology.accumulation`
- `glaciology.ablation`
- `glaciology.isostatic_adjustment`
- `glaciology.applied_isostatic_adjustment`
- `glaciology.glacial_erosion_rate`
- `glaciology.glacial_melt_runoff`

これらはすべて `Vec<f32>` なので、`indices + before_values` の sparse patch として
表現できる。対象 field を subsystem 全体で網羅できる場合は、
field 長不一致のような例外時を除き full subsystem copy を避ける。

## スコープ

- `ClimateUndoState` の selected field 拡張
- `GlaciologyUndoState` の追加
- finalize 時の climate / glaciology 圧縮更新
- rewind 時の sparse patch 適用拡張
- architecture docs の更新

## 成功条件

- climate が更新されても full copy へ落ちる頻度が大きく下がる
- glaciology 更新 tick で full copy をほぼ使わない
- 既存の rewind 等価性テストが維持される

## リスクとトレードオフ

- patch 適用分岐は増える
- 一方で、連続値列の大半は同じパターンで処理できるため実装複雑度は限定的

## 実施計画

1. climate / glaciology の undo state を追加拡張する
2. finalize 時に全 selected field を sparse patch 化する
3. rewind 時に sparse patch 適用 helper で復元する
4. tests と architecture docs を更新する
