# Geology benchmark 実データ取得手順

## 目的

`geology_solo` で使う実データの入手方法、保存先、前処理の流れを固定する。

この文書は、Earth 比較ベンチの参照データを「どこから」「何を」「どの順で」集めるかを明文化する。
主対象は `geology_solo` の tectonic / lithospheric 応答比較であり、Hydrology 主責務の侵食参照は補助扱いとする。

## 方針

- 既存の bench 資産は再利用する
- 参照データは `geology_solo` の比較指標用にだけ使う
- v1 は `terrain_ref` と `oceanic_crust_age_ref` を優先する
- 参照データが無くてもベンチ本体は実行し、欠損指標は `null` または skipped として残す

## v1 の推奨採用

`geology_solo` v1 では、候補の中から次を推奨採用とする。

| 入力 | 推奨採用 | 役割 | 採用理由 |
| --- | --- | --- | --- |
| `terrain_ref` | `ETOPO 2022` | 全球 relief の基準地形 | 既存 repo 資産と整合し、陸海をまたぐ全球地形として再利用しやすい |
| `oceanic_crust_age_ref` | `Seton et al. (2020)` present-day age grid | 海洋 age-depth の主入力 | `Geology` 単体性が高く、confidence grid も併用できる |
| `plate_boundary_ref` | EarthByte `Global Spreading Ridge File` | ridge 距離計算 | `ridge_distance_depth_gradient` に必要十分で、PB2002 より軽い |
| `continental_mask_ref` | EarthByte `Continental Polygons` | 条件付き hypsometry の母集団分離 | tectonic 文脈に近い polygon をそのまま使える |

拡張時の候補:

- 海底深度 truth を強めたい場合は `GEBCO Grid` を追加する
- 境界種別まで比較したい場合は `Bird (2003) PB2002` を追加する
- 単純 land/ocean マスクだけで足りる段階では `Natural Earth land polygons` を使ってよい

## 既存データとして使うもの

### 1. 地形

- 保存先: `benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`
- 用途: `terrain_ref.bin` の生成、`geology_solo` の固定地形入力

取得元:

- NOAA ETOPO 2022
- 製品ページ: https://www.ncei.noaa.gov/products/etopo-global-relief-model

取得方法:

1. NOAA の製品ページから ETOPO 2022 の global relief を取得する
2. 60 arc-second の surface GeoTIFF を選ぶ
3. `benches/raw/geology/` に保存する
4. ファイル名を `ETOPO_2022_v1_60s_N90W180_surface.tif` に揃える

運用メモ:

- このファイルが既にあるなら再取得しなくてよい
- `pnpm bench:dump-centroids` の後に `pnpm bench:resample:terrain` で `terrain_ref.bin` を作る

補助候補:

- `GEBCO Grid`
- 用途: `oceanic_age_depth_consistency` の海底深度 truth を ETOPO より海洋寄りにしたい場合
- 理由: 全球 15 arc-second の bathymetry grid と TID grid を持ち、海底の source confidence を扱いやすい

### 2. 既存キャッシュ

`geology_solo` の v1 では主に `height` を使うため、次の既存キャッシュは共有資産として扱う。

- `benches/data/terrain_ref.bin`
- `benches/data/continental_mask_ref.bin`

取得方法:

1. `pnpm bench:dump-centroids`
2. `pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`
3. `pnpm bench:resample:continental-mask -- --polygons benches/raw/geology/continental_mask/ContinentalPolygons/Shapefile/Matthews_etal_GPC_2016_ContinentalPolygons.shp`
4. 必要に応じて他 module 用キャッシュを再利用する

## 新規に取得するもの

### 3. 海洋地殻年齢

- 用途: `oceanic_age_depth_consistency` の主入力
- 保存先: `benches/raw/geology/oceanic_crust_age/`

推奨取得元:

- EarthByte / GPlates Portal の `Seton et al. (2020)` present-day age grid

採用理由:

- present-day oceanic crust age を bench の主目的にそのまま使える
- 6 / 2 / 1 arc-minute grid があり、CellStore 集約前の解像度選択がしやすい
- confidence grid があり、低信頼域を補助診断で除外または downweight できる

取得対象:

- 全球の海洋地殻年齢ラスタ
- 可能なら NetCDF か GeoTIFF
- 単位は Ma を優先する

取得方法:

1. Present-day seafloor age の全球ラスタを取得する
2. `benches/raw/geology/oceanic_crust_age/` に保存する
3. canonical ファイル名を `oceanic_crust_age_ma.tif` または `oceanic_crust_age_ma.nc` に揃える

運用メモ:

- bench 側では年齢の絶対値よりも、age bin ごとの深度単調性と age-depth 相関を主に使う
- 陸域は欠損のままでよい
- 元データが複数タイル・複数投影の場合は、前処理で全球緯度経度ラスタへそろえてから参照化する
- 可能なら age 本体に加えて confidence grid も取得し、`oceanic_age_ref_confidence.bin` 相当を将来追加できる形にする

### 4. プレート境界

- 用途: `ridge_distance_depth_gradient` と `boundary_type_to_relief_consistency`
- 保存先: `benches/raw/geology/SpreadingRidges/`

v1 推奨取得元:

- EarthByte `Global Spreading Ridge File`

拡張候補:

- `Bird (2003) PB2002`

取得対象:

- v1 最小: present-day spreading ridge lines
- 拡張時: ridge / trench / transform を識別できるラインデータ
- 拡張時: 可能なら relative motion か spreading rate を併記したデータ

