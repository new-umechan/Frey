# Benchmark

本書は運用文書である。Climate・Hydrology・Ecology・Domesticates の各モジュールが現実の地球をどこまで近似できているかを評価するためのベンチマーク設計をまとめる。

設計の正本は `docs/concepts/overview.md`・`docs/reference/architecture/data_model.md`・`docs/reference/architecture/module_boundaries.md` を参照する。

## 位置づけと目的

このベンチマークは「実装が壊れていないか」を確認するテストではない。
「便宜的なモデルでどこまで現実に近づけたか、どこは表現しきれていないか」を評価し、**モデルとパラメータの設計判断を支援する道具**として使う。
かなり重たいため、手動実行のみとする。

- ズレが大きい → バグなのか、モデルの限界なのか、を区別するための根拠を提供する
- 信頼度が低い変数の結果も保持する（「これは参考値」と知った上で見る）
- モデルを変えるべき判断材料として使う

回帰テストや日常的な壊れ確認は `docs/operations/test.md` を参照する。
ベンチマーク比較の正本 artifact は `benches/results/` に置く。
docs には比較指標、更新手順、結果の読み方だけを置き、期待値そのものは artifact 側を正本とする。

---

## Geologyの検証方針

Geology は次の 2 系統で検証する。

- tectonics / runtime 診断ベンチ（`geology_validation_solo`）
- crust hypsometry guard（`crust_hypsometry_guard`）
- crust runtime hypsometry series（`crust_runtime_hypsometry_series`）
- crust hydrology hypsometry series（`crust_hydrology_hypsometry_series`）
- crust coupled hypsometry series（`crust_coupled_hypsometry_series`）
- crust exec pipeline hypsometry series（`crust_exec_pipeline_hypsometry_series`）
- 長期 tectonics 検証（`validation.md`）

現行の単体 bench は `geology_validation_solo` であり、Earth preset 上で tectonics の runtime / 構造診断を記録する validation bench とする。
quality gate ではなく、JSONL artifact の最新値・baseline・差分を比較して読む運用とする。

`erosion_rate` / `deposition_rate` は Hydrology の計算責務として扱い、単体比較も `hydrology_solo` で行う。
`hydrology_solo` は `river_flow` / `is_lake` に加えて、GloSEM 参照との `erosion_rate_spearman`
と、`sediment_budget_ratio` / `coastal_deposition_share` / `low_slope_deposition_share` の
粗い sediment 診断を記録する。
主要河川 outlet やデルタ hotspot の整合も Geology ではなく Hydrology の downstream transport 検証で扱う。

`geology_solo` は Earth 実データ入力ベンチとして実装済みで、詳細は `docs/operations/bench/geology/solo.md` を参照する。
tectonics runtime / 構造診断は `docs/operations/bench/geology/validation_solo.md` と `docs/operations/bench/geology/validation.md` を参照する。
長期のウィルソンサイクルと plate 構造の妥当性は `docs/operations/bench/geology/validation.md` で別管理する。
また、沿岸低地と大陸棚の応答をみるため、`geology_solo` は `+1m/+5m/+10m/+20m/+50m` の海面上昇に対する
land ratio / newly inundated ratio の差分診断も記録する。

`crust_hypsometry_guard` は `alpha` seed の Crust 生成直後だけを評価する軽量ベンチである。
`bedrock_coastal_band_ratio`、`land_freeboard_p10/p50/p90`、hypsometry bins を見て、
海面近傍への過剰圧縮が初期地形に焼き込まれていないかを確認する。
`alpha_transition_guard` の `tick=780` より前の切り分けは、まずこの bench を優先する。

`crust_runtime_hypsometry_series` は Geology runtime だけを単独で進めて、
`tick 0..N` の `coastal_band_ratio` と `geology_runtime_bedrock_band_ratio` を記録する軽量 series bench である。
初期 Crust が健全でも `tick=780` 時点で圧縮されている場合は、この bench で悪化開始 tick を先に絞る。

`crust_hydrology_hypsometry_series` は Hydrology / erosion 反映だけを単独で進めて、
`tick 0..N` の `coastal_band_ratio` と freeboard を記録する軽量 series bench である。
Geology 単独では問題が再現しない場合、侵食・堆積の反映で海面近傍への圧縮が入っていないかをこの bench で確認する。

