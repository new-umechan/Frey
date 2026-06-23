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

速度は runtime `PlateKinematicsState` の angular speed を
`km/Myr` に変換する。
cell crossing fraction は、`speed * years_per_tick` を mesh の平均近傍距離で割る。

方向持続は plate centroid 上の速度方向を前回 sample と比較する。
centroid path straightness は sample 間の centroid 軌跡について
`net displacement / cumulative path length` を出す。

reciprocal churn は sample 間で plate ownership が変わった cell を
`from -> to` の有向 pair として数え、pair ごとの相互打ち消し割合を見る。
値が高いほど一方向性があり、低いほど相互取り合いに近い。

## Consequences

利点:

- plate 更新の自然さを目視ではなく artifact で比較できる
- 500万年/tick に対して移動量が過小/過大かを直接読める
- jitter と持続 drift を `direction_persistence` / `reciprocal_churn_ratio` で分けられる

欠点:

- centroid-based direction は大きな plate の内部変形や回転中心付近を粗く見る proxy である
- cell crossing fraction は mesh level に依存するので、level を併記して読む必要がある
- 現実の MORVEL/NUVEL 相当の plate pair 速度場を直接再現する検証ではない
