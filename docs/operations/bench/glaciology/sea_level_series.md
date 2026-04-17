# Glaciology 海水面時系列ベンチ

## 目的

`glaciology_solo`（1tick診断）を補完し、
海水面更新に対する妥当性を時系列で確認する。

主評価は `runtime.sea_level_offset` の時系列とする。
`ice_thickness` の格子比較（Spearman/RMSE）は並行して記録するが、
このベンチでは自動FAILゲートを設けず診断値として扱う。

## 入力

必須:

- `benches/data/terrain_ref.bin`
- `benches/data/climate_ref.bin`

推奨（格子比較用）:

- `benches/data/glaciology_ref.bin`

任意:

- `GLACIOLOGY_SERIES_MODERN_REF_PATH`（比較対象の上書き）
- `GLACIOLOGY_SERIES_PALEO_REF_PATH`（参照パス記録用）

## 実行モード（3階層）

- short: 短期安定性確認（既定 32 tick）
- mid: 中期トレンド確認（既定 256 tick）
- long: 長期積分確認（既定 1024 tick）

## 実行コマンド

```sh
pnpm bench:run:glaciology-series -- --horizon all --runs 3
```

オプション例:

```sh
pnpm bench:run:glaciology-series -- \
  --horizon long \
  --runs 5 \
  --modern-ref benches/data/glaciology_ref.bin \
  --ticks-long 2048
```

## 出力

JSONL:

- `benches/results/glaciology_sea_level_series_scores.jsonl`

1行1runで次を保存する。

- runtime
  - `glaciology_step_ms_median`
  - `glaciology_step_ms_p95`
- metrics
  - `sle_mm`
  - `sle_start_mm`
  - `sle_mean_mm`
  - `sle_min_mm`
  - `sle_max_mm`
  - `land_ice_volume_km3`
  - `grid_spearman`
  - `grid_rmse`
  - `region_metrics[]`
- references
  - `modern`
  - `paleo`

## 比較

```sh
pnpm bench:compare:glaciology -- --horizon short
```

baseline保存:

```sh
pnpm bench:compare:glaciology -- --horizon short --write-baseline tests/perf/glaciology-bench-short-baseline.json
```

## 地域指標

次の地域IDで局所集計（半径450km）を記録する。

- `alaska`
- `western_canada_usa`
- `arctic_canada_north`
- `arctic_canada_south`
- `greenland_periphery`
- `iceland`
- `svalbard`
- `antarctic_subantarctic`
- `new_zealand`
- `southern_andes`
- `low_latitudes`
- `central_south_asia`
- `caucasus_middle_east`
- `central_europe`
- `north_asia`
- `russian_arctic`
- `scandinavia`

## 注意

- このベンチは診断専用であり、現時点では自動PASS/FAIL判定は行わない。
- `glaciology_ref.bin` が無い場合、`grid_spearman`/`grid_rmse` は `null` になる。
