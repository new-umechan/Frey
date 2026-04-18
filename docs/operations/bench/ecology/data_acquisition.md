# Ecology benchmark 実データ取得手順

## 目的

Ecology 単体 benchmark で使う実データの取得元、保存先、前処理の流れを固定する。
この文書は「何を新規に取りにいく必要があるか」と「既存データをどこまで使い回せるか」を切り分けるための運用メモである。

## 方針

Ecology benchmark の入力は、できる限り既存の benchmark 用データを再利用する。
新規に取得するのは Ecology の参照正解を作るためのデータだけに限定する。

初版の固定方針は次のとおり。

- 地形: ETOPO 2022 surface
- 気候: WorldClim v2.1 + ERA5-Land 1970-2000 平均
- 水文: GloFAS historical + HydroLAKES
- 植生・土地被覆: MODIS Collection 6.1 の 2019 年
- 土壌: SoilGrids 250m 現行版

植生と土地被覆の年を 2019 年に固定するのは、同一年の MODIS 年次プロダクトで揃えやすく、かつ benchmark 基準として十分新しいためである。

## 既存データをそのまま使い回すもの

### 1. 地形

- 保存先: `benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`
- 用途: `terrain_ref.bin` の生成、Ecology benchmark の `height` 入力

取得元:

- NOAA ETOPO 2022 User Guide: https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO2022/docs/1.2%20ETOPO%202022%20User%20Guide.pdf

運用:

- すでにファイルがあるなら再取得不要
- `pnpm bench:resample:terrain -- --height ...` で使う

### 2. 気候

- 保存先:
    - `benches/raw/climate/worldclim_tavg_annual_c.tif`
    - `benches/raw/climate/worldclim_prec_annual_mm.tif`
    - `benches/raw/climate/era5_land_annual_1970_2000.nc`
- 用途:
    - `climate_ref.bin` の生成
    - Ecology benchmark の `temperature` / `precipitation` 入力

取得元:

- WorldClim 2.1 historical climate: https://www.worldclim.org/data/worldclim21.html
- ERA5-Land monthly means: https://cds.climate.copernicus.eu/datasets/reanalysis-era5-land-monthly-means?tab=download

運用:

- WorldClim の月次 tif 群が `benches/raw/climate/` にあれば、`pnpm bench:prepare:worldclim` で年平均・年積算を再生成できる
- ERA5-Land は既存の `pnpm bench:fetch:era5` / `pnpm bench:prepare:era5` を使う

### 3. 水文

- 保存先:
    - `benches/raw/hydrology/glofas_era5_annual_mean.nc`
    - `benches/raw/hydrology/HydroLAKES_polys_v10.shp`
- 用途:
    - `hydro_ref.bin` の生成
    - Ecology benchmark の `river_flow` 入力

取得元:

- GloFAS historical discharge: https://ewds.climate.copernicus.eu/datasets/cems-glofas-historical?tab=download
- HydroLAKES: https://www.hydrosheds.org/page/hydrolakes

運用:

- GloFAS は既存の `pnpm bench:fetch:glofas` / `pnpm bench:prepare:glofas` を使う
- HydroLAKES shapefile があるなら再取得不要

## 新規に取得するもの

Ecology benchmark の参照正解生成に必要だが、現在の `benches/raw` には入っていないもの。

### 4. MOD44B Vegetation Continuous Fields

用途:

- `tree_cover` 参照
- `ground_cover` 参照
- `biome` 合成参照

取得元:

- MOD44B User Guide: https://lpdaac.usgs.gov/documents/1494/MOD44B_User_Guide_V61.pdf
- Earthdata Search: https://search.earthdata.nasa.gov/search?q=MOD44B

前提:

- NASA Earthdata Login が必要

取得対象:

- Collection 6.1
- 年: 2019
- 必要 SDS:
    - Percent Tree Cover
    - Percent NonTree Vegetation
    - Percent NonVegetated

