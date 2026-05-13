# Domesticates benchmark 実データ取得手順

## 目的

`domesticates_solo` ベンチで使う参照 raster と、
`domesticates_ref.bin` 生成までの手順を固定する。
取得は半自動 curated 運用とし、正本は `benches/raw/domesticates/manifest.json` とする。

## 前提

- 実行場所: リポジトリルート
- セル重心 CSV: `benches/data/cell_centroids.csv`
- ベンチ入力キャッシュ:
    - `benches/data/terrain_ref.bin`
    - `benches/data/climate_ref.bin`
    - `benches/data/hydro_ref.bin`
    - `benches/data/ecology_ref.bin`
    - `benches/data/domesticates_ref.bin`

`domesticates_solo` ベンチ本体は上記 5 つを読む。

- `terrain_ref.bin` が無い: ベンチ終了
- `climate_ref.bin` が無い: ベンチ終了
- `hydro_ref.bin` が無い: ベンチ終了
- `ecology_ref.bin` が無い: ベンチ終了
- `domesticates_ref.bin` が無い: 主評価をスキップ

## 必要データ一覧

### A-D. 地形・気候など（必須）

geology, climate, hydrology, ecology のものを流用

### E. 栽培植物参照（主評価に必須）

- 生データ配置先
    - `benches/raw/domesticates/crops/`
- 出力キャッシュ
    - `benches/data/domesticates_ref.bin`

取得元:

- EarthStat harvested area and yield: https://www.earthstat.org/harvested-area-yield-175-crops/
- 再配布ミラー（zip 一括取得）: https://geodata.ucdavis.edu/geodata/crops/monfreda/

対象種:

- `Wheat`
- `Rice`
- `Maize`
- `Millet`
- `Potato`
- `Cassava`
- `Sorghum`

### F. 家畜参照（主評価に必須）

- 生データ配置先
    - `benches/raw/domesticates/livestock/`
- 出力キャッシュ
    - `benches/data/domesticates_ref.bin`

取得元:

- FAO Global Livestock Production and Health Atlas / Gridded Livestock of the World:
  https://www.fao.org/livestock-systems/global-distributions/en/
- Cattle 2015 DOI: https://doi.org/10.7910/DVN/LHBICE
- Horse 2015 DOI: https://doi.org/10.7910/DVN/JJGCTX
- Pig 2015 DOI: https://doi.org/10.7910/DVN/CIVCPB
- Sheep 2015 DOI: https://doi.org/10.7910/DVN/VZOYHM

対象種:

- `Cattle`
- `Horse`
- `Sheep`
- `Pig`

### G. manifest（必須）

- 正本:
    - `benches/raw/domesticates/manifest.json`

`bench:resample:domesticates-ref` は manifest を読み、
`mode=raster` の entry だけを `domesticates_ref.bin` に取り込む。

## ディレクトリ構成

固定運用の配置先は次のとおり。

```text
benches/raw/domesticates/
  manifest.json
  crops/
    wheat_harvested_area.tif
    rice_harvested_area.tif
    maize_harvested_area.tif
    millet_harvested_area.tif
    potato_harvested_area.tif
    cassava_harvested_area.tif
    sorghum_harvested_area.tif
  livestock/
    cattle_density.tif
    horse_density.tif
    sheep_density.tif
    pig_density.tif
```

配布元の元ファイル名は変更されてもよいが、
ベンチ実装が前提にする canonical 名は上記で固定する。

## manifest 契約

各 entry の必須項目:

- `kind`
- `name`
- `mode`
- `source_family`
- `source_url`
- `local_path`
- `transform`
- `presence_threshold`
- `exclude_regions`
- `known_hard`

`mode` は次だけを使う。

- `raster`
- `assertion_only`

`Yam` と `Camel` は `assertion_only` に残す。
`domesticates-ref` の resample 実装は `mode=raster` の entry だけを読む。

## 手順

### 1. セル重心を生成

```sh
pnpm bench:dump-centroids
```

### 2. 前提キャッシュを先に用意

`domesticates_solo` は `terrain/climate/hydro/ecology` の bench cache を前提にする。
未生成なら各モジュールの取得手順書に従って先に作る。

最低限の生成順:

```sh
pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
pnpm bench:resample:climate -- \
  --temperature benches/raw/climate/worldclim_tavg_annual_c.tif \
  --precipitation benches/raw/climate/worldclim_prec_annual_mm.tif \
  --evapotranspiration benches/raw/climate/era5_land_annual_1970_2000.nc \
  --var-name evapotranspiration=evapotranspiration_mm_yr \
  --runoff benches/raw/climate/era5_land_annual_1970_2000.nc \
  --var-name runoff=runoff_mm_yr \
  --aridity benches/raw/climate/ai_et0.tif \
  --aridity-source precip_over_pet_x10000
pnpm bench:resample:hydro-ref -- \
  --river-flow benches/raw/hydrology/glofas_era5_annual_mean.nc \
  --lakes benches/raw/hydrology/HydroLAKES_polys_v10.shp
pnpm bench:resample:ecology-ref:with-soil
```

### 3. EarthStat から crop raster を手動取得

用途:

- crop observed intensity
- crop observed presence

取得対象:

- EarthStat harvested area raster
- 対象種: `Wheat`, `Rice`, `Maize`, `Millet`, `Potato`, `Cassava`, `Sorghum`

実務手順:

1. EarthStat の説明ページを開く
2. 実取得は `geodata.ucdavis.edu/geodata/crops/monfreda/` の zip 配布を使う
3. `Monfreda_HarvestedAreaHectares.zip` を取得する
4. zip を展開し、対象 crop の `*_HarvestedAreaHectares.tif` を取り出す
5. `domesticates` v1 では原則として `*_HarvestedAreaHectares.tif` を使う
6. `benches/raw/domesticates/crops/` に canonical 名で配置する

canonical 名:

- `wheat_harvested_area.tif`
- `rice_harvested_area.tif`
- `maize_harvested_area.tif`
- `millet_harvested_area.tif`
- `potato_harvested_area.tif`
- `cassava_harvested_area.tif`
- `sorghum_harvested_area.tif`

運用上の注意:

- 配布元 zip 名や内部ファイル名は変わってよい
- benchmark 側は `manifest.json` の `local_path` を見るため、
  最終配置名だけ canonical に揃えばよい
- 入力は GeoTIFF か NetCDF だけを受け付ける
- CRS は geographic を前提にする
- 2026-04-16 時点では `earthstat.org` から辿る旧配布導線や
  `data.mint.isi.edu` の一部 URL で `NoSuchBucket` / `502 Bad Gateway` が出ることがある
- そのため取得手順の基準は、UC Davis mirror の zip 一括取得に寄せる
- `https://geodata.ucdavis.edu/geodata/crops/monfreda/prep.R` から、
  `Monfreda_HarvestedAreaHectares.zip` が各 crop の `*_HarvestedAreaHectares.tif`
  をまとめた zip であると読み取れる
- 例: Wheat は展開後に `wheat_HarvestedAreaHectares.tif` を使う

### 4. FAO GLW から livestock raster を手動取得

用途:

- livestock observed intensity
- livestock observed presence

取得対象:

- FAO GLW 2015 の species dataset
- 対象種: `Cattle`, `Horse`, `Sheep`, `Pig`

実務手順:

1. FAO の `Global distributions` ページを開く
2. 対象 species ページへ移動する
3. `2015` の DOI リンクを開き、Harvard Dataverse の dataset ページへ移動する
4. dataset 内の GeoTIFF 群から `5_*_2015_Da.tif` を取得する
5. 取得した GeoTIFF を `benches/raw/domesticates/livestock/` に canonical 名で配置する

canonical 名:

- `cattle_density.tif`
- `horse_density.tif`
- `sheep_density.tif`
- `pig_density.tif`

運用上の注意:

- 配布元の元ファイル名は固定しない
- benchmark 側は `manifest.json` の `local_path` を正本にする
- `domesticates` v1 では次の Dataverse ファイルを使う
    - Cattle: `5_Ct_2015_Da.tif`
    - Horse: `5_Ho_2015_Da.tif`
    - Pig: `5_Pg_2015_Da.tif`
    - Sheep: `5_Sh_2015_Da.tif`
- これらは「5 arc-min pixel あたりの頭数」であり、
  厳密な km² density raster ではない
- ただし本 benchmark は species ごとに `log1p -> clip -> min-max` 正規化して
  相対的な分布強度を比較するため、v1 ではこの近似を受け入れる
- 真の密度へ変換するには `8_Areakm.tif` で面積補正が必要だが、
  v1 では取得手順と比較基準を単純化するため採用しない
