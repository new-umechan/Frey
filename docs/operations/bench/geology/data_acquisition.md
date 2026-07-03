# Geology benchmark 実データ取得手順

## 目的

`geology_solo` で使う実データの入手方法、保存先、前処理の流れを固定する。
既存の bench 資産は再利用し、参照データは `geology_solo` の比較指標生成にだけ使う。

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

取得元:

- EarthByte / GPlates Portal の present-day age grid

取得対象:

- 全球の海洋地殻年齢ラスタ
- 可能なら NetCDF か GeoTIFF
- 単位は Ma を優先する

取得方法:

1. Present-day seafloor age の全球ラスタを取得する
2. `benches/raw/geology/oceanic_crust_age/` に保存する
3. canonical ファイル名を `oceanic_crust_age_ma.tif` または `oceanic_crust_age_ma.nc` に揃える

運用メモ:

- 陸域は欠損のままでよい
- 元データが複数タイル・複数投影の場合は、前処理で全球緯度経度ラスタへそろえてから参照化する

### 4. プレート境界

- 用途: `ridge_distance_depth_gradient` と `boundary_type_to_relief_consistency`
- 保存先: `benches/raw/geology/SpreadingRidges/`

取得元:

- EarthByte `Global Spreading Ridge File`

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

- feature ごとの属性名は配布元で揺れやすいので、前処理で repo 内 canonical schema へ落とす

### 4.1 プレート再構成モデル

- 用途: Earth plate shape metrics の参照分布
- 保存先: `benches/raw/geology/plate_reconstruction/Muller2019/`
- 性質: 観測値そのものではなく、Muller et al. 2019 reconstruction model

取得コマンド:

```bash
uv run --python 3.11 --with gplately --with pygplates python -c \
  'from gplately import download; download.DataServer("Muller2019").get_plate_reconstruction_files()'
```

取得後、local gplately cache から次を保存先へコピーする。

- `Topologies/Muller_etal_2019_PlateBoundaries_DeformingNetworks.gpmlz`
- `Rotations/Muller_etal_2019_CombinedRotations.rot`
- `StaticPolygons/Muller_etal_2019_Global_StaticPlatePolygons.gpmlz`

まずは `0, 10, 25, 50, 75, 100, 140 Ma` の各時点を独立した plate field として解決し、
Frey と同じ shape metric を計算する。時系列変化そのものは、同一 reconstruction model 内の
参照分布が安定してから追加する。

CellStore 参照データ生成:

```bash
pnpm bench:resample:earth-plate-id --time-ma 0
pnpm bench:resample:earth-plate-id --time-ma 10
pnpm bench:resample:earth-plate-id --time-ma 25
pnpm bench:resample:earth-plate-id --time-ma 50
pnpm bench:resample:earth-plate-id --time-ma 75
pnpm bench:resample:earth-plate-id --time-ma 100
pnpm bench:resample:earth-plate-id --time-ma 140
```

出力:

- `benches/data/earth_plate_id_ref_000Ma.bin`
- `benches/data/earth_plate_id_ref_010Ma.bin`
- `benches/data/earth_plate_id_ref_025Ma.bin`
- `benches/data/earth_plate_id_ref_050Ma.bin`
- `benches/data/earth_plate_id_ref_075Ma.bin`
- `benches/data/earth_plate_id_ref_100Ma.bin`
- `benches/data/earth_plate_id_ref_140Ma.bin`

Earth 側 shape metric:

```bash
pnpm bench:earth-plate-shape
pnpm bench:compare:plate-shape-earth
```

出力:

- `benches/results/earth_plate_shape_stats.json`

`earth_plate_id_ref_*Ma.bin` は `StaticPolygons` を reconstruction time へ回した cell assignment である。
過去時点ほど present-day static polygon 由来の gap が増えるため、`unassigned_cell_count` を必ず確認する。
また小さい plate id が `narrow_connection_cell_ratio` の上位 percentile を支配しやすいので、
Frey の major plate と比較する前に all-plates 分布と major-plate 相当の分布を分けて読む。
`bench:compare:plate-shape-earth` は Frey の最新 `plate_shape` の `top8` / `area_ge_1pct` p99 を、
Earth reconstruction の同じ scope の p99 上限に対して表示する。
古い Frey record で scope 別 p99 がない場合だけ `max_*` に fallback する。
これは PASS/FAIL ではなく、目視で怪しい shape が Earth reconstruction の外れ値帯を超えているかを読むための診断である。

### 5. 大陸 / 海洋マスク

- 用途: `crust_conditioned_hypsometry_separation`
- 保存先: `benches/raw/geology/continental_mask/`

取得元:

- EarthByte `Continental Polygons`

取得対象:

- 少なくとも land / ocean を識別できる polygon または raster

取得方法:

1. EarthByte `Continental Polygons` を取得する
2. `benches/raw/geology/continental_mask/` に保存する
3. `ContinentalPolygons.zip` を展開し、`ContinentalPolygons/Shapefile/Matthews_etal_GPC_2016_ContinentalPolygons.shp` を canonical 入力にする
4. 簡易代替が必要なら `Natural Earth land` を `land_mask.gpkg` として別管理する

運用メモ:

- Earth 側の条件付き hypsometry の参照母集団を切る用途で使う

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
- GloSEM は補助用途なので、`geology_solo` v1 の着手条件にはしない

関連:

- `docs/operations/bench/geology/solo.md`