`crust_coupled_hypsometry_series` は `geology + climate + hydrology` だけを回して、
Crust 中の主要結合で `coastal_band_ratio` がどう変わるかを見る軽量 series bench である。
Geology 単独・Hydrology 単独で再現しない場合の次の切り分けとして使う。

`crust_exec_pipeline_hypsometry_series` は `exec_world_with_feedback_and_hydrology` をそのまま使い、
feedback queue・era bookkeeping・shared state 更新順を含む本番寄りの Crust runtime を薄く記録する series bench である。
軽量 series が本番軌道を再現しない場合、最後の切り分けとしてこの bench を使う。
`alpha_transition_guard` より軽く、`tick 0..N` の `coastal_band_ratio`、`sea_level_offset`、runtime bedrock 診断、
`feedback_queue_len` を artifact に残す。
加えて、`geology_runtime_mean_abs_*` を見れば、tectonic uplift / volcanic uplift / tectonic subsidence /
thermal subsidence / diffusive smoothing / isostatic adjustment のどの項が freeboard を潰しているかを tick ごとに追える。
また `geology_runtime_crust_recentering_*` を見れば、Crust 期の `preserve_crust_freeboard` が
どれだけ sea-level quantile をシフトし、適用前後で沿岸帯比率をどう変えたかを追える。
Marine diffusion の切り分けでは、
`geology_runtime_mean_abs_diffusive_ocean_up_raw` と
`geology_runtime_mean_abs_diffusive_ocean_up_applied` を比較する。
raw が大きく applied が小さければ、shoreline-limited attenuation が
深海側の basin infill を抑えていると読める。
それでも `coastal_band_ratio` が落ちない場合は、
`geology_runtime_mean_abs_isostatic_reference_freeboard_applied` と
`geology_runtime_mean_abs_isostatic_compensated_anomaly_applied` を見る。
applied ベースでも前者が支配なら、late Crust の collapse は
isostatic target 面の押し付けが主因である。
そのうえで向きを見るには、
`geology_runtime_mean_signed_isostatic_reference_freeboard_applied_oceanic` と
`geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental` を使う。
continental 側をさらに詰める段階では、
`geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_orogenic` と
`geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable`
まで見る。
stable continental / `PassiveMargin` 診断の詳細ログと棄却仮説は、
[legacy_hypsometry_handover.md](/Users/umehararyu/prog/100days/Frey/docs/operations/bench/geology/legacy_hypsometry_handover.md)
を正本とする。

本書では、読むべき diagnostics の種類だけを保持し、
`vxx` ごとの artifact 比較や棄却履歴は `docs/operations/bench/geology/` 側へ分離する。

Environment 期入口の切り分けでは、少なくとも次を読む。

- stage attribution:
  `geology_stage_mean_abs_height_delta`、
  `glaciology_stage_mean_abs_height_delta`、
  `hydrology_stage_mean_abs_height_delta`
- runtime rebuild / era ramp:
  `geology_runtime_activity_scale`、
  `geology_runtime_rebuild_applied`
- surface delta 分解:
  `geology_runtime_mean_abs_surface_write_delta`、
  `geology_runtime_mean_abs_surface_raw_delta`、
  `geology_runtime_mean_abs_surface_step_delta`、
  `geology_runtime_mean_abs_surface_step_clamp_delta`、
  `geology_runtime_mean_abs_surface_pre_isostatic_delta`、
  `geology_runtime_mean_abs_surface_output_delta`
- stress carry-over 診断:
  `geology_runtime_mean_compressive`、
  `geology_runtime_mean_tensile`、
  `debug_surface_max_delta_*`

これらで `Environment` 初回 tick の異常が、
surface dynamics、runtime rebuild、phase 境界書き戻しのどこにあるかを判定する。

---

## ベンチマーク構成

5種類のベンチを用意する。単体ベンチはモジュール自体の評価、統合ベンチはフィードバックと組み合わせの評価に使う。

