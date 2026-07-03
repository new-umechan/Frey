# Geology validation 単体ベンチ

## 概要

`geology_validation_solo` は、tectonics の runtime / 構造診断を記録する手動 bench である。
default seed は `earth` だが、これは通常の damage-first plate emergence ではなく
hand-authored `earth_preset` 分岐を通る。Earth 実データ比較 bench ではない。

- suite 名: `geology_validation_solo`
- seed: `earth` by default
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
- plate shape observation:
    - `plate_count`
    - `max_area_ratio`
    - `effective_plate_count`
    - `multi_component_plate_count`
    - `max_component_count`
    - `mean_detached_fragment_ratio`, `max_detached_fragment_ratio`
    - `mean_boundary_complexity`, `max_boundary_complexity`
    - `mean_elongation`, `max_elongation`
    - `mean_narrow_connection_cell_ratio`, `max_narrow_connection_cell_ratio`
    - `area_ge_1pct_*`
    - `top8_*`

`plate_shape` は correctness 判定ではなく、目視で感じる shape の違和感を数値へ分解するための観測値である。
特に `narrow_connection_cell_ratio` は「くびれ」候補を拾う proxy であり、単独では PASS/FAIL に使わない。
Earth plate outline を CellStore 上の plate field に変換できたら、同じ metric を Earth 側にも適用し、percentile 分布との比較で読む。
Frey と Earth の比較では `max_*` だけでなく、`top8` と `area_ge_1pct` の p95/p99 を優先する。
これは 1 枚の外れ plate や小 plate の過剰な影響を分けて読むためである。

`seed=earth` の plate field は簡易 preset であり、Earth 実データとの plate-shape 比較対象にしない。
Frey の通常生成を Earth plate outline と比較するときは、明示的に生成 seed で bench を走らせる。

```bash
GEOLOGY_BENCH_SEED=alpha GEOLOGY_BENCH_RUN_ID=plate-shape-generated-alpha \
    cargo bench --manifest-path benches/rust/Cargo.toml --bench geology_validation_solo
pnpm run bench:compare:plate-shape-earth -- --run-id plate-shape-generated-alpha
```

2026-07-03 の観測では、`seed=earth` preset は初期状態から
`plate_count=4`, `multi_component_plate_count=3`, `max_area_ratio=0.745935` であり、
通常生成 seed の plate emergence 退行ではなく preset 固有の粗い分類として扱う。
同じ日に `seed=alpha` で確認した通常生成は初期状態で
`plate_count=8`, `multi_component_plate_count=0`, `max_area_ratio=0.303525`,
`effective_plate_count=4.776167` だった。

## 運用ルール

- PASS/FAIL 判定はしない
- 最新値と baseline の差分を読む
- tectonics runtime や構造変化の退行検出を主用途とする

## 関連

- `docs/operations/bench/geology/solo.md`
- `docs/operations/bench/geology/validation.md`
