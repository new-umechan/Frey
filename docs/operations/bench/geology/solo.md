# Geology単体ベンチ（Earth 実データ入力）

## 概要

`geology_solo` は、Earth 実データを入力として与えたときの `Geology` 単体応答を比較する bench である。
既存の tectonics 診断 bench は `geology_validation_solo` として別管理する。

- 想定入力: Earth の地形・海洋地殻年齢・プレート境界などの外部データ
- 想定目的: `Geology` が主責務を持つ tectonic / lithospheric 応答だけを比較する
- 非目的: tectonics validation bench の兼用
- 非目的: `Hydrology` 責務の河川・侵食・堆積主比較

設計方針は次のとおり。

- Earth 固有 preset への過剰適合ではなく、Earth 実データ入力に対する出口比較を行う
- `height` の全球一致そのものではなく、`Geology` 固有の構造応答を主評価に置く
- fluvial `erosion_rate` / `deposition_rate` は Hydrology の責務として扱う
- 現行 `hydrology_solo` と責務が衝突しない指標だけを持ち込む

## 現在の状態

`geology_solo` は実装済みで、`terrain_ref.bin` と `oceanic_crust_age_ref.bin` を使い、`plate_boundary_ref.bin` があれば ridge 距離指標、`continental_mask_ref.bin` があれば hypsometry 分離指標も出す。
現行の tectonics 診断 bench は [validation_solo.md](/Users/umehararyu/prog/100days/Frey/docs/operations/bench/geology/validation_solo.md) を参照する。
加えて、`terrain_ref.bin` の海岸近傍 hypsometry を使い、`+1m/+5m/+10m/+20m/+50m` の海面上昇時に
generated terrain と reference terrain で land ratio / newly inundated ratio がどれだけずれるかを diagnostics に残す。

## ベンチの考え方

この bench では、「Earth 実データを input として読ませたとき、その出力が `Geology` 単体の妥当性を測れているか」を最優先にする。

そのため、次のような項目は主評価に置かない。

- 全球 `height` の直接一致
- `river_flow`
- fluvial `erosion_rate`
- fluvial `deposition_rate`
- 湖分布

これらは `Climate` / `Hydrology` / `Glaciology` の影響が強く、`Geology` 単体の検証としては責務境界が曖昧になる。

代わりに、現実の tectonic setting を入力し、その setting に対する lithosphere / topography 応答だけを測る。

## 入力

| 入力ID | 内容 | 主用途 |
| --- | --- | --- |
| `terrain_ref` | Earth DEM を CellStore に落とした標高参照 | 海陸・海盆・粗い hypsometry 参照 |
| `oceanic_crust_age_ref` | 現実の海洋地殻年齢グリッドを CellStore に集約した参照 | age-depth / ridge 距離検証 |
| `plate_boundary_ref` | 現実のプレート境界種別と ridge / trench 軸 | 境界条件付き relief 応答検証 |
| `continental_mask_ref` | 大陸地殻 / 海洋地殻の Earth 参照分類 | 条件付き hypsometry 分離検証 |

本 bench では次の dataset を使う。

- `terrain_ref`
    - 既存 `ETOPO 2022`
- `oceanic_crust_age_ref`
    - `Seton et al. (2020)` present-day age grid
- `plate_boundary_ref`
    - EarthByte `Global Spreading Ridge File`
- `continental_mask_ref`
    - EarthByte `Continental Polygons`

## 主評価

### 1. `oceanic_age_depth_consistency`

海洋地殻年齢を入力として与えたとき、海洋地殻が age とともに深くなるかをみる。

- input:
    - `oceanic_crust_age_ref`
    - `terrain_ref` または海底深度参照
    - 必要なら `continental_mask_ref`
- model output:
    - `geology.height`
    - 海洋セル集合
- score:
    - age bin ごとの median depth の単調性
    - age と depth の Spearman 相関
    - ridge 近傍と老齢海洋地殻の depth 差

これは `Geology` の thermal subsidence / oceanic lithosphere 応答を見る指標であり、他 module 依存が小さい。

### 2. `ridge_distance_depth_gradient`

海嶺軸からの距離に応じて海底が深くなるかをみる。
`oceanic_age_depth_consistency` の代替または補完として使う。

- input:
    - `plate_boundary_ref` 内の ridge 軸
    - `terrain_ref`
- model output:
    - `geology.height`
- note:
    - `plate_boundary_ref.bin` があるときのみ追加評価する任意指標
- score:
    - ridge からの最短距離と海底深度の Spearman 相関
    - 距離 bin ごとの median depth の単調性