| ベンチ               | 入力                                              | tick数         | 目的                         |
| -------------------- | ------------------------------------------------- | -------------- | ---------------------------- |
| Climate単体          | 実地形 + 固定植生（0.5）                          | 1 tick         | Climateモデル自体の評価      |
| Hydrology単体        | 実地形 + 実気候データ                             | 1 tick         | Hydrologyモデル自体の評価    |
| Geology validation単体 | Earth preset                                     | 12+10 tick     | tectonics runtime / 構造診断 |
| Ecology単体          | 実気候データ + 実水文データ                       | 収束まで       | Ecologyモデル自体の評価      |
| Domesticates単体     | 実地形 + 実気候 + 実水文 + 実植生 + 現代分布proxy | 1 tick         | Domesticatesの適地モデル評価 |
| Glaciology単体       | 実地形 + Climate出力                              | 1 tick         | Glaciologyモデル自体の評価   |
| Glaciology海面時系列 | 実地形 + 実気候データ                             | short/mid/long | 海面寄与時系列の診断評価     |
| Climate+Ecology      | 実地形のみ                                        | 収束まで       | 植生フィードバックの効果確認 |
| フルパイプライン     | 実地形のみ                                        | 収束まで       | 統合評価                     |

### 読み方

- Climate単体でズレる → Climateモデル自体の問題
- Climate+Ecologyで改善する → 植生フィードバックが現実方向に効いている
- Climate+Ecologyで悪化する → Ecologyフィードバックが現実と逆方向に働いている可能性
- フルパイプラインで単体より悪化する → モジュール間の相互作用に問題がある可能性

---

## 評価フェーズ

### Phase 1：順序・ランキング

代表地域セットを用意し、乾湿・気温などの大小関係が現実と一致しているかを確認する。
絶対値の一致は問わない。

### Phase 2：分布形状・相関係数

全球スケールで実データとの相関係数・分布形状の一致を確認する。

### フェーズの使い分け方針

Phase 2（Spearman相関）を主指標とする。「改善前後でスコアが何点変わったか」をモデル変更の判断基準として使う。
Phase 1（代表地域ランキング）は、Phase 2のスコアが変動したときに「なぜ変わったか」を掘り下げる診断ツールとして使う。

---

## 比較変数と信頼度ラベル

各変数に信頼度ラベルを付ける。信頼度はベンチ結果の読み方のガイドであり、「低」でもベンチから除外しない。

### Climate

| 変数                 | 信頼度 | 備考                                                     |
| -------------------- | ------ | -------------------------------------------------------- |
| `temperature`        | 高     | 緯度・標高モデルが比較的素直に効く                       |
| `precipitation`      | 中     | 経験的近似の限界あり（モンスーン・偏西風の非対称性など） |
| `aridity`            | 中     | precipitationの精度に依存                                |
| `evapotranspiration` | 中     | Fu式の近似精度に依存                                     |
| `runoff`             | 中     | precipitation・evapotranspirationに依存                  |
| `ocean_temperature`  | 低     | モデルが簡易。参考値として見るのみ                       |

### Hydrology

| 変数              | 信頼度 | 備考                                             |
| ----------------- | ------ | ------------------------------------------------ |
| `river_flow`      | 中     | 主要河川の流量分布。対数スケールで評価           |
| `is_lake`         | 中     | 主要湖の位置再現（バイカル・カスピ・五大湖など） |
| `erosion_rate_spearman` | 低 | GloSEM 由来の侵食 proxy との順位相関。絶対量ではなく傾向を見る |
| `sediment_budget_ratio` | 低 | モデル内部の収支診断。`Σdeposition / Σerosion` を見る |
| `coastal_deposition_share` | 低 | 海岸・浅海への堆積偏りをみる粗い診断 |
| `low_slope_deposition_share` | 低 | 低勾配域への堆積集中をみる粗い診断 |

### Ecology

| 変数             | 信頼度 | 備考                                                                        |
| ---------------- | ------ | --------------------------------------------------------------------------- |
| `biome`          | 中     | 衛星 land cover + 気候 + 水文 + 地形から合成した参照バイオームとの macro F1 |
| `tree_cover`     | 中     | MODIS VCF 由来の樹木被覆との相関                                            |
| `ground_cover`   | 中     | MODIS VCF 由来の non-tree vegetation との相関（開放系植生に限定）           |
| `soil_fertility` | 低     | SoilGrids 由来 proxy との相関。参考値扱い                                   |

### Domesticates

