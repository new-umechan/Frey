# Plate ownership Euler front advection

## Status

Accepted

## Context

Crust runtime は `PlateKinematicsState` に Euler 的な angular axis / speed を持つ一方、
`plate_id` は境界 cell の stochastic takeover で更新していた。
このため velocity alignment が正でも、ownership field は局所的な取り合いを起こし、
500万年/tick の時間スケールに対して front として進まず斑点状に見える場合がある。

`seed=alpha` の tick 160 付近では、plate #3 の見た目だけでなく、
`persistent_boundary_complexity_growth` や boundary transfer spatial coherence が
runtime 中の形状劣化を示していた。

## Decision

既存の `legacy_takeover` を default として残しつつ、
`plate_ownership_mode = 1` に `euler_front_advection` を追加する。

`euler_front_advection` は全 cell の global remap ではなく、v1 では既存 mesh adjacency 上の
boundary front を進める。

- boundary edge ごとに source / target plate の Euler velocity を計算する
- `target - source` の相対速度が source cell へ向く場合だけ candidate にする
- candidate を target plate ごとの connected component にまとめる
- component の平均 `relative_inflow / edge_spacing` を component span の平方根で scaling し、
  expected transfer cell 数として使う
- tick 全体の transfer 数は mesh size 由来の CFL 予算で cap する
- stochastic hash ではなく score 順に deterministic に transfer する
- donor plate が極小化する transfer は拒否する

validation 用に `CRUST_PLATE_SERIES_OWNERSHIP_MODE=legacy|euler_front` を追加し、
同じ seed/tick/record interval で比較できるようにする。

## Consequences

利点:

- ownership 更新が Euler velocity field に直接従う
- stochastic な単発 takeover より front としてまとまりやすい
- 既存 mode を残すため、validation で比較してから default 化できる

欠点:

- global nearest-cell remap ではないため、plate interior の剛体移動を完全には再構成しない
- deterministic budget は sub-cell fraction の履歴を持たないため、低速 front は丸めの影響を受ける
- CFL 予算は近似的な数値安定化であり、現実の plate boundary migration rate そのものではない
- split / merge / microplate lifecycle は扱わず、既存 plate count 維持を前提にする

## Validation

主に次を legacy と euler front で比較する。

- `persistent_boundary_complexity_growth`
- `boundary_complexity_growth`
- `area_delta_ratio_per_sample`
- `area_growth_from_initial`
- `max_plate_area_growth_from_initial`
- `enclosed_plate_risk`
- `appendage_isolation_risk`
- `boundary_transfer_largest_component_ratio`
- `boundary_transfer_isolated_cell_ratio`
- `mean_euler_rotation_residual_ratio`
- `reciprocal_churn_ratio`

`pnpm bench:compare:plate-ownership` は legacy / candidate の
`crust_plate_count_series` JSONL を読み、同じ tick / plate id でこれらの指標を比較する。
`pnpm bench:run:plate-ownership-series` は複数 seed で legacy と euler front を実行し、
seed ごとの summary を出す。
v1 の warning threshold は `max_plate_area_growth_from_initial <= 2.0`、
`max_abs_plate_area_delta_ratio <= 0.05`、`max_enclosed_plate_risk <= 0.8`、かつ
persistent complexity ratio と max boundary complexity が legacy 以下であることとする。
初期 plate selection は、postprocess で enclosed plate を吸収するのではなく、
やや多めの plate 数を許容し、`max_enclosed_plate_risk` が低い候補を優先する。
これは plate 数を保ったまま、閉じ込められた小 plate を避けるためである。

Earth/GPlates は静的 shape baseline として corridor / thin / core 指標に使い、
runtime persistence 指標とは直接比較しない。
