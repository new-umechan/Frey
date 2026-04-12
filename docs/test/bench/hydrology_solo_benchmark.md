# Hydrology単体ベンチ（詳細仕様）

## 概要

入力として実地形（`geology.height`、固定）と実気候データ（`climate.runoff`、ERA5-Land由来）を与え、
安定化 tick 実行後の `river_flow`・`is_lake` を主指標として評価する。
`erosion_rate`・`deposition_rate` は実データが粗いため参考値として記録する。

Hydrologyモジュール単体の評価が目的であり、Climateの誤差を混入させないため
`climate.runoff` はClimateモジュールの出力ではなくERA5-Landの実測値を直接入力する。

実行seedは `earth` 固定とし、参照実データと地形前提を一致させる。

## 実行コマンド（予定）

```
# repo root から実行
cargo bench --manifest-path benches/rust/Cargo.toml --bench hydrology_solo
```

## 評価時点

fill-spill の sink / spill 状態を落ち着かせるため、ベンチは次の手順で評価する。

1. 初期 world を構築する
2. `stabilization_ticks = 8` tick 実行する
3. 続く `sample_ticks = 3` tick を計測する
4. runtime は sample tick 群の p95 を採用する
5. 品質指標は最終 tick の `river_flow` / `is_lake` を使う

## 入力の準備

このベンチは、実地形キャッシュ `benches/data/terrain_ref.bin` と、
Hydrology単体ベンチ専用の入力キャッシュ `benches/data/hydro_input.bin`、
評価用キャッシュ `benches/data/hydro_ref.bin` を使う。

- `hydro_input.bin`
  - `runoff`
- `hydro_ref.bin`
  - `river_flow`
  - `is_lake`

現在の実運用では、リポジトリルートで次の順に準備する。

1. `pnpm bench:dump-centroids`（未実行の場合のみ）
2. `pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`（未実行の場合のみ）
3. `pnpm bench:prepare:era5`（未実行の場合のみ）
4. `pnpm bench:resample:hydro-input -- --runoff benches/raw/climate/era5_land_annual_1970_2000.nc --var-name runoff=runoff_mm_yr`
5. `pnpm bench:resample:hydro-ref -- --river-flow benches/raw/hydrology/glofas_era5_annual_mean.nc --lakes benches/raw/hydrology/HydroLAKES_polys_v10.shp`

`bench:prepare:era5` の前提として、`benches/raw/climate/era5_land_monthly_1970_2000.zip` を用意する
（`pnpm bench:fetch:era5` で取得可）。

GloFAS-ERA5 は `benches/raw/hydrology/glofas_era5_annual_mean.nc` を参照する
（Copernicus EWDS から取得: https://ewds.climate.copernicus.eu）。
このファイルは、日次データをそのまま全件取得した年平均ではなく、
複数年・複数月に対して7日刻み（既定: `01,08,15,22,29`）で取得した日次サンプルの平均から作る近似年平均でもよい。
固定ベンチの比較用参照として年ごとの偏りを抑えることを優先する。

HydroLAKES は `benches/raw/hydrology/HydroLAKES_polys_v10.shp` を参照する
（https://www.hydrosheds.org/products/hydrolakes）。

| フィールド | 型 | 値 |
|---|---|---|
| `geology.height` | `Vec<f32>` | 実地形データを内部標高単位へ変換した値（`height * 6000 = m`） |
| `geo.latitude` | `Vec<f32>` | セル重心緯度（単位: 度、-90〜90） |
| `climate.runoff` | `Vec<f32>` | `hydro_input.bin` から読むERA5-Landのrunoff（単位: mm/年） |

---

## セル選定の方法

Climate単体ベンチと同じ方式を踏襲する。
「指定した緯度経度に最も近い重心を持つセル」を選定セルとする。

```rust
fn nearest_cell(cells: &CellStore, lat: f32, lon: f32) -> CellId {
    cells.latitude.iter().zip(cells.longitude.iter())
        .enumerate()
        .min_by(|(_, (la, lo)), (_, (lb, lob))| {
            haversine(*la, *lo, lat, lon)
                .partial_cmp(&haversine(*lb, *lob, lat, lon))
                .unwrap()
        })
        .map(|(i, _)| CellId(i as u32))
        .unwrap()
}
```

代表地域の指定緯度経度は以下の通り。
各地域は「流路の振る舞いのバリエーション」を網羅するよう選定した。