| 変数          | 信頼度 | 備考                                                           |
| ------------- | ------ | -------------------------------------------------------------- |
| `intensity`   | 中     | EarthStat / FAO GLW の現代分布 proxy と Spearman で比較する    |
| `available`   | 中     | proxy intensity を閾値化した二値ラベルとの F1 で評価する       |
| `origin_seed` | 低     | v1 の quality gate では扱わず、将来の別 bench へ分離する       |
| `adoption`    | 低     | 文化・交易・人口圧の影響が大きく、単体ベンチでは主評価にしない |

### Glaciology

| 変数                  | 信頼度 | 備考                                                  |
| --------------------- | ------ | ----------------------------------------------------- |
| `ice_thickness`       | 中     | Millan et al. 2022 全球氷厚推定データとのSpearman相関 |
| `accumulation`        | 低     | 直接実測が困難。proxy評価                             |
| `ablation`            | 低     | 直接実測が困難。proxy評価                             |
| `glacial_melt_runoff` | 低     | 水文データとの間接比較                                |

---

## Phase 1：代表地域セットと評価軸（全モジュール共通）

以下の代表地域を使って順序確認を行う。

| 地域                             | 気候特性   | 評価上の注意                           |
| -------------------------------- | ---------- | -------------------------------------- |
| サハラ・アラビア半島             | 極乾燥     | モデルの得意領域                       |
| アマゾン・コンゴ盆地             | 極湿潤     | モデルの得意領域                       |
| 地中海沿岸                       | 夏乾燥     | 季節性は表現できないため年平均で評価   |
| インド・東南アジア               | モンスーン | モデルの苦手領域。ズレを最初からマーク |
| 西岸海洋性気候（西欧・太平洋岸） | 中緯度湿潤 | 偏西風由来の降水は近似精度が低い可能性 |

### Climateの評価軸（Phase 1）

- `precipitation`：代表地域のランキングが現実と一致しているか
- `temperature`：代表地域のランキングが現実と一致しているか

### Hydrologyの評価軸（Phase 1）

- `river_flow`：アマゾン・コンゴ・ミシシッピ・ナイル・長江などの主要河川流量の大小関係が現実と一致しているか

### Ecologyの評価軸（Phase 1）

- `biome`：代表地域のバイオームラベルが現実と一致しているか
- `tree_cover`：代表地域の樹木被覆の大小関係が現実と一致しているか
- `ground_cover`：代表地域の草本・低木被覆の大小関係が現実と一致しているか

### Domesticatesの評価軸（Phase 1）

- `intensity`：代表地域での種別ごとの成立順序が現実と整合するか
- `available`：成立帯 / 非成立帯の分類が現実と整合するか
- `origin_seed`：v1 では診断対象外

### Glaciologyの評価軸（Phase 1）

- `ice_thickness`：グリーンランド・南極・パタゴニア・ヒマラヤ・アルプス等の氷厚の大小関係が現実と一致しているか
- `glacial_melt_runoff`：融解流出の地域間比較（参考値・known-hard扱い）

---

## Phase 2：評価指標

- 基本指標：Spearman相関係数（ランキング相関の延長として使いやすい）
- 補助指標：分布形状の目視確認（ヒストグラム・散布図）

実データとの格子点ごとの対応は、リサンプリング基盤を通じて行う（上述）。

---

## ベンチマーク基盤（Hydrology・Ecology・統合ベンチ用）

Climate単体ベンチで確立したリサンプリング基盤・出力フォーマット・診断集計（matched/coverage）の設計を他ベンチでも踏襲する。
詳細仕様は次を参照する。

- `docs/operations/bench/climate/solo.md`
- `docs/operations/bench/hydrology/solo.md`
- `docs/operations/bench/ecology/solo.md`
- `docs/operations/bench/domesticates/solo.md`
- `docs/operations/bench/glaciology/solo.md`
- `docs/operations/bench/glaciology/sea_level_series.md`

### 収束判定

Climate+Ecology・フルパイプラインのベンチでは、収束条件を定義して実行を止める。
具体的な収束閾値は実装時に調整する。

## 比較 artifact の扱い

- hypsometry / relief / river flux / basin occupancy などの比較用サンプルと回帰基準は `benches/results/` を正本とする
- `docs/operations/bench/` 配下の文書は、生成方法・更新条件・判定基準のみを書く
- reservoir diagnostics を追加した場合も、継続比較に使う数値出力は `benches/results/` に保存する

## alpha era遷移 guard（統合）

`alpha_transition_guard` は era 遷移（`Crust -> Environment`）の海陸安定性をみる統合ベンチとする。

