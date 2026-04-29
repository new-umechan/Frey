# Geology benchmark 実データ取得手順

## 目的

`geology_solo` で使う実データの入手方法、保存先、前処理の流れを固定する。

この文書は、Earth 比較ベンチの参照データを「どこから」「何を」「どの順で」集めるかを明文化する。
GloSEM のような共有参照データは既存 raw ディレクトリ配下で管理する。

## 方針

- 既存の bench 資産は再利用する
- 参照データは `geology_solo` の比較指標用にだけ使う
- 参照データが無くてもベンチ本体は実行し、欠損指標は `null` または skipped として残す

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

### 2. 気候・水文のキャッシュ

`geology_solo` の v1 では主に height を使うが、将来の参照比較や派生指標のために、次の既存キャッシュは共有資産として扱う。

- `benches/data/terrain_ref.bin`
- `benches/data/hydro_input.bin`

取得方法:

1. `pnpm bench:dump-centroids`
2. `pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`
3. 必要に応じて `pnpm bench:prepare:era5`
4. 必要に応じて `pnpm bench:resample:hydro-input`

## 新規に取得するもの

### 3. GloSEM

- 用途: Hydrology 側 `erosion_rate_spearman` の侵食参照
- 保存先: `benches/raw/hydrology/glosem/` または同等の raw ディレクトリ

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
- v1 は絶対値一致ではなく、セル順位の比較に使う
- 取得形態が複数タイルに分かれる場合は、前処理で全球モザイクを作ってから参照化する

## 既存スクリプトで再生成するもの

### terrain_ref.bin

```bash
pnpm bench:dump-centroids
pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
```

### hydro_input.bin

```bash
pnpm bench:prepare:era5
pnpm bench:resample:hydro-input -- --runoff benches/raw/climate/era5_land_annual_1970_2000.nc --var-name runoff=runoff_mm_yr
```

## 取得後の確認

最終的に最低限次が存在することを確認する。

```text
benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
benches/raw/hydrology/glosem/
benches/data/terrain_ref.bin
```

## 手元で足りない場合

GloSEM は取得導線が複数あり、手動取得が必要になる場合がある。
もし配布ページの選定やファイル形式の判断が必要なら、該当ファイルを渡してもらえればこちらで canonical 化の前処理を詰める。

関連:

- `docs/operations/bench/geology/solo.md`
- `docs/proposal/geology-erosion-deposition-earth-benchmark.md`
