# Basal plate motion floor

## Status

Accepted

## Context

Crust runtime の plate motion は、boundary classification から得た
slab convergence / rollback / ridge activity を使って速度を更新している。
しかし `crust_plate_count_series` の motion 指標では、alpha level 6 の 80 tick run で
tick 40 以降の `mean_plate_speed_km_per_myr` が `0.1 km/Myr` 前後まで落ちた。

現実の plate motion は典型的に `20-100 km/Myr`、高速側で
`100-150 km/Myr` 程度である。
`5 Myr/tick` の Crust では、late runtime が `1 km/Myr` 未満へ落ちる状態は
時間スケールに対して遅すぎる。

原因は、runtime kinematics が観測された boundary drive だけへ毎 tick 追従し、
boundary drive が弱まると初期 plate field が持っていた mantle-scale drift まで
失っていたことにある。

## Decision

`PlateKinematicsState` に `reference_angular_speed` を持たせる。
これは初期 plate kinematics 由来の basal motion proxy であり、
boundary drive が一時的に弱い場合でも plate speed がその一定割合より下へ
急落しないように使う。

`update_plate_kinematics` は次の 2 つの target の大きい方へ緩和する。

- slab pull / rollback / ridge push / collision drag から求める force target
- `reference_angular_speed` 由来の basal target

速度の上昇は従来どおり比較的速く追従させ、減速はより緩やかにする。
また、plate velocity の実効速度から `activity` 乗算を外す。
`activity` は境界過程の強さであり、plate 全体の drift 速度を二重に減衰させる
係数としては使わない。

## Consequences

利点:

- late Crust で plate speed が `20-100 km/Myr` の現実的な帯に残りやすい
- 境界分類が一時的に passive へ寄っても、plate が停止したように見えにくい
- 初期 plate emergence が作った downwelling / plume bias を runtime へ保持できる

欠点:

- basal target は mantle convection を直接解くものではなく、初期 kinematics の proxy である
- level 6 では `mean_cell_crossing_fraction_per_tick` が 1 を超える場合があり、
  複数セル相当の displacement を boundary crossing がどう扱うかは別途検証が必要
- `reference_angular_speed` は serialized runtime state に追加されるため、
  既存 snapshot から読む場合は serde default と現速度で補正する