年齢データが揃わない場合でも成立する。
一方で、海嶺からの距離は transform / microplate の影響を受けるため、主指標としては age-depth より一段弱い。

### 3. `crust_conditioned_hypsometry_separation`

大陸地殻と海洋地殻の高度分布が十分に分離しているかをみる。
これは既存の geology validation 文脈とも整合する。
`continental_mask_ref.bin` がある場合にだけ追加評価する。

- input:
    - `continental_mask_ref`
    - `terrain_ref`
- model output:
    - `geology.height`
    - 必要なら `crust_type` 相当の出力
- score:
    - continental / oceanic の mean height 差
    - median 差
    - Wasserstein 距離
    - 分布の overlap ratio

これは「地球らしい二峰性」そのものではなく、「地殻種別条件付きで高度分布が分離しているか」を見る。

### 4. `boundary_type_to_relief_consistency`

プレート境界種別を入力として、境界近傍の relief 応答が tectonic setting に合うかをみる。

- input:
    - `plate_boundary_ref`
    - 必要なら relative plate motion
    - `terrain_ref`
- model output:
    - `geology.height`
    - `debug_trench_strength`
    - `debug_arc_strength`
    - `debug_backarc_strength`
- score:
    - trench 近傍の負標高比率
    - arc / collision 帯の高 relief 比率
    - ridge 近傍の浅海比率

境界と relief の対応を直接測れるため geology 専用度は高いが、truth の整備が重いため v1 では補助主指標寄りとする。

## 補助評価候補

主評価を読む補助として、次は採用しやすい。

- `hypsometry_distance`
    - 全球または海陸別の標高ヒストグラム距離
- `relief_distribution_distance`
    - 局所 relief 分布の距離
- `continent_count`
    - 陸塊数
- `largest_continent_cells`
    - 最大陸塊サイズ
- `coastal_mask_agreement`
    - 海岸セルの precision / recall

これらは `terrain_ref` だけで比較可能で、主指標のスコア変動理由を掘りやすい。
ただし、これ単独では `Geology` 単体性が弱いため、主評価にはしない。

### 沿岸浸水応答診断

`terrain_ref.bin` は `height_to_meters=6000` の内部高さへ正規化されている。
したがって `+50m` は内部高さ `50 / 6000 = 0.008333...` に対応する。

この bench では次を diagnostics として記録する。

- `coastal_inundation_response[].sea_level_rise_m`
- `coastal_inundation_response[].generated_land_ratio`
- `coastal_inundation_response[].reference_land_ratio`
- `coastal_inundation_response[].land_ratio_gap`
- `coastal_inundation_response[].generated_newly_inundated_ratio`
- `coastal_inundation_response[].reference_newly_inundated_ratio`
- `coastal_inundation_response[].newly_inundated_ratio_gap`

これは局所 coastline の厳密一致ではなく、Earth 条件で「海面近傍の hypsometry が不自然に平坦すぎる / 急すぎる」退行を検出する粗い指標である。

## v1 で優先しない項目

次の項目は、v1 では主評価に置かない。

- `river_flow`
- `erosion_rate`
- `deposition_rate`
- `is_lake`
- sediment outlet / delta hotspot
- 全球 `height` の RMSE

理由:

- `river_flow` / `is_lake` は `Hydrology` 単体ベンチの責務
- `erosion_rate` / `deposition_rate` は Hydrology 所有の state として整理中
- delta / outlet は downstream transport bench で扱う方が責務境界が明確
- 全球 `height` の RMSE は module 分離が弱く、`Geology` 単体の退行原因を読みにくい

## 評価基準の所在

学術的な妥当性基準（何を pass/fail とみなすか）は
[validation.md](/Users/umehararyu/prog/100days/Frey/docs/operations/bench/geology/validation.md)
を正本とする。

本書 (`solo.md`) は Earth 実データ入力ベンチの
実行方法・入出力・比較対象の定義だけを持つ。

## 実行コマンド

```bash
pnpm run bench --suite geology_solo
```

baseline 比較:

```bash
pnpm bench:compare:geology-solo -- --baseline tests/perf/geology-solo-baseline.json
```

JSONL 出力先:

- `benches/results/geology_solo_main_scores.jsonl`

`terrain_ref.bin` と `oceanic_crust_age_ref.bin` を同じセル集合で突き合わせ、
海洋セルの age-depth 関係を主に読む。
ただし `oceanic_age_depth_consistency` は若い海洋地殻帯を主に見るため、`100 Myr` 以内を主評価域とする。

## 関連

- `docs/operations/bench/geology/data_acquisition.md`
- `docs/operations/bench/geology/validation.md`
- `docs/operations/bench/geology/validation_solo.md`