取得方法:

1. まず EarthByte の ridge line data を取得する
2. `benches/raw/geology/SpreadingRidges/` に保存する
3. canonical 名を `Global_EarthByte_GPlates_PresentDay_Ridges_20100927.xy` または `spreading_ridges.xy` に揃える
4. 境界種別比較まで進める場合のみ、`PB2002` を別ファイルとして追加する

運用メモ:

- v1 では、まず ridge 軸だけ使えれば `ridge_distance_depth_gradient` は実装できる
- trench / arc / backarc の評価までやる場合は、境界種別と極性の正規化が必要になる
- feature ごとの属性名は配布元で揺れやすいので、前処理で repo 内 canonical schema へ落とす
- `PB2002` は境界クラスと相対速度ベクトルを持つため有力だが、v1 の最小構成としては重い

### 5. 大陸 / 海洋マスク

- 用途: `crust_conditioned_hypsometry_separation`
- 保存先: `benches/raw/geology/continental_mask/`

推奨取得元:

- EarthByte `Continental Polygons`

簡易候補:

- Natural Earth の land polygons
- ETOPO 由来の海陸マスク

取得対象:

- 少なくとも land / ocean を識別できる polygon または raster

取得方法:

1. EarthByte `Continental Polygons` を取得する
2. `benches/raw/geology/continental_mask/` に保存する
3. `ContinentalPolygons.zip` を展開し、`ContinentalPolygons/Shapefile/Matthews_etal_GPC_2016_ContinentalPolygons.shp` を canonical 入力にする
4. 簡易代替が必要なら `Natural Earth land` を `land_mask.gpkg` として別管理する

運用メモ:

- v1 では厳密な crustal provenance ではなく、Earth 側の条件付き hypsometry の参照母集団を切る用途に限る
- model 側の `crust_type` が `height` から再導出される段階では、この指標は参考値扱いに留める
- `Natural Earth land` は取得が容易だが tectonic 意味づけは弱いため、主候補は EarthByte 側を優先する

### 6. GloSEM（補助）

- 用途: Hydrology 側 `erosion_rate_spearman` の侵食参照
- 保存先: `benches/raw/hydrology/glosem/` または同等の raw ディレクトリ
- canonical cache: `benches/data/glosem_ref.bin`

取得元:

- JRC/ESDAC Global Soil Erosion map (GloSEM)
- 概要ページ: https://esdac.jrc.ec.europa.eu/themes/global-soil-erosion
- 取得対象は原則として 2012 / 2001 の 25km resampled GeoTIFF

取得方法:

1. 概要ページの Download から Global Soil Erosion dataset を開く
2. 2012 もしくは 2001 の GeoTIFF を取得する
3. `benches/raw/hydrology/glosem/` に保存する
4. canonical ファイル名は `glosem_2012_25km.tif` または `glosem_2001_25km.tif` に揃える

実務上の注意:

- この 2012 / 2001 データは ESDAC 上で free download と案内されているため、通常は Request Form の送信は不要
- ただし GloSEM 1.3 の cropland dataset は別物で、こちらは registration / Request Form が必要
- GloSEM は土壌侵食 proxy であり、露岩侵食や氷河起源 sediment は直接表さない
- `hydrology_solo` の主比較入力として使い、`erosion_rate_spearman` の参照になる
- 取得形態が複数タイルに分かれる場合は、前処理で全球モザイクを作ってから参照化する

生成コマンド:

```bash
pnpm bench:resample:glosem-ref
```

## 既存スクリプトで再生成するもの

### terrain_ref.bin

```bash
pnpm bench:dump-centroids
pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
```

### oceanic_crust_age_ref.bin

実行コマンド:

```bash
pnpm bench:dump-centroids
pnpm bench:resample:geology-age
```

想定内容:

- CellStore 重心へ年齢を落とした `Vec<f32>`
- 陸域や欠損域は `f32::NAN`
- 単位は Ma
- 既定入力は `benches/raw/geology/Grids/age.2020.1.GTS2012.6m.nc`

`geology_solo` v1 では `terrain_ref.bin` と組み合わせて使う。

### plate_boundary_ref.bin

想定コマンド:

```bash
pnpm bench:dump-centroids
pnpm bench:resample:plate-boundary -- --ridges benches/raw/geology/SpreadingRidges/Global_EarthByte_GPlates_PresentDay_Ridges_20100927.xy
```

想定内容:

- 各セルから最近傍 ridge までの距離
- 各セルから最近傍 trench までの距離
- 必要なら最近傍境界種別 ID
- `geology_solo` の v1 では ridge 距離だけを先に使う

### hydro_input.bin（任意再利用）

```bash
pnpm bench:prepare:era5
pnpm bench:resample:hydro-input -- --runoff benches/raw/climate/era5_land_annual_1970_2000.nc --var-name runoff=runoff_mm_yr
```

## 取得後の確認

最終的に最低限次が存在することを確認する。

```text
benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
benches/data/terrain_ref.bin
benches/raw/geology/oceanic_crust_age/
```

## 手元で足りない場合

- oceanic crust age と plate boundary は配布元ごとに属性名・形式が揺れやすい
- もし dataset の選定や canonical schema の切り方を先に固めたいなら、取得候補ファイルを渡してもらえれば前処理仕様まで詰められる
- GloSEM は補助用途なので、`geology_solo` v1 の着手条件にはしない

関連:

- `docs/operations/bench/geology/solo.md`
- `docs/proposal/geology-erosion-deposition-earth-benchmark.md`
