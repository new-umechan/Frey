# Earth-like Crust plate timestep

## Status

Accepted

## Context

Crust era の plate ownership transfer は、boundary activity と隣接 plate の
inflow だけで `plate_id` を更新していた。

この更新は cell-local な近似としては軽いが、1 tick を地質時間として読むと、
毎 tick の境界セル入れ替えが速すぎる seed を作りやすい。

Frey の Crust tick は 500万年単位として扱う。
Earth の plate motion はおおむね cm/yr オーダーであり、
500万年では数百 km 程度の移動になる。

参考にする代表値:

- DeMets et al. (1994) NUVEL-1A
- DeMets et al. (2010) MORVEL

## Decision

Crust era の既存 `real_years_per_tick = 5_000_000` 年は維持する。

runtime の plate kinematics では、boundary state にある
`slab_convergence_component` / `slab_rollback_component` を主な駆動力として
plate ごとの target speed を決める。
ridge / rift activity は弱い補助駆動、collision activity は drag として扱う。
このとき駆動力は plate 全セルではなく active boundary cell で正規化する。

Earth-like な速度校正として `50 km/Myr` と Earth mean radius `6371 km` を使い、
`real_years_per_tick` から 1 tick あたりの angular displacement に変換する。
`plate_motion_gain` はこの slab-driven target speed への倍率として使う。

沈み込み帯は一度速度が落ちると即座に消えるものではないため、
`convergence_memory` と old oceanic crust の age / density aging proxy が残る場合は、
subduction state を hysteresis 付きで維持する。

boundary crossing では、固定代表速度ではなく実際の隣接 plate velocity から
1 tick で近傍セル間隔の何割を進めるかを見積もる。
この移動距離は `boundary_activity` で再スケールしない。
`boundary_activity` は境界候補の有無に使い、500万年 tick に対応する
変位量そのものは plate velocity で決める。
また、隣 plate の絶対 inflow だけでなく自 plate との相対 inflow を使い、
自 cell が相手へ向かっている場合の相互取り合いを抑える。
transfer は deterministic sampling で間引き、coarse mesh で
毎 tick 境界セルが総入れ替えになることを避ける。

surface update では、oceanic thermal subsidence を実際の height forcing に含める。
また global zero-mean 補正は `zero_mean_weights` を使い、
slab / volcanic / tectonic forcing が強いセルを優先的に残し、
低 forcing セルで平均補正を受けるようにする。

## Consequences

利点:

- Crust tick の既存 500万年スケールを維持できる
- slab pull が強い plate ほど速くなり、plate ownership churn もその速度に従う
- late Crust で plate は動いているのに surface 変化だけが完全に消える状態を避ける
- 既存 config schema を増やさず、`plate_motion_gain` の意味を保てる

欠点:

- `50 km/Myr` は速度校正の代表値であり、fast spreading ridge や microplate まで
  厳密に再現するものではない
- slab pull / ridge push / collision drag の重みは coarse mesh 用の近似であり、
  force balance を厳密に解くものではない
- deterministic sampling は保存則を厳密に解く移流ではなく、軽量な近似である
