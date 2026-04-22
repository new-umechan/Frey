# Glaciologyの詳細仕様

## 目的

Glaciologyは、地形と気候から氷河の質量収支を計算し、氷厚・融解流出・氷河侵食率を更新する。
毎tickで次の値を `World State` に書く。

- 氷厚（`glaciology.ice_thickness`）
- 堆積量（`glaciology.accumulation`）
- 消耗量（`glaciology.ablation`）
- 融解流出量（`glaciology.glacial_melt_runoff`）
- 氷河侵食率（`glaciology.glacial_erosion_rate`）

Glaciologyは「氷河固有状態の更新」に責務を限定し、標高の最終反映は `Geology` が担う。

## 入力

Glaciologyが読む主な値は次のとおり。

- `geology.height`
- `climate.temperature`
- `climate.precipitation`
- `geo.neighbors_offsets`
- `geo.neighbors`
- `clock.epoch`

## 出力

Glaciologyは次の配列を全セル分持つ。

- `glaciology.ice_thickness`
- `glaciology.accumulation`
- `glaciology.ablation`
- `glaciology.glacial_melt_runoff`
- `glaciology.glacial_erosion_rate`

## 処理ロジック

### 実行位置

tick内の実行順は次のとおり。

1. `Geology`
2. `Climate`
3. `Glaciology`
4. `Hydrology`

`Glaciology` で計算した融解流出量は同tickの `Hydrology` に入力される。

### 質量収支

各セルで温度と降水から氷河の収支を計算する。

```text
cold_excess = max(accum_temp_threshold_c - temperature, 0)
warm_excess = max(temperature - ablation_temp_threshold_c, 0)

accumulation =
  precipitation * accumulation_gain
  * (1 + cold_excess * accumulation_temp_sensitivity)

ablation =
  warm_excess * ablation_gain * (1 + local_relief * relief_weight)
```

`local_relief` は近傍セル標高差から導く地形起伏proxyとする。

### 氷厚更新

氷厚は急変を避けるために平滑化更新する。

```text
next_raw = max(prev_ice + accumulation - ablation, 0)
ice_thickness = lerp(prev_ice, next_raw, alpha)
```

`alpha` は実行budgetと `thickness_response_rate` から導く。

### 融解流出

融解由来の流出を次式で計算し、`Hydrology` の入力流出へ加算する。

```text
melt_source = max(ablation - accumulation, 0)
glacial_melt_runoff = melt_source * melt_runoff_gain
```

### 氷河侵食率

v1では氷厚と起伏の近似式で侵食率を与える。

```text
glacial_erosion_rate = ice_thickness * local_relief * erosion_gain
```

標高への反映は `Geology` 側で `glacial_erosion_coupling` を通して行う。
この侵食で生じる sediment は v1 では `Hydrology` へ渡さず、
glacial erosion source と export / `marine_sediment_mass` diagnostics にのみ計上する。

## パラメータ管理

氷河パラメータは `config/glaciology.yaml` を正本とし、
`pnpm run config:sync` で
`rust/src/generated/glaciology_params_defaults.rs` を再生成する。

主パラメータ:

- `accum_temp_threshold_c`
- `ablation_temp_threshold_c`
- `accumulation_gain`
- `accumulation_temp_sensitivity`
- `ablation_gain`
- `thickness_response_rate`
- `melt_runoff_gain`
- `erosion_gain`
- `glacial_erosion_coupling`

## 責務分離

- `Glaciology` は氷河状態と氷河由来フラックスのみ書く
- `Hydrology` は河川ネットワーク・流量・河川侵食を更新する
- `Geology` は河川侵食と氷河侵食を合算して標高へ反映する
- `Climate` は気候場を更新し、氷河自体は更新しない
- v1 では氷河由来 sediment transport は持たず、`Hydrology` に渡すのは `glacial_melt_runoff` のみとする

## 今後の展望

### v2候補（物理強化）

- 氷河流動の方向性を持つ輸送（ice flux divergence）を導入する
- 氷厚勾配と基盤勾配を分離した侵食則に置き換える
- 氷床と山岳氷河を別モードで扱う

### v2候補（水文連携強化）

- `glacial_melt_runoff` を季節性付きで分解する
- 氷河湖形成と氷河湖決壊（GLOF）をHydrologyへ接続する
- 河川水温や土砂輸送への氷河寄与を独立項として持つ

### v3候補（生態・社会連携）

- 高山帯バイオーム境界の時間変化を `Ecology` に供給する
- 氷河後退が居住可能域へ与える遅延効果を `Settlement` へ接続する
- 長期淡水安定性指標を `Subsistence` / `Population` へ提供する

関連:

- `docs/reference/architecture/module_boundaries.md`
- `docs/reference/architecture/data_model.md`
- `docs/reference/modules/climate.md`
- `docs/reference/modules/hydrology.md`
- `docs/reference/modules/geology.md`