`seed=alpha` かつ記録窓が stage 境界以降だけを対象にする場合、bench は対応する dev snapshot
（`environment` / `life` / `civilization` / `history`）から自動再開してよい。
これは計算量削減のための実行最適化であり、artifact 上には `resume_from_snapshot_stage` と
`resume_from_snapshot_tick` を残して、どの状態から再開したかを追跡可能にする。
ただし `transition_pre_end_tick` をまたぐ連続性検証が必要な run では cold start を維持する。
`ALPHA_TRANSITION_SNAPSHOT_PATH` を与えた場合は、その explicit snapshot から再開してよい。
これは `tick=780` など stage 境界以外の診断 window を短時間で反復するための開発用経路である。

この bench は warning ではなく violation gate を持つ。少なくとも次を満たさない run は失敗扱いにする。

- `land_ratio` が許容帯内にある
- tick 間の `land_ratio_jump` / `sea_level_jump` / `largest_continent_ratio_jump` が閾値以下
- 描画整合用の `render_land_ratio_diff` が閾値以下
- `coastal_band_ratio` が閾値以下
- `land_freeboard_p90` が過大 freeboard の上限以下
- `water_mass_closure_drift` が絶対値・比率の両方で閾値以下

`ocean_water_inventory_drift` は raw 診断値としては残すが、単独では gate に使わない。
氷床成長や融解で ocean inventory は正当に変化するため、保存則の確認は
`ocean + coupling * ice` の closure から導出する `water_mass_closure_drift` で行う。

`alpha_transition_guard` artifact には、runtime geology の項別寄与として以下も残す。

- `geology_runtime_mean_abs_tectonic_uplift`
- `geology_runtime_mean_abs_volcanic_uplift`
- `geology_runtime_mean_abs_tectonic_subsidence`
- `geology_runtime_mean_abs_thermal_subsidence`
- `geology_runtime_mean_abs_thickness_equilibrium_gap`
- `geology_runtime_mean_abs_isostatic_equilibrium_gap`
- `geology_runtime_mean_abs_isostatic_reference_freeboard`
- `geology_runtime_mean_abs_isostatic_compensated_anomaly`
- `geology_runtime_mean_density_ratio`
- `geology_runtime_mean_abs_diffusive_raw`
- `geology_runtime_mean_abs_diffusive_applied`
- `geology_runtime_mean_abs_diffusive_land_down_raw`
- `geology_runtime_mean_abs_diffusive_land_up_raw`
- `geology_runtime_mean_abs_diffusive_ocean_down_raw`
- `geology_runtime_mean_abs_diffusive_ocean_up_raw`
- `geology_runtime_mean_abs_isostatic_raw`
- `geology_runtime_mean_abs_isostatic_applied`
- `geology_runtime_smoothing_limited_cells_ratio`
- `geology_runtime_mean_smoothing_factor`
- `geology_runtime_zero_mean_adjusted_cells_ratio`
- `geology_runtime_zero_mean_mean_abs_correction`
- `geology_runtime_zero_mean_std_delta`
- `geology_runtime_crust_recentering_shift`
- `geology_runtime_crust_recentering_pre_band_ratio`
- `geology_runtime_crust_recentering_post_band_ratio`

ここで `raw` は limiter 前、`applied` は limiter 後に実際に標高更新へ入った寄与を表す。
`thickness_equilibrium_gap` は `equilibrium_thickness` への回復圧、
`isostatic_equilibrium_gap` は `h_eq - height` の平衡ずれを表す。
`isostatic_reference_freeboard` と `isostatic_compensated_anomaly` は、
`h_eq = reference_freeboard + compensated_anomaly` のどちらが raw isostatic term を支配しているかを見るための診断である。
`diffusive_*_raw` は diffusion の向きと相を分けた診断であり、
land を削る下向き smoothing と ocean を埋める上向き smoothing のどちらが支配かを読むために使う。
`zero_mean_*` は zero-mean mass-centering がどれだけのセルにどれだけの補正を入れたかを表し、
`mean_smoothing_factor` は diffusive / isostatic smoothing が limiter でどこまで削られたかを表す。
`coastal_band_ratio` は `surface_elevation` が海面近傍帯に集中していないかを見る hypsometry gate であり、
「数値上は land ratio を満たすが、実質的に海面ぎりぎりの低地へ圧縮されて見える」状態を捕捉する。
`land_freeboard_p90` はその逆側、すなわち freeboard 保全補助が過剰に効いて
内陸 relief まで不自然に引き上げていないかを見る上側 gate である。
原因切り分け用に、artifact には次の診断値も残す。

