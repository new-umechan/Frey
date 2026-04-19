# Hydrology Sink Incremental Rebuild の採用

## Status

Accepted

## Context

Hydrology の sink 再構築は、`1tick=100ms` 目標に対する主要な性能ボトルネックだった。
従来の実装はフル再構築前提で、局所的な地形変化でも sink 全体を作り直していたため、計算量が大きかった。

## Decision

sink 再構築は次の3モードで運用する。

- `Full`
- `Incremental`
- `Skip`

`Full` はバッファ不整合、トポロジ検証失敗、または再構築間隔・変化率の閾値超過時に使う。
`Incremental` は地形変化の近傍と関連 sink のみを再計算する。
`Skip` は sink 正本が安定しており、追加再計算が不要な tick で使う。

制御パラメータは `GeologyParams` に置く。

- `sink_full_rebuild_interval_ticks`
- `sink_full_rebuild_changed_ratio`
- `sink_incremental_neighbor_hops`

## Rationale

- sink 容量・spill の正本は維持したまま、再計算範囲だけを局所化できる
- 変更が小さい tick では sink 全体の再構築を避けられる
- 失敗時は Full に戻せるため、近似導入のリスクを局所に閉じ込められる
- `recent_changed` を地形変化に限定することで、単なる水量変動で full 再構築に流れにくくできる

## Consequences

- `HydrologyMFDSystem` の実行コストが下がる
- perf 記録で `step_geology_river_sink_incremental_rebuild` を監視できる
- sink 周辺の局所近似が入るため、長期では微小差分が蓄積しうる
- その場合でも、閾値超過や検証失敗で Full にフォールバックする