| 地域ID | 地域名 | 緯度 | 経度 | 水文特性 |
|---|---|---|---|---|
| `amazon_mouth` | アマゾン河口 | -1.5 | -51.5 | 世界最大流量・MFD集中の基準点 |
| `congo_mouth` | コンゴ河口 | -6.0 | 12.5 | 第2位流量・熱帯湿潤 |
| `mississippi_mouth` | ミシシッピ河口 | 29.0 | -89.5 | 北米最大・デルタ地帯 |
| `yangtze_mouth` | 長江河口 | 31.5 | 121.5 | アジア最大・モンスーン |
| `nile_mouth` | ナイル河口 | 31.5 | 31.0 | 長大・乾燥域を流れる外来河川 |
| `sahara_interior` | サハラ内部 | 23.0 | 13.0 | 流量ゼロ相当・乾燥域の対照 |
| `himalaya_foothills` | ヒマラヤ山麓 | 27.0 | 85.0 | 急勾配・高流量・MFD集中 |
| `ganges_delta` | ガンジスデルタ | 22.5 | 89.5 | 緩勾配・MFD分散・デルタ |

---

## 主評価：全球比較

### 1-A：river_flow の Spearman 相関

#### 実データソース

| 変数 | データソース | 解像度 | 取得先 |
|---|---|---|---|
| `river_flow` | GloFAS-ERA5（近似複数年平均流量） | 0.05度（version_4_0） | https://ewds.climate.copernicus.eu |

#### 対数変換

河川流量は対数スケールで評価する。
流量は数桁にわたるダイナミックレンジを持ち、線形スケールでは大河川のみに相関が引っ張られるためである。

```rust
let log_flow = flow.map(|v| if v > 0.0 { v.ln() } else { f32::NAN });
```

ゼロ以下のセルは `f32::NAN` として Spearman 計算から除外する。

#### 陸セル限定

陸セルのみで計算する（`geology.height > 0` のセルに限定）。

#### リサンプリング手順

Climate単体ベンチのリサンプリング基盤と同一の手順を踏襲する。

```
GloFAS-ERA5グリッド（緯度経度ラスタ）
  → 各セルの重心座標（latitude, longitude）でバイリニア補間
  → セルごとの実データ値 Vec<f32>
```

#### 評価方針

主評価 1-A は閾値判定を行わず、`rho` の生スコアを記録して比較する。
モデル変更の判断は同一条件での前後差で行う。

---

### 1-B：is_lake の F1 スコア

#### 実データソース

| 変数 | データソース | 解像度 | 取得先 |
|---|---|---|---|
| `is_lake` | HydroLAKES v1.0（湖ポリゴン） | ベクタ | https://www.hydrosheds.org/products/hydrolakes |

#### セルへの変換

```
HydroLAKESポリゴン
  → 各セルの重心がポリゴン内に含まれるか判定
  → Vec<bool>（湖セル = true）
```

最小面積フィルタとして、面積 1,500 km² 未満の湖は除外する。

これはL=6正二十面体分割における1セルあたりの平均面積（約6,200 km²）の1/4に相当する。
セルより小さい湖は重心一致で安定して検出できないため、評価対象から除く。
チャド湖（〜1,350 km²、変動大）はこのフィルタで除外され、既知の限界として扱う。

#### F1 計算

```rust
fn f1(pred: &[bool], truth: &[bool]) -> (f32, f32, f32) {
    let tp = pred.iter().zip(truth).filter(|(p, t)| **p && **t).count() as f32;
    let fp = pred.iter().zip(truth).filter(|(p, t)| **p && !**t).count() as f32;
    let fnn = pred.iter().zip(truth).filter(|(p, t)| !**p && **t).count() as f32;
    let precision = tp / (tp + fp);
    let recall    = tp / (tp + fnn);
    let f1        = 2.0 * precision * recall / (precision + recall);
    (precision, recall, f1)
}
```

陸セルのみで計算する（`geology.height > 0` のセルに限定）。
湖候補領域への事前限定は行わず、全陸セルを評価母集団とする。
海セルは常に `is_lake=false` となり Precision を見かけ上押し上げるため、評価から除外する。

#### 評価方針

主評価 1-B は閾値判定を行わず、Precision・Recall・F1 の生スコアを記録して比較する。

---

### 1-C：参考値（erosion_rate・deposition_rate）

実データが粗いため主評価には含めない。シミュレーション出力の絶対値と分布形状を記録するにとどめる。

---

## 補助評価：代表地点診断

