# Geology単体ベンチ（Earth 侵食・堆積 v1）

## 概要

`geology_solo` は Earth 固定条件で侵食・堆積の傾向場を診断する手動ベンチである。
v1 は quality gate ではなく、JSONL artifact の時系列比較を目的にする。

- seed: `earth`
- mesh_level: `6`
- 評価軸: 空間傾向（順位相関 / hotspot / 収支診断）

このベンチは次の知見に整合する近似を採る。

- 全球土壌侵食 proxy（Borrelli et al., GloSEM）との順位整合
- 全球河川 sediment flux の地域差（Syvitski 系レビュー）との整合

## 実行コマンド

```bash
pnpm run bench --suite geology_solo
```

系列実行と比較:

```bash
pnpm run bench:run:geology-series -- --runs 5
pnpm run bench:compare:geology
```

## 出力 artifact

- JSONL: `benches/results/geology_main_scores.jsonl`
- 1 run = 1 record

v1 で最低限記録する項目:

- runtime: `geology_step_p50_ms`, `geology_step_p95_ms`
- phase2.metrics: `sediment_budget_ratio`, `coastal_deposition_share`, `low_slope_deposition_share`
- diagnostics: `open_boundary_export_fraction`, `erosion_reference_coverage`, `lake_deposition_share`

## 入力データ要件

必須（地形）:

- `benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`

推奨（比較参照）:

- GloSEM（侵食参照）

v1 実装では、参照データ未整備でもベンチ自体は実行し、計算不能な指標は `null` で保存する。

取得手順の詳細は `docs/operations/bench/geology/data_acquisition.md` を参照する。

## データ取得メモ

ETOPO は既存運用の `bench:resample:terrain` と同じ入力を使う。
GloSEM は配布形態や利用規約の都合で手動取得が必要な場合があるため、プロジェクトでの再配布は行わない。

## 運用ルール

- PASS/FAIL 判定はしない
- 最新値と baseline の差分を読む
- under-resolved な狭小地形（デルタ・狭湾・小流域）の扱いは Hydrology 側の堆積診断で扱う
- モデル変更時は `bench:run:geology-series` を実行して比較記録を残す

## 既知の限界

- `mesh_level=6`（約100 km）では局所地形を解像しきれない
- GloSEM は土壌侵食 proxy であり、露岩侵食・氷河輸送・海底輸送を直接は表さない
- 河口・デルタの堆積 hotspot と主要 outlet の整合は Geology 単体より Hydrology 側の downstream transport 検証に近い