- `bedrock_land_ratio`
- `bedrock_coastal_band_ratio`
- `land_freeboard_p10/p50/p90`
- `bedrock_freeboard_p10/p50/p90`
- `geology_runtime_bedrock_band_ratio`
- `geology_runtime_bedrock_p10/p50/p90`
- `geology_runtime_activity_scale`
- `geology_runtime_rebuild_applied`
- `geology_runtime_mean_abs_surface_write_delta`
- `geology_runtime_mean_compressive`
- `geology_runtime_mean_tensile`
- `geology_runtime_mean_signed_surface_write_delta`
- `geology_runtime_min_surface_write_delta`
- `geology_runtime_max_surface_write_delta`
- `geology_runtime_mean_abs_surface_range_clamp_delta`
- `geology_runtime_mean_abs_surface_raw_delta`
- `geology_runtime_mean_abs_surface_step_delta`
- `geology_runtime_mean_abs_surface_step_clamp_delta`
- `geology_runtime_mean_abs_surface_pre_isostatic_delta`
- `geology_runtime_mean_abs_surface_output_delta`
- `geology_runtime_mean_abs_surface_pre_zero_mean_delta`
- `geology_runtime_mean_abs_surface_zero_mean_delta`
- `geology_runtime_debug_surface_max_delta_index`
- `geology_runtime_debug_surface_max_delta_raw_delta`
- `geology_runtime_debug_surface_max_delta_step_delta`
- `geology_runtime_debug_surface_max_delta_thermal_subsidence`
- `geology_runtime_debug_surface_max_delta_diffusive`
- `geology_runtime_debug_surface_max_delta_height_before`
- `geology_runtime_debug_surface_max_delta_height_after_pre_isostatic`

`coastal_band_ratio` が高いときに `bedrock_coastal_band_ratio` も高ければ、問題は主に地形分布
（bedrock hypsometry）側にあると読む。`coastal_band_ratio` のみが高く `bedrock` 側が高くない場合は、
氷床厚や海面更新の coupling が自由表面を海面近傍へ圧縮している可能性を優先して調べる。
`geology_runtime_*` は Geology runtime の `cached_metrics` 由来で、`Crust` 末期から `Environment`
遷移直前までに zero-level 圧縮がいつ発生したかを切り分けるために使う。

- artifact: `benches/results/alpha_transition_guard/alpha_transition_guard.jsonl`
- 主要評価窓: `tick=780..900`
- explicit `ALPHA_TRANSITION_SNAPSHOT_PATH` が stale でも bench は panic で止めず、
  warning を残して cold start へ fallback する。
- `tick=800` の environment snapshot から Geology phase 1 回だけを観測したいときは、
  `cargo run --manifest-path rust/Cargo.toml --bin environment_geology_probe`
  を使う。これは `surface_dynamics` 単体の hidden delta 切り分け用で、
  `mean_abs_surface_write_delta` と `mean_abs_surface_range_clamp_delta` を即座に返す。
- ただし environment snapshot candidate がすべて壊れている場合、
  probe は cold start から `tick=800` まで自動で進めて同じ観測を返す。
- gate（violation）:
    - `land_ratio` の範囲逸脱
    - `land_ratio` ジャンプ
    - `sea_level_offset` ジャンプ/傾き過大
    - `mass_proxy` drift 超過
    - `render_land_ratio_diff` 超過
    - `largest_continent_ratio` ジャンプ
- 診断出力:
    - `continent_count`, `largest_continent_ratio`
    - `sea_level_slope`, `land_ratio_slope`
    - `coastal_band_ratio`

---

## 既知の限界（モデルの表現範囲外）

以下は現行モデルの設計上の限界であり、ベンチでズレが出ても修正対象ではなく「モデルの限界」として記録する。

- モンスーン降水の季節性（年平均しか持たない）
- 偏西風・貿易風由来の降水非対称性（大気大循環を計算しないため）
- 海洋循環の詳細（`ocean_temperature` が簡易モデル）
- 季節的な湖面変動・湿地の動態