主評価のスコアが変動した原因を掘り下げるために使う。
アサーションは `matched/total` と `coverage_ratio` を記録し、前後差で診断する。

### 2-A：主要河川の流量大小関係

各行は `left > right`（左が右より大流量）であるべき関係を示す。
比較は代表セルの `river_flow` 値を使う。

| # | left（大流量） | right（小流量） | 根拠 |
|---|---|---|---|
| R-01 | `amazon_mouth` | `congo_mouth` | アマゾン > コンゴ（世界1位 vs 2位） |
| R-02 | `congo_mouth` | `mississippi_mouth` | コンゴ > ミシシッピ |
| R-03 | `amazon_mouth` | `nile_mouth` | 熱帯湿潤大河 vs 乾燥域外来河川 |
| R-04 | `himalaya_foothills` | `sahara_interior` | 急勾配・高降水 vs 乾燥無流域 |
| R-05 | `ganges_delta` | `sahara_interior` | 季節河川・モンスーン vs 乾燥無流域 |

### 2-B：代表セルの流量特性確認

代表セルの `river_flow` 絶対値を出力し、目視で異常値を確認する。
数値アサーションは設けず、出力フォーマットに値を並べることで診断材料とする。

| 地域ID | 期待する特性 |
|---|---|
| `amazon_mouth` | 極大（世界最大クラス） |
| `ganges_delta` | 大（MFD分散が効いているため単セルは中程度になる可能性あり） |
| `himalaya_foothills` | 中〜大（MFD集中） |
| `sahara_interior` | 極小〜ゼロ |

---

## キャッシュのバイナリ形式

`benches/scripts/resample.py` / `benches/rust/benches/hydrology_solo.rs` 実装。

### hydro_input.bin

1. magic: `HYDINPUT1`（9 bytes）
2. version: `u32` little-endian（現行 `1`）
3. cell_count: `u64` little-endian
4. `runoff` の `f32` little-endian 配列（`cell_count` 件、単位: mm/年）

欠損値は `runoff` では `f32::NAN` とする。

### hydro_ref.bin

1. magic: `HYDROREF1`（9 bytes）
2. version: `u32` little-endian（現行 `1`）
3. cell_count: `u64` little-endian
4. `river_flow` の `f32` little-endian 配列（`cell_count` 件、単位: m³/s）
5. `is_lake` の `u8` 配列（`cell_count` 件、`1` = 湖、`0` = 非湖）

欠損値は `river_flow` では `f32::NAN`、`is_lake` では `0`（非湖扱い）とする。

---

## 出力フォーマット

標準出力に以下の形式で出力する。

```
=== Hydrology Solo Bench ===

-- Main Evaluation 1-A: river_flow Spearman (log scale, land cells only) --
river_flow:  rho=0.741

-- Main Evaluation 1-B: is_lake F1 (land cells only) --
precision=0.412  recall=0.638  f1=0.501

-- Main Evaluation 1-C: Reference Only --
erosion_rate:    mean=0.0031  p50=0.0018  p95=0.0089
deposition_rate: mean=0.0024  p50=0.0014  p95=0.0071

-- Diagnostic Evaluation 2-A: River Flow Ranking Assertions --
R-01  amazon_mouth > congo_mouth:       match  (182340.0 vs 41000.0)
R-02  congo_mouth > mississippi_mouth:  match  (41000.0 vs 16800.0)
R-03  amazon_mouth > nile_mouth:        match  (182340.0 vs 2830.0)
R-04  himalaya_foothills > sahara_interior: match  (4120.0 vs 0.1)
R-05  ganges_delta > sahara_interior:   match  (6800.0 vs 0.1)

-- Diagnostic Evaluation 2-B: Representative Cell Values --
amazon_mouth:      river_flow=182340.0
congo_mouth:       river_flow=41000.0
mississippi_mouth: river_flow=16800.0
yangtze_mouth:     river_flow=30200.0
nile_mouth:        river_flow=2830.0
sahara_interior:   river_flow=0.1
himalaya_foothills: river_flow=4120.0
ganges_delta:      river_flow=6800.0

-- Main Evaluation 1-A Summary: metrics_reported=1 --
-- Main Evaluation 1-B Summary: metrics_reported=3 --
-- Diagnostic Evaluation 2-A Summary: matched=5/5 coverage_ratio=1.000 --
-- Main Evaluation State: READY --
-- Score Save: OK --
```

---

## 実データ未整備時の暫定運用

