# Plate motion naturalness metrics

## Status

Accepted

## Context

Crust runtime の plate 更新は、目視では shape を保っていても、
500万年/tick という時間スケールに対して移動量や方向持続が自然かを判断しにくい。

現実の plate motion はおおむね `2-10 cm/yr`、速い plate では
`10-15 cm/yr` 程度である。
これは `20-100 km/Myr`、高速側で `100-150 km/Myr` に相当する。
Frey の Crust は `5 Myr/tick` なので、典型的な変位は
`100-500 km/tick`、高速側で `750 km/tick` 程度になる。

目視だけでは、局所的な ownership transfer の相互取り合いと、
plate が一定方向へ drift している状態を区別しづらい。

## Decision

`crust_plate_count_series` に plate motion naturalness 指標を追加する。

主指標:

- `mean_plate_speed_km_per_myr`
- `max_plate_speed_km_per_myr`
- `mean_cell_crossing_fraction_per_tick`
- `max_cell_crossing_fraction_per_tick`
- `mean_direction_persistence`
- `reciprocal_churn_ratio`
- `mean_centroid_path_straightness`
- `mean_euler_rotation_residual_km`
- `max_euler_rotation_residual_km`
- `mean_euler_rotation_residual_ratio`
- `max_euler_rotation_residual_ratio`
- `boundary_transfer_evaluated_cell_count`
- `mean_boundary_transfer_velocity_alignment`
- `boundary_transfer_velocity_aligned_ratio`
- `boundary_transfer_velocity_unaligned_ratio`
- `mean_boundary_transfer_largest_component_ratio`
- `max_boundary_transfer_isolated_cell_ratio`
- `mean_articulation_cell_ratio`
- `max_articulation_cell_ratio`
- `mean_boundary_complexity_growth`
- `max_boundary_complexity_growth`
- `mean_boundary_complexity_growth_window_mean`
- `max_boundary_complexity_growth_window_mean`
- `persistent_boundary_complexity_growth_plate_ratio`
- `mean_corridor_neck_risk`
- `max_corridor_neck_risk`
- `mean_boundary_thin_cell_ratio`
- `max_boundary_thin_cell_ratio`
- `mean_eroded_core_cell_ratio`
- `min_eroded_core_cell_ratio`

速度は runtime `PlateKinematicsState` の angular speed を
`km/Myr` に変換する。
cell crossing fraction は、`speed * years_per_tick` を mesh の平均近傍距離で割る。

方向持続は plate centroid 上の速度方向を前回 sample と比較する。
centroid path straightness は sample 間の centroid 軌跡について
`net displacement / cumulative path length` を出す。

Euler rotation residual は、前回 sample の centroid を前回 `PlateKinematicsState` の
`angular_axis` / `angular_speed` で今回 tick まで剛体回転させた予測位置と、
今回の実測 centroid の大円距離である。
`angular_speed` は radians/tick として扱い、sample 間隔ぶん積分する。
ratio は `residual_km / max(expected_rotation_displacement_km, mean_cell_spacing_km)` とし、
低速 plate で値が発散しないようにする。
ownership transfer や plate 面積変化によって centroid は剛体回転から外れるため、
この指標は hard fail ではなく、kinematic state と実際の plate id 更新の整合性を見る
runtime proxy として扱う。

Boundary transfer velocity alignment は、sample 間で `plate_id` が `from -> to` に
変わった cell について、前回 sample の `to` plate 近傍からその cell へ向かう方向と、
前回 `PlateKinematicsState` から計算した `to - from` の相対 Euler velocity が
同じ向きかを見る。
値が正なら local velocity は takeover を支持し、負なら takeover と逆向きである。
実装の boundary crossing は substep ごとに更新される一方、artifact は sample 間差分だけを
見るため、この指標は「記録間隔内の最終差分が直前境界 velocity で説明しやすいか」の
proxy とする。
Boundary transfer spatial coherence は、`from -> to` に変わった cell を `to` plate 別に
induced graph として見て、獲得 cell 群の component 数、最大 component 比率、
isolated cell 比率を出す。
実装は candidate を集めて component 単位で一括適用するが、local velocity が正しくても
component が細かく分かれる場合は boundary front として滑らかに進んでいない可能性がある。
そのため形状劣化との併読に使う。
component の優先順位には support density を使う。support density は candidate cell 群が
既存の target plate にどれだけ接しているかの proxy で、front patch のまとまりを
cell 数だけでなく境界支援の密度でも見るためである。

