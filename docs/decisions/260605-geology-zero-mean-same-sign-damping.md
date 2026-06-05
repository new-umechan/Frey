# Geology zero-mean補正の同符号減衰化

## Status

Accepted

## Context

Crust期の `tick=30..80` 付近で、プレート境界上の山地が急速に `height=1.2`
上限へ張り付くケースがあった。

`alpha` / `beta` seed の 100 tick 系列では、`land_freeboard_p90` が
`tick=32..41` に急増し、`tectonic_uplift` はほぼ 0 のままだった。
同時に `mean_abs_isostatic_applied`、`mean_abs_diffusive_applied`、
`zero_mean_mean_abs_correction` が増加していた。

原因は、内生的な鉛直変位の零平均拘束が、全球残差を
`uplift/subsidence/volcanism/plume` 由来の重み付きセルへ逆符号で配分していたこと。
この重みはプレート境界に集中しやすいため、拡散・アイソスタシー由来の net subsidence
残差が少数の境界セルへの上向き補正として注入され、補正自体が造山シグナルになっていた。

## Decision

零平均拘束は、残差と同符号の height delta を比例減衰する。

- net uplift が正なら、その tick の上向き変位だけを比例して小さくする。
- net subsidence が負なら、その tick の下向き変位だけを比例して小さくする。
- 補正は逆符号の変位を新規生成しない。
- 補正後も `Σ(next_height - prev_height) = 0` を保つ。

## Rationale

閉じた固体地球 reservoir として全球平均の内生変位を拘束する目的は維持する。
ただし、補正をプレート境界の重みへ配分すると、保存則が局所的な uplift forcing に化ける。

同符号減衰なら、過剰だった uplift または subsidence を弱めるだけで、
物理過程が生成していない山や海盆を補正項が作らない。

## Consequences

- `zero_mean_mean_abs_correction` は引き続き補正量の診断値として使える。
- プレート境界の急激な p90 freeboard 上昇は、zero-mean 補正ではなく
  tectonics / isostasy 本体の寄与としてのみ発生する。
- 境界セルに集中する補正を前提にした地形は変わる。
