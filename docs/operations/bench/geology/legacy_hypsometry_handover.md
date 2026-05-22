# 旧 Geology 系の棚卸しと検証ログ

本書は、Geology 旧系の Crust / Environment 診断で確定した事実、棄却した仮説、再利用するべき bench 読みをまとめた退避文書である。

`scientific-reservoir-coupled-sea-land-redesign` 系 proposal / decision / reference から切り出した内部検証ログの正本として扱う。

## 位置づけ

- 対象: 旧 Geology 実装の late Crust hypsometry と Environment 入口崩壊の診断
- 用途: 作り替え前の知見退避、artifact の読み方、棄却済み仮説の再確認
- 非対象: 新 Geology の仕様決定そのもの

## 確定事項

### Environment 入口

- `tick=801` の大域崩壊は `Geology` phase が支配で、`Glaciology` / `Hydrology` の寄与はほぼ 0
- 最大変化セルでは `tectonic_subsidence` と `stress` が巨大で、`diffusive` / `thermal_subsidence` は正常範囲
- 原因は Crust stress memory の carry-over であり、Environment 入口での quench により解消した
- この修正後、`alpha_transition_guard` の Environment 入口崩壊は再発していない

### late Crust hypsometry

- shoreline remap と局所 freeboard inflation により `coastal_band_ratio` gate は通るようになった
- ただし post-process 補助の後も、isostatic raw/applied では `reference_freeboard` が支配項
- oceanic の signed term は stabilizing だが小さく、支配残差は continental 側
- continental を `orogenic` / `stable` に分けると、正向き寄与は `stable` に偏る
- `stable` を `rift/ridge` と `passive/transform` に分けると、残差は `passive/transform` に偏る
- さらに `passive/transform` を分けると、残差は `PassiveMargin` のみ

## 実験で棄却した仮説

### `reference_freeboard` 総量を一律に下げれば改善する

- `v27` では stable continental baseline を一律に下げると
    `mean_abs_isostatic_reference_freeboard` は低下した一方、
    `signed_continental_stable` は悪化した
- よって stable 側 baseline の一括引き下げは採用しない

### `PassiveMargin` baseline だけを下げれば改善する

- `v29` では `Transform/PassiveMargin` の baseline を下げても
    `stable_passive_transform` は悪化した
- `v30` で `Transform = 0`、`PassiveMargin > 0` を確認
- よって `PassiveMargin` 単独 baseline の単純な引き下げも採用しない

## `PassiveMargin` 診断の到達点

### baseline 採用版

- `v31`
    - `PassiveMargin raw = 0.003739131`
    - `PassiveMargin applied = 0.0024369606`
    - `Transform raw/applied = 0.0`

- `v33`
    - `PassiveMargin cell ratio = 0.31160587`
    - `PassiveMargin mean isostatic_adjustment_rate = 0.009243147`

- `v35`
    - `PassiveMargin mean_smoothing_factor = 0.9996455`
    - `PassiveMargin effective_applied_factor = 0.6517452`

### 実験版（`PassiveMargin` baseline 引き下げ）

- `v32`
    - `raw = 0.0031162854`
    - `applied = 0.002655808`

- `v34`
    - `cell ratio = 0.31160587`
    - `mean isostatic_adjustment_rate = 0.00924312`

## 読み

- `raw` が減って `applied` が増えたため、悪化の主因は raw target 総量ではない
- `cell ratio` と pre-smoothing の `mean isostatic_adjustment_rate` はほぼ不変だった
- よって旧系で最後まで疑うべきだったのは、`PassiveMargin` における
    `raw -> applied` の実効変換過程である

## 新 Geology へ持ち越す知見

- Environment 入口崩壊は stress memory carry-over を別問題として扱う
- late Crust hypsometry の支配項は、旧系では `PassiveMargin` の isostatic `reference_freeboard`
    applied 化に局在していた
- shoreline 補助の存在は「旧系を延命するための近似」であり、新 Geology の中核仕様にしない
- 旧系で追加した diagnostics は、新系でも再利用価値が高い
    - signed reference-freeboard split
    - raw/applied split
    - phase attribution
    - debug max-delta sample

## 関連 artifact

- `/tmp/crust_exec_pipeline_hypsometry_series_v28.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v29.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v30.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v31.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v32.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v33.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v34.jsonl`
- `/tmp/crust_exec_pipeline_hypsometry_series_v35.jsonl`

## 停止点

旧 Geology 系は、ここで追加係数調整を止めてよい。

次に進めるなら:

1. docs から `vxx` 履歴を本書へ集約する
2. 旧系の dirty state を `WIP` で固定する
3. `docs/research/procedural_tctonic_planets.md` を入力に、新 Geology の proposal を別起票する
