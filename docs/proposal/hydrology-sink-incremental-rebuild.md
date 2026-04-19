# Hydrology Sink Incremental Rebuild

## Status

Accepted

採用済み。実装と文書の正本は `docs/decisions/260420-hydrology-sink-incremental-rebuild.md` と `docs/reference/modules/hydrology.md` を参照する。

## 背景

- `1tick` の主要ボトルネックは hydrology の sink 再構築で、`step_geology_river_automaton_sink_ms` が大きい。
- 既存実装はフル再構築前提で、毎回 sink 全体を更新するため、局所変化でも計算量が増える。

## 目的

- 間引きに頼らず、sink 更新の計算量を局所化して `tick_total` を短縮する。
- 決定性を維持しつつ、微小差分は seed 回帰ゲート内で許容する。

## 提案概要

- sink 更新を `Full / Incremental / Skip` の3モードに分岐する。
- 既定は `Incremental` とし、`recent_changed` 近傍と関連 sink のみ再計算する。
- バッファ不整合やトポロジ検証失敗時は `Full` にフォールバックする。
- 閾値は `GeologyParams` で制御する。
    - `sink_full_rebuild_interval_ticks`
    - `sink_full_rebuild_changed_ratio`
    - `sink_incremental_neighbor_hops`

## 根拠

- goSPL の sink 容量モデル（sink 検出、容量・spill 管理、容量充足後に越流）と整合する。
- depression handling の系譜（Priority-Flood 系）で使われる「局所更新と必要時の全体再計算」方針を採る。

参考:

- Barnes et al., 2014, *Priority-Flood: An Optimal Depression-Filling and Watershed-Labeling Algorithm for Digital Elevation Models*.
- Salles et al., 2018, goSPL 関連論文群（landscape evolution with depression/sediment handling）。

## 成功条件

- 既存 perf 条件で sink 更新時間が減少する。
- seed regression の許容閾値内に収まる。
- 検証失敗時に full rebuild へ自動復帰し、破綻しない。

## リスクとトレードオフ

- Incremental は厳密な全体再計算より局所近似になるため、長期で微小差分が蓄積し得る。
- その代わり、計算コストは抑えられ、100ms 目標に向けた反復速度が上がる。

## 実装メモ

- `recent_changed` は地形高低の変化を基準に蓄積する。
- perf では `step_geology_river_sink_incremental_rebuild` と `step_geology_river_sink_full_rebuild` を観測する。
