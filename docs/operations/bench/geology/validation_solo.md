# Geology validation 単体ベンチ

## 概要

`geology_validation_solo` は、Earth preset 上で tectonics の runtime / 構造診断を記録する手動 bench である。
Earth 実データ比較 bench ではない。

- suite 名: `geology_validation_solo`
- seed: `earth`
- mesh_level: `6`
- 目的: runtime と tectonics 系 diagnostics の時系列比較

## 実行コマンド

```bash
pnpm run bench --suite geology_validation_solo
```

系列実行と比較:

```bash
pnpm run bench:run:geology-validation-series -- --runs 5
pnpm run bench:compare:geology-validation
```

## 出力 artifact

- JSONL: `benches/results/geology_validation_main_scores.jsonl`
- 1 run = 1 record

最低限記録する項目:

- runtime: `geology_step_p50_ms`, `geology_step_p95_ms`
- phase2.metrics: `sediment_budget_ratio`, `coastal_deposition_share`, `low_slope_deposition_share`
- diagnostics: `open_boundary_export_fraction`, `erosion_reference_coverage`, `lake_deposition_share`

注:

- 上記 sediment 系指標は、現行実装では Earth 実測比較ではなく、runtime 上の補助診断である
- 主責務は Hydrology 側の `erosion_rate` / `deposition_rate` 単体比較ではない

## 運用ルール

- PASS/FAIL 判定はしない
- 最新値と baseline の差分を読む
- tectonics runtime や構造変化の退行検出を主用途とする

## 関連

- `docs/operations/bench/geology/solo.md`
- `docs/operations/bench/geology/validation.md`
