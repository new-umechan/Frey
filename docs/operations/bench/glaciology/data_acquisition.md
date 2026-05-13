# Glaciology benchmark 実データ取得手順

## 目的

Glaciology 単体ベンチ（`glaciology_solo`）に必要な生データと、
キャッシュ（`*_ref.bin`）生成までの手順を固定する。
不足しているデータと再取得手順を明確化する。

## 前提

- 実行場所: リポジトリルート
- セル重心CSV: `benches/data/cell_centroids.csv`
- ベンチ入力キャッシュ:
    - `benches/data/terrain_ref.bin`
    - `benches/data/climate_ref.bin`
    - `benches/data/glaciology_ref.bin`

`glaciology_solo` ベンチ本体は上記3つのキャッシュを読む。

- `terrain_ref.bin` が無い: ベンチ終了
- `climate_ref.bin` が無い: ベンチ終了
- `glaciology_ref.bin` が無い: 主評価（Spearman）のみスキップ

## 必要データ一覧

### A. 地形（必須）

geologyのものを流用

### B. 気候（必須）

climateのものを流用

### C. 氷厚参照（主評価に必須）

- 生データ（手動取得）
    - 取得元データ一式は `benches/raw/glaciology/` 配下へ配置
    - ベンチ入力には GeoTIFF/NetCDF 1ファイルを使う（配置先は任意）
- 出力キャッシュ
    - `benches/data/glaciology_ref.bin`

取得元:

- Millan et al. 2022（DOI）
    - https://doi.org/10.1038/s41561-021-00885-z

## 手順（最短）

### 1. セル重心を生成

```sh
pnpm bench:dump-centroids
```

### 2. 地形データを配置して terrain キャッシュ生成

手動で `ETOPO_2022_v1_60s_N90W180_surface.tif` を `benches/raw/geology/` に置く。

```sh
pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
```

### 3. 気候データを用意して climate キャッシュ生成

3-1. WorldClim 月次 tif 群と `ai_et0.tif` を手動で `benches/raw/climate/` に配置する。

3-2. ERA5 月次を年単位で取得（再取得容易・中断再開対応）。

```sh
pnpm bench:fetch:era5
```

補足:

- 年別キャッシュ: `benches/raw/climate/era5_land_monthly_yearly/era5_land_monthly_<YEAR>.nc`
- 統合出力: `benches/raw/climate/era5_land_monthly_1970_2000.nc`
- 再実行時は、既に存在して読める年ファイルを自動スキップする（続きから再開）。

3-3. 年平均/年積算へ前処理。

```sh
pnpm bench:prepare:worldclim
pnpm bench:prepare:era5
```

3-4. `climate_ref.bin` を生成。

```sh
pnpm bench:resample:climate -- \
  --temperature benches/raw/climate/worldclim_tavg_annual_c.tif \
  --precipitation benches/raw/climate/worldclim_prec_annual_mm.tif \
  --evapotranspiration benches/raw/climate/era5_land_annual_1970_2000.nc \
  --var-name evapotranspiration=evapotranspiration_mm_yr \
  --runoff benches/raw/climate/era5_land_annual_1970_2000.nc \
  --var-name runoff=runoff_mm_yr \
  --aridity benches/raw/climate/ai_et0.tif \
  --aridity-source precip_over_pet_x10000
```

### 4. 氷厚参照を配置して glaciology_ref を生成

生データの展開先が深いディレクトリでも、`--ice-thickness` に実ファイルパスを渡せばよい。
固定ファイル名に寄せる場合は `benches/raw/glaciology/millan_2022_ice_thickness.tif` へ移動またはコピーする。

例1: 固定パス運用

```sh
pnpm bench:resample:glaciology-ref -- \
  --ice-thickness benches/raw/glaciology/millan_2022_ice_thickness.tif
```

例2: ネストされた配置を直接参照

```sh
pnpm bench:resample:glaciology-ref -- \
  --ice-thickness benches/raw/glaciology/<nested-dir>/<ice-thickness-file>.tif
```

### 5. 実行

```sh
pnpm run bench --suite glaciology_solo
```

## チェックリスト

実行前に次が存在すること。

```text
benches/data/cell_centroids.csv
benches/data/terrain_ref.bin
benches/data/climate_ref.bin
benches/data/glaciology_ref.bin
```

## 補足

- `benches/data/glaciology_ref.bin` が無い場合でも、
  補助評価（代表地域ランキング）は実行される。
- `rust/benches/glaciology_solo.rs` のSKIPPED時メッセージには
  `bench:prepare:glaciology` と表示されるが、現行の実コマンドは
  `bench:resample:glaciology-ref` である。
- ディレクトリを移動した場合は、本書のコマンド例の `--ice-thickness` パスを
  実配置に合わせて更新すること。

関連:

- `docs/operations/bench/glaciology/solo.md`
- `docs/operations/bench/ecology/data_acquisition.md`
