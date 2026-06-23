# Regime-based damage-first plate emergence v1

## Status

Accepted

## Context

Damage-first 初期化では、`plate_count_min` / `plate_count_max` を目標として使うと、
plate 数を直接生成する設計へ戻りやすい。
一方で、完全に抽出結果を無制限に受け入れると、巨大な stagnant lid や破片化した shattered lid を
通常の mobile plate tectonics と誤認する。

また、boundary skeleton を最初から本格実装すると、球面グリッド上の幾何・トポロジー処理が
plate emergence 本体より支配的になる。

## Decision

第一段階では、plate 数を target として使わない。
Damage-first path では `plate_count_min` / `plate_count_max` を参照しない。
plate 数は observed result とし、抽出結果は次の regime で評価する。

- `stagnant_lid`: 境界 network が未発達で、巨大な蓋が残る
- `mobile_lid`: 複数の強い内部 block と弱い境界 network が成立する
- `shattered_lid`: 破壊されすぎて小片化する

Boundary extraction v1 は本格 skeleton ではなく、`boundary_potential` の adaptive threshold と
小さい boundary island の cleanup に留める。
`boundary_potential` は境界として抽出されやすい度合いであり、ridge/trench/transform/collision などの
boundary type ではない。

初期化で作った kinematics は runtime へ渡す。
名前は成熟した slab pull と区別し、初期段階では `subduction_tendency`、
plume/downwelling 由来の運動傾向は `plume_divergence_bias` /
`downwelling_convergence_bias` と呼ぶ。

Crust 期の活動は、`plate_id` の変更量ではなく、boundary activity、surface delta、
volcanism、crust age/density/thickness の更新を主指標にする。
`plate_id` churn は guardrail として計測する。

## Consequences

利点:

- plate 数 target への依存を外し、恣意性を下げる
- skeleton 実装の不安定さを第一段階へ持ち込まない
- initial kinematics が runtime rebuild で失われない
- `plate_id` churn を成功指標ではなく監視指標として扱える

欠点:

- `mobile_lid` にならない seed では proto fallback になりうる
- boundary は 1-cell skeleton ではないため、境界幅はまだ粗い
- split / merge / endpoint connection は別段階の課題として残る

## Follow-up

- Boundary segment length filter と tiny fragment handling を追加する
- local maximum skeleton と graph cleanup を段階導入する
- ownership transfer の churn guardrail を強化する