reciprocal churn は sample 間で plate ownership が変わった cell を
`from -> to` の有向 pair として数え、pair ごとの相互打ち消し割合を見る。
値が高いほど一方向性があり、低いほど相互取り合いに近い。

articulation cell ratio は、同じ `plate_id` の induced graph で
その cell を除くと連結性が壊れる cell の割合を見る。
これは「分断済み fragment」ではなく、まだ単一 component だが
1 cell 幅の neck で大きな塊がつながる状態を検出する topology proxy である。

boundary complexity growth は sample 初回の `plate_boundary_complexity` に対する
現在値の比である。
絶対的に入り組んだ plate だけでなく、runtime ownership transfer によって
周長/面積 proxy が継続的に悪化している plate を検出する。
さらに直近 4 sample の window mean/min を各 plate に出し、全 window sample が
`1.5x` 以上のとき `persistent_boundary_complexity_growth` とする。
これは一時的な境界揺らぎではなく、数 sample にわたって境界複雑度が悪化した状態を
runtime 専用 validation として分離するためである。
Earth/GPlates benchmark は単一時点の shape baseline なので、この persistence 指標の
直接比較対象にはしない。

corridor neck risk は、同じ `plate_id` の graph から低い内部接続度の cell を
k-core 的に剥がしたとき、残った core が複数の大きな lobe に分かれるかを見る。
これは 1 cell articulation だけでは検出できない、数 cell 幅の corridor / hourglass
形状の proxy である。
ただし EarthByte/GPlates 由来の Earth plate id でも corridor risk は 0 にならないため、
単独 hard fail ではなく Earth 分布、area ratio、boundary complexity growth と併読する。

boundary thin cell ratio は、plate 内 cell のうち境界から graph distance 2 以下にある
cell の割合である。eroded core cell ratio は、同じ距離場で 2 layer erosion 後に
残る interior cell の割合である。
これらは corridor ではないが plate 全体が薄い、または境界支配的である状態を見る。
Earth/GPlates の major plate でも非ゼロの薄さは出るため、単独 fail ではなく
Earth percentile と `boundary_complexity_growth` を併読する。

## Consequences

利点:

- plate 更新の自然さを目視ではなく artifact で比較できる
- 500万年/tick に対して移動量が過小/過大かを直接読める
- jitter と持続 drift を `direction_persistence` / `reciprocal_churn_ratio` で分けられる
- plate state の剛体回転予測と、実際の centroid 移動のズレを数値化できる
- ownership transfer が local Euler velocity に沿っているかを cell 差分で検証できる
- velocity と整合した transfer が、空間的にまとまった front か斑点状かを分けられる
- `component_count=1` / `detached_fragment_ratio=0` では通ってしまう細い neck と
  runtime 中の形状劣化を検出できる
- 単発の `boundary_complexity_growth` と persistent な悪化を分けて読める
- Earth plate shape benchmark と同じ corridor proxy を比較できる
- corridor ではない「薄い plate」「interior が少ない plate」を Earth baseline と比較できる

欠点:

- centroid-based direction は大きな plate の内部変形や回転中心付近を粗く見る proxy である
- Euler rotation residual は ownership transfer による centroid 変化も含むため、
  residual だけで角速度 model が間違いとは言えない
- boundary transfer velocity alignment は sample 間隔が粗いほど substep 経路を失うため、
  `record_every=1` の artifact でも確認する必要がある
- boundary transfer spatial coherence は sample 間差分の acquired cells だけを見るため、
  微小な step ごとの front geometry を完全には復元しない
- cell crossing fraction は mesh level に依存するので、level を併記して読む必要がある
- articulation cell ratio は 1 cell 幅の neck に強く、2 cell 以上の neck は
  boundary complexity growth などと併読する必要がある
- persistent boundary complexity growth は sample 間隔に依存するため、
  `record_every` とあわせて読む必要がある
- corridor neck risk は現実の複雑な plate geometry でも非ゼロになるため、
  threshold は Earth benchmark の percentile を基準に調整する必要がある
- boundary thin/core 指標は小 plate や microplate で自然に極端化するため、
  area ratio や top-N major plate scope で読む必要がある
- 現実の MORVEL/NUVEL 相当の plate pair 速度場を直接再現する検証ではない