保存方針:

- 生データ置き場: `benches/raw/ecology/MOD44B/`
- canonical 変換後:
    - `benches/raw/ecology/mod44b_tree_cover.tif`
    - `benches/raw/ecology/mod44b_non_tree_cover.tif`
    - `benches/raw/ecology/mod44b_non_vegetated.tif`

実務手順:

1. Earthdata Search で `MOD44B` を検索する
2. 2019 年の全球 tile を取得する
3. 各 tile から必要 SDS を抽出する
4. 全球モザイクを作る
5. GeoTIFF に書き出して上記 canonical ファイル名に揃える

補足:

- raw の取得形式は HDF tile のままでよい
- benchmark 側では canonical GeoTIFF 名だけを前提にする

### 5. MCD12Q1 Land Cover Type

用途:

- `natural_mask` 生成
- `biome` 合成参照

取得元:

- MCD12Q1 User Guide: https://lpdaac.usgs.gov/documents/1409/MCD12_User_Guide_V61.pdf
- Earthdata Search: https://search.earthdata.nasa.gov/search?q=MCD12Q1

前提:

- NASA Earthdata Login が必要

取得対象:

- Collection 6.1
- 年: 2019
- 必要 SDS:
    - `LC_Type1`
    - `LC_Prop2`

保存方針:

- 生データ置き場: `benches/raw/ecology/MCD12Q1/`
- canonical 変換後:
    - `benches/raw/ecology/mcd12q1_lc_type1.tif`
    - `benches/raw/ecology/mcd12q1_lc_prop2.tif`

実務手順:

1. Earthdata Search で `MCD12Q1` を検索する
2. 2019 年の全球 tile を取得する
3. 各 tile から `LC_Type1` と `LC_Prop2` を抽出する
4. 全球モザイクを作る
5. GeoTIFF に書き出して canonical ファイル名に揃える

### 4-5 共通: canonical 生成コマンド

`MOD44B` と `MCD12Q1` の HDF tile がそろったら、次のコマンドで
benchmark 用 canonical GeoTIFF を生成する。

```sh
pnpm bench:prepare:ecology-modis
```

生成されるファイル:

- `benches/raw/ecology/mod44b_tree_cover.tif`
- `benches/raw/ecology/mod44b_non_tree_cover.tif`
- `benches/raw/ecology/mod44b_non_vegetated.tif`
- `benches/raw/ecology/mcd12q1_lc_type1.tif`
- `benches/raw/ecology/mcd12q1_lc_prop2.tif`

### 6. SoilGrids

用途:

- `soil_fertility` proxy の生成

取得元:

- SoilGrids documentation: https://docs.isric.org/globaldata/soilgrids/index.html
- SoilGrids data access（公式ドキュメント）: https://docs.isric.org/globaldata/soilgrids/
- SoilGrids WebDAV access（ISRIC公式）: https://www.isric.org/explore/soilgrids/soilgrids-access

前提:

- 初回はブラウザの手動ダウンロードを推奨する
- API/WCSは環境差分で失敗しやすいため、運用の基準手順からは外す

取得対象:

- バージョン: SoilGrids 2.0
- 統計量: mean
- 物性と深さ:
    - SOC（`soc`）: 0-5cm, 5-15cm, 15-30cm
    - CEC（`cec`）: 0-5cm, 5-15cm, 15-30cm
    - pH(H2O)（`phh2o`）: 0-5cm, 5-15cm, 15-30cm
    - Bulk density（`bdod`）: 0-5cm, 5-15cm, 15-30cm

保存方針:

- `benches/raw/ecology/soilgrids/`

推奨ファイル構成:

- canonical（0.1度で再投影済み、12ファイル）
    - `bdod_0_5cm_mean_0p1deg.tif`
    - `bdod_5_15cm_mean_0p1deg.tif`
    - `bdod_15_30cm_mean_0p1deg.tif`
    - `cec_0_5cm_mean_0p1deg.tif`
    - `cec_5_15cm_mean_0p1deg.tif`
    - `cec_15_30cm_mean_0p1deg.tif`
    - `phh2o_0_5cm_mean_0p1deg.tif`
    - `phh2o_5_15cm_mean_0p1deg.tif`
    - `phh2o_15_30cm_mean_0p1deg.tif`
    - `soc_0_5cm_mean_0p1deg.tif`
    - `soc_5_15cm_mean_0p1deg.tif`
    - `soc_15_30cm_mean_0p1deg.tif`

実務手順:

1. 次のコマンドで、0.1度へ再投影した12ファイルを直接作成する

```sh
pnpm bench:prepare:soilgrids:0p1deg
```

1. 出力先は`benches/raw/ecology/soilgrids/`で固定
2. `pnpm bench:resample:ecology-ref:with-soil`で、12ファイルを入力に重み付き0-30cmを内部合成して`ecology_ref.bin`を再生成する
   固定運用の深さ重みは `0-5 : 5-15 : 15-30 = 5 : 3.5 : 1.5` とする。
   これは厳密な層厚比ではなく、生物利用しやすさを優先した benchmark 運用値である。

最小検証コマンド:

```sh
gdalinfo benches/raw/ecology/soilgrids/soc_0_5cm_mean_0p1deg.tif
gdalinfo benches/raw/ecology/soilgrids/soc_5_15cm_mean_0p1deg.tif
gdalinfo benches/raw/ecology/soilgrids/soc_15_30cm_mean_0p1deg.tif
```

補足:

- `benches/scripts/prepare-soilgrids.py`はSoilGridsのremote VRTを`/vsicurl/`経由で読み、ローカル保存する
- `benches/scripts/resample.py`は`--soil-dir`入力時に、3深さを重み付きで内部合成して`soil_fertility`を計算する
- どれか欠けると`soil_fertility`はNaNで保存される
- 深さ重みは`--soil-w-0-5`/`--soil-w-5-15`/`--soil-w-15-30`で調整できるが、固定運用値は`5/3.5/1.5`とする

## 取得後の配置確認

最終的に benchmark 実装が期待する raw ファイルは次のとおり。

```text
benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif

benches/raw/climate/worldclim_tavg_annual_c.tif
benches/raw/climate/worldclim_prec_annual_mm.tif
benches/raw/climate/era5_land_annual_1970_2000.nc

benches/raw/hydrology/glofas_era5_annual_mean.nc
benches/raw/hydrology/HydroLAKES_polys_v10.shp

benches/raw/ecology/mod44b_tree_cover.tif
benches/raw/ecology/mod44b_non_tree_cover.tif
benches/raw/ecology/mod44b_non_vegetated.tif
benches/raw/ecology/mcd12q1_lc_type1.tif
benches/raw/ecology/mcd12q1_lc_prop2.tif
benches/raw/ecology/soilgrids/
```

## 既存スクリプトで取得できるもの

既存スクリプトがあるもの:

- `pnpm bench:fetch:era5`
- `pnpm bench:prepare:era5`
- `pnpm bench:fetch:glofas`
- `pnpm bench:prepare:glofas`
- `pnpm bench:prepare:worldclim`
- `pnpm bench:prepare:ecology-modis`
- `pnpm bench:prepare:soilgrids:0p1deg`
- `pnpm bench:prepare:soilgrids:aggregate`（任意。4枚の事前集約ファイルが必要な場合のみ）

まだ自動化がないもの:

- MOD44B の取得
- MCD12Q1 の取得

## 次に実装するもの

取得手順をこの文書で固定したので、次の実装単位は次の順がよい。

1. `benches/scripts/resample.py` に `ecology-ref` モジュールを追加する
2. MOD44B / MCD12Q1 / SoilGrids の raw から canonical ファイルを作る補助スクリプトを足す
3. `benches/rust/benches/ecology_solo.rs` を追加する