`benches/data/terrain_ref.bin` が存在しない場合、ベンチは実行せず終了する。
`benches/data/hydro_input.bin` が存在しない場合、ベンチは実行せず終了する。
`benches/data/hydro_ref.bin` が存在しない場合、主評価はスキップして補助評価のみ実行する。

補助評価は実データ不要（代表セルのシミュレーション出力値同士を比較するだけ）のため、
実データ整備前から即座に実行できる。

```
=== Hydrology Solo Bench ===

-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --
To generate:
  1) pnpm bench:dump-centroids
  2) pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
```

```
=== Hydrology Solo Bench ===

-- Hydro Input: SKIPPED (benches/data/hydro_input.bin not found) --
To generate:
  pnpm bench:resample:hydro-input -- --runoff <path>
```

```
=== Hydrology Solo Bench ===

-- Main Evaluation: SKIPPED (benches/data/hydro_ref.bin not found) --
To generate:
  pnpm bench:resample:hydro-ref -- --river-flow <path> --lakes <path>

-- Diagnostic Evaluation 2-A: River Flow Ranking Assertions --
（以下、通常通り出力）
```

---

## リサンプリングツール（`benches/scripts/resample.py` への追加）

Climate単体ベンチで実装した `resample.py` に、Hydrology単体ベンチ向けの入力生成と評価データ生成を追加する。

CLI 契約は次の通りとする。

- `--module hydro-input`
- 必須引数は `--runoff`
- 任意引数は `--var-name runoff=<name>`
- 出力既定値は `benches/data/hydro_input.bin`
- `--module hydro-ref`
- 必須引数は `--river-flow` と `--lakes`
- 出力既定値は `benches/data/hydro_ref.bin`

```bash
python benches/scripts/resample.py --module hydro-input \
  --centroids benches/data/cell_centroids.csv \
  --runoff benches/raw/climate/era5_land_annual_1970_2000.nc \
  --var-name runoff=runoff_mm_yr \
  --output benches/data/hydro_input.bin
```

```bash
python benches/scripts/resample.py --module hydro-ref \
  --centroids benches/data/cell_centroids.csv \
  --river-flow benches/raw/hydrology/glofas_era5_annual_mean.nc \
  --lakes benches/raw/hydrology/HydroLAKES_polys_v10.shp \
  --output benches/data/hydro_ref.bin
```

処理手順：

1. `hydro-input`
2. ERA5-Land NetCDF を読み、年平均runoffグリッドを取得する
3. CellStore のセル重心座標一覧（`benches/data/cell_centroids.csv`）を読む
4. 各セルの重心座標でバイリニア補間し、`runoff` を `hydro_input.bin` に保存する
5. `hydro-ref`
6. GloFAS-ERA5 NetCDF を読み、近似複数年平均流量グリッドを取得する
7. HydroLAKES シェープファイルを読み、面積 1,500 km² 以上の湖ポリゴンを抽出する
8. `river_flow` は各セルの重心座標でバイリニア補間する
9. `is_lake` は各セルの重心が湖ポリゴン内に含まれるか判定する
10. 結果を上記バイナリ形式で保存する

依存ライブラリ（追加分）：

- `xarray`, `netCDF4`（GloFAS-ERA5 NetCDF 読込）
- `geopandas`, `shapely`（HydroLAKES ポリゴン処理）

---

## スコア保存フロー

`cargo bench --manifest-path rust/Cargo.toml --bench hydrology_solo` 実行時に、
補助評価要約と主評価生スコアをJSONLへ追記保存する。

- 保存先: `benches/results/hydrology_main_scores.jsonl`
- 1実行 = 1行（時刻、seed、mesh_level、cell_count、river_flow_rho、is_lake_precision/recall/f1、補助評価要約）

---

## 既知の限界（モデルの表現範囲外）

以下は現行モデルの設計上の限界であり、ベンチでズレが出ても修正対象ではなく「モデルの限界」として記録する。

- 内水面の季節変動（年平均のみ評価）
- 地下水・湧水由来の流量（Hydrologyは地表流のみを扱う）
- 人工ダム・取水による流量改変（FeedbackQueue経由の将来実装まで未考慮）
- チャド湖など面積が1,500 km²を下回る・または変動が大きい湖（フィルタで除外）

関連:

- `docs/architecture/module_boundaries.md`
- `docs/architecture/data_model.md`
- `docs/modules/hydrology/hydrology.md`
- `docs/manage/bench/climate_solo_benchmark.md`
