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

force target は、生の drive proxy をそのまま `0-1` の速度係数とは扱わない。
runtime の `slab_pull_drive` / `ridge_push_drive` は plate 全体の境界 cell 平均であり、
mobile-lid として十分に駆動されている状態でも 1.0 には近づかない。
そのため、合成 drive を `EXPECTED_MOBILE_LID_DRIVE` で正規化してから
Earth reference speed に変換する。

速度の上昇は従来どおり比較的速く追従させ、減速はより緩やかにする。
また、plate velocity の実効速度から `activity` 乗算を外す。
`activity` は境界過程の強さであり、plate 全体の drift 速度を二重に減衰させる
係数としては使わない。

この判断を後から検証できるように、runtime `PlateKinematicsState` へ
drive diagnostics を保存する。
`crust_plate_count_series` は次を記録する。

- `mean_slab_pull_drive`
- `mean_ridge_push_drive`
- `mean_collision_drag`
- `mean_force_target_speed_km_per_myr`
- `mean_basal_target_speed_km_per_myr`

Frey では slab pull を主駆動、ridge push を副次駆動として読む。
そのため、ridge push が slab pull を継続的に上回る run は、
plate motion の駆動バランスを再確認する対象とする。

## Consequences

利点:

- late Crust で plate speed が `20-100 km/Myr` の現実的な帯に残りやすい
- 境界分類が一時的に passive へ寄っても、plate が停止したように見えにくい
- 初期 plate emergence が作った downwelling / plume bias を runtime へ保持できる

欠点:

- basal target は mantle convection を直接解くものではなく、初期 kinematics の proxy である
- level 6 では `mean_cell_crossing_fraction_per_tick` が 1 を超える場合があるため、
  boundary crossing は速度に応じて 1 tick を少数の substep に分ける
- `reference_angular_speed` は serialized runtime state に追加されるため、
  既存 snapshot から読む場合は serde default と現速度で補正する
