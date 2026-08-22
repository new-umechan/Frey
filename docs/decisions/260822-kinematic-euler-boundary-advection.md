# Kinematic Euler boundary advection

## Status

Accepted

## Context

旧 ownership 更新は隣接 plate の相対速度を、その tick の raster edge 法線へ射影して移動方向にした。
相対速度は収束・発散・横ずれを表すが、地球固定座標系での共有境界の絶対移動を決めない。
さらに cell transfer 後の法線変化が同じ物理運動を逆向き候補へ変え、alpha tick 0–160 では
transfer の 92.2% が次 tick に反転していた。

有限回転と Euler pole を plate motion の基本表現にし、絶対参照系と相対回転を分ける考え方は
GPlates と global plate motion model に基づく。

- Gurnis et al. (2012), _Plate tectonic reconstructions with continuously closing plates_,
  doi:10.1016/j.cageo.2011.04.014.
- DeMets, Gordon and Argus (2010), _Geologically current plate motions_,
  doi:10.1111/j.1365-246X.2009.04491.x.
- [GPlates reconstruction theory](https://www.gplates.org/docs/user-manual/reconstructions/).

## Decision

初期生成した Euler axis と 5 Myr 分の有限回転角を kinematics の正本とする。
`plate_motion_gain` だけを速度倍率とし、slab pull、ridge push、collision drag は診断に残すが
plate speed を上書きしない。位置 `r` の速度は `v = omega x r` から直接求める。

共有境界の絶対速度は通常は両 plate 速度の平均、subduction は overriding plate の速度とする。
relative velocity は境界分類と material reaction だけに使う。法線は ordered plate pair の low から high へ
固定し、pair と球面 bucket ごとに符号付き進行量を積分する。整数 cell 分だけ contiguous patch を移し、
source 分断、target 孤立、donor floor、plate throughput guard を維持する。

front span と plate throughput 上限も `real_years_per_tick / 5 Myr` で縮尺する。
epoch で時間刻みが変われば旧 accumulator を破棄し、保存元の刻みが不明な旧状態も初回に破棄する。

plate split、merge、birth、loss は実装しない。subduction が material marker をすべて隠す場合は、
投影面積最大の 1 cell を診断 seed として残し、未実装の plate loss を暗黙に起こさない。

## Approximation

各 tick は保存角を実時間比で線形縮尺し、tick 内の Euler pole 変化と角加速度を解かない。
両 plate の平均境界速度は ridge migration と collision impedance を解かない一次近似である。
固定 mesh では移動は整数 cell に量子化され、1 cell 未満は accumulator に残る。
slab torque、mantle flow、rollback、persistent spherical boundary graph、plate lifecycle は別 decision の対象とする。

## Validation

alpha level 6 は tick 1600 を完走し、全9 plateが1 component、orphan 0を維持した。
Crust末尾とEnvironment冒頭の変更量比は0.210で、実時間幅の比0.200と整合した。
4 seedの反転率、棄却案、全tickの形状値は
[validation log](../operations/bench/geology/kinematic_euler_boundary_advection.md) に記録する。
