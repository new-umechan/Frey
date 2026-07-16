# Plate ownership Euler front advection

## Status

Superseded

## Context

Superseded by `260708-plate-ownership-influence-field.md`. The implementation remains available as
experimental ownership model `0`, but is no longer the default.

Crust runtime は `PlateKinematicsState` に Euler 的な angular axis / speed を持つ一方、
`plate_id` は境界 cell の stochastic takeover で更新していた。
このため velocity alignment が正でも、ownership field は局所的な取り合いを起こし、
500万年/tick の時間スケールに対して front として進まず斑点状に見える場合がある。

`seed=alpha` の tick 160 付近では、plate #3 の見た目だけでなく、
`persistent_boundary_complexity_growth` や boundary transfer spatial coherence が
runtime 中の形状劣化を示していた。

## Decision

`legacy_takeover` は削除し、runtime の plate ownership 更新を
`euler_front_advection` に一本化する。

`euler_front_advection` は全 cell の global remap ではなく、v1 では既存 mesh adjacency 上の
boundary front を進める。

- boundary edge ごとに source / target plate の Euler velocity を計算する
- `target - source` の相対速度が source cell へ向く場合だけ candidate にする
- candidate を `source_plate + target_plate + coarse spatial bucket` ごとの
  connected component にまとめる
- component 内の `relative_inflow / edge_spacing` を cell ごとに足し、
  `2 * sqrt(front_size)` で cap して expected transfer cell 数として使う
- component key ごとに 1 cell 未満の residual cell fraction を次 tick へ持ち越す
- tick 全体の global transfer cap は持たず、component budget を plate-level consistency
  projection で縮小する
- stochastic hash ではなく、score の高い cell から contiguous patch を作って transfer する
- patch が source plate を分断する、または target plate から孤立する場合は commit しない
- donor plate が極小化する transfer は拒否する

比較検証後は mode switch を残さない。
runtime の単一路線として扱い、validation は current implementation の
multi-seed gate と Earth shape baseline で行う。

## Consequences

利点:

- ownership 更新が Euler velocity field に直接従う
- stochastic な単発 takeover より front としてまとまりやすい
- mode switch がないため、precompute / preview / bench で古い ownership path を
  誤って使わない

欠点:

- material/tracer advection ではないため、plate interior の剛体移動を完全には再構成しない
- residual は coarse bucket key が再出現した場合だけ効くため、細かい front identity の
  完全な追跡ではない
- plate-level throughput cap は近似的な数値安定化であり、
  現実の plate boundary migration rate そのものではない
- split / merge / microplate lifecycle は扱わず、既存 plate count 維持を前提にする
- legacy との runtime 比較は commit history と過去 artifact に限られる

## Validation

主に次を current implementation の multi-seed gate で監視する。

- `persistent_boundary_complexity_growth`
- `boundary_complexity_growth`
- `area_delta_ratio_per_sample`
- `area_growth_from_initial`
- `max_plate_area_growth_from_initial`
- `enclosed_plate_risk`
- `appendage_isolation_risk`
- `boundary_transfer_largest_component_ratio`
- `boundary_transfer_isolated_cell_ratio`
- `boundary_motion_response_ratio`
- `boundary_motion_underactive_risk`
- `boundary_motion_overactive_risk`
- `boundary_motion_runtime_raw_expected_cell_count`
- `boundary_motion_runtime_accumulated_expected_cell_count`
- `boundary_motion_runtime_component_budget_cell_count`
- `boundary_motion_runtime_transferable_component_budget_cell_count`
- `boundary_motion_runtime_plate_consistency_budget_cell_count`
- `boundary_motion_runtime_plate_consistency_deferred_cell_count`
- `boundary_motion_runtime_plate_consistency_donor_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_outgoing_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_incoming_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_net_area_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_max_projected_out_ratio`
- `boundary_motion_runtime_actual_transfer_cell_count`
- `boundary_motion_runtime_patch_rejected_component_count`
- `boundary_motion_runtime_patch_rejected_budget_cell_count`
- `boundary_motion_runtime_source_fragment_rejected_budget_cell_count`
- `boundary_motion_runtime_target_disconnected_rejected_budget_cell_count`
- `boundary_motion_runtime_budget_utilization_ratio`
- `boundary_motion_runtime_plate_consistency_limited_ratio`
- `boundary_motion_runtime_component_limited_ratio`
- `mean_euler_rotation_residual_ratio`
- `reciprocal_churn_ratio`

`pnpm bench:run:plate-ownership-series` は複数 seed で current implementation を実行し、
seed ごとの summary を出す。
`boundary_motion_runtime_*` は sample 間 response の fail/warn 原因を、
raw expected、residual accumulation、component cap、donor guard、patch guard、
plate-level consistency projection、actual transfer に
分解するための診断値である。
v1 の warning threshold は `max_plate_area_growth_from_initial <= 2.0`、
`max_abs_plate_area_delta_ratio <= 0.05`、`max_enclosed_plate_risk <= 0.8`、
`persistent_boundary_complexity_growth_plate_ratio <= 0.01`、かつ
`max_boundary_complexity_growth <= 1.25`、`boundary_motion_underactive_risk == 0`、
`boundary_motion_overactive_risk == 0` とする。
初期 plate selection は、postprocess で enclosed plate を吸収するのではなく、
やや多めの plate 数を許容し、`max_enclosed_plate_risk` が低い候補を優先する。
これは plate 数を保ったまま、閉じ込められた小 plate を避けるためである。

Earth/GPlates は静的 shape baseline として corridor / thin / core 指標に使い、
runtime persistence 指標とは直接比較しない。
