# Plate ownership local flux projection

## Status

Superseded

## Context

Superseded by `260708-plate-ownership-influence-field.md`.

`euler_front_advection` は boundary component ごとの expected transfer を計算しているが、
tick 全体の global cap が主制御になっていた。
このため shape guard は通るのに `actual_transfer` が world-level quota で律速し、
5 Myr/tick の boundary motion response が不足していた。

Frey は full geodynamic solver ではなく、mesh 上の軽量な plate-boundary evolution を扱う。
ただし boundary を component ごとに独立更新すると、plate 全体の面積収支や剛体運動との
整合が崩れる。

## Decision

global transfer cap は削除する。
boundary component ごとの local flux proposal を作り、plate-level consistency projection で
縮小してから contiguous patch として commit する。
candidate は undirected mesh edge ごとに signed normal flux を一度だけ評価し、
同じ edge から両側の takeover proposal が同時に出ないようにする。

v1 の projection は次だけを扱う。

- source / target plate ごとの throughput cap
- plate ごとの net area delta cap
- donor floor
- source plate を分断しない topology guard
- target plate から孤立しない topology guard
- 1 cell 未満の residual carry-over

throughput cap は plate cell count から決める conservative cap とし、
world 全体の transfer quota は持たない。
net area delta cap は throughput cap より小さくし、1 tick で plate 面積が
一方向に急変する proposal を縮小する。

`plate block`、weak-line split/merge、landmass lifecycle は runtime 更新則には入れず、
当面は validation に限定する。

## Consequences

利点:

- world-level quota ではなく local boundary flux を主制御にできる
- plate ごとの過剰な削れ/増加を抑えられる
- 既存の front component、fractional accumulator、patch guard を再利用できる

欠点:

- material/tracer advection ではないため、plate interior の連続移流ではない
- throughput cap は近似的な stability projection であり、物理量そのものではない
- split/merge lifecycle はまだ表現しない

## Validation

alpha/beta/gamma/delta の 160 tick run で次を確認する。

- `max_plate_block_count == 1`
- `persistent_boundary_complexity_growth_plate_ratio == 0`
- `max_boundary_complexity_growth <= 1.25`
- `max_abs_plate_area_delta_ratio <= 0.05`
- `boundary_motion_runtime_plate_consistency_budget_cell_count`
- `boundary_motion_runtime_plate_consistency_deferred_cell_count`
- `boundary_motion_runtime_plate_consistency_donor_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_outgoing_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_incoming_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_net_area_limited_cell_count`
- `boundary_motion_runtime_plate_consistency_max_projected_out_ratio`
- `boundary_motion_runtime_actual_transfer_cell_count`
- `boundary_motion_runtime_patch_rejected_budget_cell_count`
- `boundary_motion_response_ratio`