- projected raster でも読めるが、その場合は内部で nearest にフォールバックする
- 全球比較の一貫性を優先し、可能なら geographic GeoTIFF に揃える
- Dataverse 側で access request や確認画面が出る species がある。
  その場合でも DOI ページから dataset を開くのを正規手順とする

### 5. manifest を更新または確認

`benches/raw/domesticates/manifest.json` には、
取得した raster の配置先と benchmark 閾値を記録する。

固定運用:

- crop `source_family`: `earthstat`
- livestock `source_family`: `fao_glw`
- `transform`: `log1p_clip01_minmax`
- `Yam` / `Camel`: `mode=assertion_only`

現在の canonical `local_path`:

- `crops/wheat_harvested_area.tif`
- `crops/rice_harvested_area.tif`
- `crops/maize_harvested_area.tif`
- `crops/millet_harvested_area.tif`
- `crops/potato_harvested_area.tif`
- `crops/cassava_harvested_area.tif`
- `crops/sorghum_harvested_area.tif`
- `livestock/cattle_density.tif`
- `livestock/horse_density.tif`
- `livestock/sheep_density.tif`
- `livestock/pig_density.tif`

固定の `presence_threshold`:

- `Wheat`: `0.18`
- `Rice`: `0.16`
- `Maize`: `0.16`
- `Millet`: `0.12`
- `Potato`: `0.14`
- `Cassava`: `0.14`
- `Sorghum`: `0.12`
- `Cattle`: `0.12`
- `Horse`: `0.08`
- `Sheep`: `0.10`
- `Pig`: `0.10`

### 6. `domesticates_ref.bin` を生成

```sh
pnpm bench:resample:domesticates-ref
```

このコマンドは内部で次を行う。

1. `manifest.json` を読む
2. `mode=raster` の 11 species を cell centroid に sample する
3. 各 raster に `log1p` をかける
4. 1% / 99% quantile で clip する
5. 0..1 に min-max 正規化する
6. species ごとの `presence_threshold` で二値化する
7. `benches/data/domesticates_ref.bin` を出力する

生成物:

- crop observed intensity
- livestock observed intensity
- crop observed presence bitmap
- livestock observed presence bitmap
- crop evaluation mask
- livestock evaluation mask

### 7. 実行

```sh
pnpm bench --suite domesticates_solo
```

## evaluation mask

v1 の `evaluation_mask = 0` 条件:

- raw raster が欠損
- 正規化不能

`known_hard` と `exclude_regions` は manifest に残すが、
v1 の resample 実装では診断メタデータ扱いに留める。

## 取得後の配置確認

最終的に benchmark 実装が期待する raw ファイルは次のとおり。

```text
benches/raw/domesticates/manifest.json

benches/raw/domesticates/crops/wheat_harvested_area.tif
benches/raw/domesticates/crops/rice_harvested_area.tif
benches/raw/domesticates/crops/maize_harvested_area.tif
benches/raw/domesticates/crops/millet_harvested_area.tif
benches/raw/domesticates/crops/potato_harvested_area.tif
benches/raw/domesticates/crops/cassava_harvested_area.tif
benches/raw/domesticates/crops/sorghum_harvested_area.tif

benches/raw/domesticates/livestock/cattle_density.tif
benches/raw/domesticates/livestock/horse_density.tif
benches/raw/domesticates/livestock/sheep_density.tif
benches/raw/domesticates/livestock/pig_density.tif
```

生成済みキャッシュ確認:

```text
benches/data/cell_centroids.csv
benches/data/terrain_ref.bin
benches/data/climate_ref.bin
benches/data/hydro_ref.bin
benches/data/ecology_ref.bin
benches/data/domesticates_ref.bin
```

## 補足

- v1 は「歴史的起源の厳密再現」ではなく、
  現代分布 proxy と環境適地モデルの整合性を測る benchmark である
- `origin_seed` と `adoption` は v1 の gate 対象にしない
- `Yam` と `Camel` は assertion のみ残し、定量比較には使わない
- `manifest.json` に source URL を残し、再取得時に配布元差し替えを追跡できるようにする

関連:

- `docs/operations/bench/domesticates/solo.md`
- `docs/operations/bench/ecology/data_acquisition.md`
- `docs/operations/bench/glaciology/data_acquisition.md`
