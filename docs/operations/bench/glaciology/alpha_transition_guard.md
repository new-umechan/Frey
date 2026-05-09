# Alpha Era Transition Guard Benchmark

## 目的

`alpha` の `Crust -> Environment` 遷移近傍で発生する陸海比・海面急変を、
手動目視ではなく benchmark artifact としきい値判定で検出する。

## 実行

```sh
pnpm bench:alpha:transition
```

strict しきい値で走らせる場合:

```sh
pnpm bench:alpha:transition:strict
```

## 出力

- JSONL artifact:
  - `benches/results/alpha_transition_guard/alpha_transition_guard.jsonl`
- 1行が1 run で、以下を含む:
  - run metadata
  - `tick=record_start..record_end` の時系列サンプル
  - violation 一覧

## 既定パラメータ

- `ALPHA_TRANSITION_SEED=alpha`
- `ALPHA_TRANSITION_LEVEL=6`
- `ALPHA_TRANSITION_TICKS=900`
- `ALPHA_TRANSITION_RECORD_START=780`
- `ALPHA_TRANSITION_RECORD_END=900`
- `ALPHA_TRANSITION_LAND_RATIO_MIN=0.15`
- `ALPHA_TRANSITION_LAND_RATIO_MAX=0.85`
- `ALPHA_TRANSITION_MAX_LAND_RATIO_JUMP=0.03`
- `ALPHA_TRANSITION_MAX_SEA_LEVEL_JUMP=0.08`
- `ALPHA_TRANSITION_MAX_OCEAN_DRIFT_ABS=1e-4`
- `ALPHA_TRANSITION_PRE_END_TICK=799`
- `ALPHA_TRANSITION_POST_START_TICK=800`
- `ALPHA_TRANSITION_POST_END_TICK=840`
- `ALPHA_TRANSITION_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT=0.04`
- `ALPHA_TRANSITION_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT=0.10`
- `ALPHA_TRANSITION_MAX_MASS_PROXY_DRIFT_ABS=1e-3`
- `ALPHA_TRANSITION_MAX_MASS_PROXY_DRIFT_RATIO=0.02`

## FAIL 条件

- hard fail:
  - `|mass_proxy_drift|` が `MAX_MASS_PROXY_DRIFT_ABS` かつ
    `|mass_proxy_drift| / |mass_proxy_baseline|` が `MAX_MASS_PROXY_DRIFT_RATIO` 超過
- warning（artifactには記録するがfailさせない）:
  - `land_ratio` が `[min, max]` 範囲外
  - `|Δland_ratio|` が `MAX_LAND_RATIO_JUMP` 超過
  - `|Δsea_level_offset|` が `MAX_SEA_LEVEL_JUMP` 超過
  - 遷移前後窓の `median(land_ratio)` 差が `MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT` 超過
  - 遷移前後窓の `median(sea_level_offset)` 差が `MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT` 超過

hard fail 発生時のみ benchmark は非0終了する。
