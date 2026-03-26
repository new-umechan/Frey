# Ecology単体ベンチ（詳細仕様）

## 概要

入力として実気候データ（`temperature`、`precipitation`）、実水文データ（`river_flow`）、実地形（`height`）を与え、
Ecologyのみを収束まで実行した結果の `tree_cover`・`ground_cover`・`biome` を主指標として評価する。
`soil_fertility` は実データとの対応づけに恣意性が入るため参考値として記録する。
`disturbance` はSoloベンチでは外生フィードバックを無効化するため、評価対象外とする。

Ecologyモジュール単体の評価が目的であり、ClimateとHydrologyの誤差を混入させないため、
`temperature`・`precipitation`・`river_flow` は他モジュールの出力ではなく実データ由来の参照値を直接入力する。

実行seedは `earth` 固定とし、参照実データと地形前提を一致させる。

## 実行コマンド（予定）

```sh
# repo root から実行
cargo bench --manifest-path rust/Cargo.toml --bench ecology_solo

# もしくは rust/ 配下で実行
cd rust
cargo bench --bench ecology_solo
```

## 入力の準備

このベンチは、既存の `bench/data/terrain_ref.bin`・`bench/data/climate_ref.bin`・`bench/data/hydro_ref.bin` に加え、
Ecology単体ベンチ専用の評価キャッシュ `bench/data/ecology_ref.bin` を使う。
実データの取得元と保存先の運用は `docs/manage/bench/ecology_benchmark_data_acquisition.md` を参照する。

- `terrain_ref.bin`
  - `height`
- `climate_ref.bin`
  - `temperature`
  - `precipitation`
- `hydro_ref.bin`
  - `river_flow`
- `ecology_ref.bin`
  - `tree_cover`
  - `ground_cover`
  - `biome`
  - `soil_fertility`
  - `natural_mask`
  - `open_canopy_mask`

現時点では実装前提の仕様として、リポジトリルートで次の順に準備する。
既存の benchmark 用データを極力使い回し、Ecology 固有の参照データだけを追加する方針とする。

1. `npm run bench:dump-centroids`（未実行の場合のみ）
2. `npm run bench:resample:terrain -- --height bench/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`（未実行の場合のみ）
3. `npm run bench:prepare:worldclim`（未実行の場合のみ）
4. `npm run bench:prepare:era5`（未実行の場合のみ）
5. `npm run bench:resample:climate -- --temperature bench/raw/climate/worldclim_tavg_annual_c.tif --precipitation bench/raw/climate/worldclim_prec_annual_mm.tif --evapotranspiration bench/raw/climate/era5_land_annual_1970_2000.nc --var-name evapotranspiration=evapotranspiration_mm_yr --runoff bench/raw/climate/era5_land_annual_1970_2000.nc --var-name runoff=runoff_mm_yr --aridity bench/raw/climate/ai_et0.tif --aridity-source precip_over_pet_x10000`
6. `npm run bench:resample:hydro-ref -- --river-flow bench/raw/hydrology/glofas_era5_annual_mean.nc --lakes bench/raw/hydrology/HydroLAKES_polys_v10.shp`
7. Ecology 参照データを `bench/raw/ecology/` に配置する
8. `npm run bench:resample:ecology-ref:with-soil` で `bench/data/ecology_ref.bin` を生成する

### 既存データの再利用

Ecology 単体ベンチの入力のうち、次は既存データをそのまま使い回す。

| 用途 | 既存ファイル | 備考 |
|---|---|---|
| 実地形 | `bench/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif` | Climate/Hydrology benchmark と共用 |
| 実気温 | `bench/raw/climate/worldclim_tavg_annual_c.tif` | Climate benchmark と共用 |
| 実降水 | `bench/raw/climate/worldclim_prec_annual_mm.tif` | Climate benchmark と共用 |
| 実河川流量参照 | `bench/raw/hydrology/glofas_era5_annual_mean.nc` | Hydrology benchmark と共用 |
| 湖参照 | `bench/raw/hydrology/HydroLAKES_polys_v10.shp` | 必要なら湿地補助判定にも流用可 |
| 中間キャッシュ | `bench/data/terrain_ref.bin` / `bench/data/climate_ref.bin` / `bench/data/hydro_ref.bin` | そのまま入力に使う |

つまり、Ecology benchmark のために新規取得が必要なのは、Ecology 固有の参照正解データだけである。

### 新規に追加する Ecology 参照データ

次のファイルは現状の `bench/raw` には入っていないため、新規に追加する。

| 用途 | 配置先 | 備考 |
|---|---|---|
| MODIS VCF tree cover | `bench/raw/ecology/mod44b_tree_cover.tif` | `tree_cover` 参照 |
| MODIS VCF non-tree vegetation | `bench/raw/ecology/mod44b_non_tree_cover.tif` | `ground_cover` 参照 |
| MODIS VCF non-vegetated | `bench/raw/ecology/mod44b_non_vegetated.tif` | `biome` 合成参照 |
| MODIS Land Cover Type 1 | `bench/raw/ecology/mcd12q1_lc_type1.tif` | `natural_mask` と `biome` 合成に使う |
| MODIS Land Use / LCCS layer | `bench/raw/ecology/mcd12q1_lc_prop2.tif` | 農地・都市の除外に使う |
| SoilGrids 0-30cm 入力群 | `bench/raw/ecology/soilgrids/` | `soil_fertility` proxy 用 |

初版では、Ecology benchmark 用の raw データ配置規約だけを固定し、取得コマンドの自動化は後段に回す。
先にファイル名と入力契約を固定しておかないと、resample 実装の引数仕様も固まらないためである。

### 実データソース

| 参照値 | データソース | 役割 |
|---|---|---|
| `tree_cover` | MODIS Vegetation Continuous Fields `MOD44B` | 樹木被覆の主参照 |
| `ground_cover` | `MOD44B` の non-tree vegetation fraction | 草本・低木被覆の主参照 |
| `biome` | `MOD44B` + `MCD12Q1` + 実気候 + 実水文 + 実地形から合成 | 離散バイオーム参照 |
| `soil_fertility` | SoilGrids | 参考用の土壌肥沃度 proxy |

`MOD44B` / `MCD12Q1` / SoilGrids は benchmark の参照正解生成にのみ使う。
Ecology モジュールへの入力としては使わず、入力は従来どおり既存の climate / hydrology / terrain 参照を流用する。

運用上は、各ソースの取得年を完全一致させるよりも、長期平均で整合した静的参照を固定することを優先する。
初版では次の方針で固定する。

- 気候: 1970-2000 年平均
- 水文: 複数年平均
- 植生: 同一年の MODIS 年次プロダクト
- 土壌: 単年依存しない静的グリッド

### 入力フィールド

| フィールド | 型 | 値 |
|---|---|---|
| `geology.height` | `Vec<f32>` | 実地形データを内部標高単位へ変換した値（`height * 6000 = m`） |
| `climate.temperature` | `Vec<f32>` | `climate_ref.bin` から読む年平均気温（単位: ℃） |
| `climate.precipitation` | `Vec<f32>` | `climate_ref.bin` から読む年間降水量（単位: mm/年） |
| `hydrology.river_flow` | `Vec<f32>` | `hydro_ref.bin` から読む年平均流量（単位: m³/s） |
| `ecology.tree_cover` | `Vec<f32>` | 初期値 `0.0` |
| `ecology.ground_cover` | `Vec<f32>` | 初期値 `0.0` |
| `ecology.disturbance` | `Vec<f32>` | 初期値 `0.0`、外生フィードバックなし |
| `ecology.soil_fertility` | `Vec<f32>` | 初期値 `0.35` |

`feedback_value()` はすべてゼロとして扱い、伐採・放牧・焼畑・汚染などの外乱は投入しない。

---

## 収束条件

Ecology単体ベンチは 1 tick ではなく収束まで回す。
収束判定は「状態変化が十分小さい状態が連続して続いたか」で決める。

- `tree_cover` の陸セル P95 絶対変化量が `0.002` 未満
- `ground_cover` の陸セル P95 絶対変化量が `0.002` 未満
- `soil_fertility` の陸セル P95 絶対変化量が `0.001` 未満
- `biome` の変化セル比率が `0.001` 未満

上記4条件を 8 tick 連続で満たした時点で収束とみなす。
安全上限は 256 tick とし、到達しない場合は `NOT_CONVERGED` として結果に明示する。

補助指標として `ticks_to_converge` を必ず出力する。

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
各地域は Ecology の主要な判別境界を網羅するよう選定した。

| 地域ID | 地域名 | 緯度 | 経度 | 期待バイオーム |
|---|---|---|---|---|
| `amazon_core` | アマゾン盆地中央 | -3.0 | -60.0 | TropicalForest |
| `congo_core` | コンゴ盆地中央 | -1.0 | 24.0 | TropicalForest |
| `serengeti` | セレンゲティ | -2.5 | 34.8 | Savanna |
| `great_plains` | 北米グレートプレーンズ | 44.0 | -101.0 | Grassland |
| `sahara_core` | サハラ中部 | 23.0 | 13.0 | Desert |
| `europe_temperate` | 中央ヨーロッパ | 49.0 | 14.0 | TemperateForest |
| `siberia_taiga` | シベリアタイガ | 61.0 | 105.0 | BorealForest |
| `yamal_tundra` | ヤマル半島 | 70.0 | 70.0 | Tundra |
| `pantanal` | パンタナール | -17.0 | -57.0 | Wetland |
| `tibet_alpine` | チベット高原 | 32.0 | 86.0 | Alpine |

---

## 主評価：全球比較

### 1-A：`tree_cover` の Spearman 相関

#### 参照データ

`MOD44B` の tree cover fraction を使う。
評価時には 0..100 を 0..1 に正規化して比較する。

#### 評価母集団

- 陸セルのみ
- `natural_mask = true`

`natural_mask` は `MCD12Q1` から作る。
農地、都市、永久氷雪、水域は除外する。
Ecology単体ベンチは「自然植生の再現」を見るため、人為改変が支配的な土地利用は母集団から外す。

#### 評価方針

主評価は閾値判定を行わず、`rho` の生スコアを記録して比較する。
モデル変更の判断は同一条件での前後差で行う。

---

### 1-B：`ground_cover` の Spearman 相関

#### 参照データ

`MOD44B` の non-tree vegetation fraction を使う。
評価時には 0..100 を 0..1 に正規化して比較する。

#### 評価母集団

- 陸セルのみ
- `natural_mask = true`
- `open_canopy_mask = true`

`open_canopy_mask` は `tree_cover_ref <= 0.40` のセルで定義する。
密林内部では衛星の non-tree fraction とモデル内の `ground_cover` の意味がずれやすいため、
開放系植生に限定して比較する。

#### 評価方針

主評価は閾値判定を行わず、`rho` の生スコアを記録して比較する。

---

### 1-C：`biome` の macro F1

#### 参照データの作り方

`biome` は単一データセットから直接は取らず、以下の実地球データから参照ラベルを合成する。

- `MOD44B` tree cover fraction
- `MOD44B` non-tree vegetation fraction
- `MOD44B` non-vegetated fraction
- `MCD12Q1` land cover / land use
- `climate_ref.bin` の `temperature` / `precipitation`
- `hydro_ref.bin` の `river_flow`
- `terrain_ref.bin` の `height`

参照ラベルの決定順は次の通り。

1. `MCD12Q1` で農地・都市・水域・氷雪なら評価対象外
2. `height_m >= 2500` かつ `tree_cover_ref < 0.20` なら `Alpine`
3. `temperature <= -2.0` かつ `tree_cover_ref < 0.25` なら `Tundra`
4. `non_vegetated_ref >= 0.60` かつ `precipitation < 300` なら `Desert`
5. `MCD12Q1` が wetland 系、または `river_flow_ref` 上位 2% かつ低地なら `Wetland`
6. `temperature >= 22.0` かつ `tree_cover_ref >= 0.60` なら `TropicalForest`
7. `temperature >= 22.0` かつ `tree_cover_ref >= 0.10` なら `Savanna`
8. `temperature >= 6.0` かつ `tree_cover_ref >= 0.55` なら `TemperateForest`
9. `temperature < 6.0` かつ `tree_cover_ref >= 0.35` なら `BorealForest`
10. それ以外の自然陸地は `Grassland`

これは「衛星 land cover をそのまま採点対象にする」のではなく、
本プロジェクトの簡略バイオーム定義へ現実データを落とし込むための固定 crosswalk とみなす。

#### 指標

`biome` はクラス不均衡が大きいため、単純一致率ではなく macro F1 を主指標にする。
補助指標として overall accuracy も記録する。

#### 評価母集団

- 陸セルのみ
- `natural_mask = true`
- 参照ラベルが確定したセルのみ

---

### 1-D：`soil_fertility` の Spearman 相関（参考）

#### 参照データ

SoilGrids の topsoil 指標から benchmark専用 proxy を作る。
直接比較するのではなく、自然陸域内の相対順位を評価する。

使う入力は次の4変数。

- soil organic carbon
- cation exchange capacity
- pH
- bulk density

深さは 0-5 cm、5-15 cm、15-30 cm を重み付きで合成して 0-30 cm 平均を作る。
固定運用の重みは `5 : 3.5 : 1.5` とする。
これは厚み比の厳密モデルではなく、生物利用のしやすさを優先する benchmark 上の運用パラメータである。
実装では12ファイルを保持し、`bench:resample:ecology-ref:with-soil` 実行時にこの重みで合成する。

#### 参照 proxy

```rust
soil_fertility_ref =
    let soc_0_30 = weighted_mean(soc_0_5, soc_5_15, soc_15_30, [5, 3.5, 1.5]);
    let cec_0_30 = weighted_mean(cec_0_5, cec_5_15, cec_15_30, [5, 3.5, 1.5]);
    let ph_0_30 = weighted_mean(ph_0_5, ph_5_15, ph_15_30, [5, 3.5, 1.5]);
    let bdod_0_30 = weighted_mean(bdod_0_5, bdod_5_15, bdod_15_30, [5, 3.5, 1.5]);

    0.45 * percentile_rank(soc_0_30)
  + 0.25 * percentile_rank(cec_0_30)
  + 0.20 * ph_suitability(ph_0_30)
  + 0.10 * (1.0 - percentile_rank(bdod_0_30));
```

`ph_suitability` は `6.5` を最適値とする台形関数で定義する。
この値は benchmark専用の比較軸であり、農学的な絶対肥沃度そのものを意味しない。

#### 評価方針

主評価には含めない。
`rho` を記録し、設計変更で極端に悪化していないかを見る参考値とする。

---

## 補助評価：代表地域診断

主評価のスコアが変動した原因を掘り下げるために使う。
アサーションは `matched/total` と `coverage_ratio` を記録し、前後差で診断する。

### 2-A：`biome` ラベル診断

| # | 地域ID | 期待 |
|---|---|---|
| B-01 | `amazon_core` | TropicalForest |
| B-02 | `congo_core` | TropicalForest |
| B-03 | `serengeti` | Savanna |
| B-04 | `great_plains` | Grassland |
| B-05 | `sahara_core` | Desert |
| B-06 | `europe_temperate` | TemperateForest |
| B-07 | `siberia_taiga` | BorealForest |
| B-08 | `yamal_tundra` | Tundra |
| B-09 | `pantanal` | Wetland |
| B-10 | `tibet_alpine` | Alpine |

### 2-B：`tree_cover` の大小関係

各行は `left > right` であるべき関係を示す。

| # | left | right | 根拠 |
|---|---|---|---|
| T-01 | `amazon_core` | `serengeti` | 熱帯雨林 > サバンナ |
| T-02 | `congo_core` | `great_plains` | 熱帯雨林 > 草原 |
| T-03 | `europe_temperate` | `sahara_core` | 温帯林 > 砂漠 |
| T-04 | `siberia_taiga` | `yamal_tundra` | タイガ > ツンドラ |

### 2-C：`ground_cover` の大小関係

各行は `left > right` であるべき関係を示す。

| # | left | right | 根拠 |
|---|---|---|---|
| G-01 | `serengeti` | `sahara_core` | サバンナ > 砂漠 |
| G-02 | `great_plains` | `sahara_core` | 草原 > 砂漠 |
| G-03 | `pantanal` | `tibet_alpine` | 湿地低地 > 高山帯 |

---

## `ecology_ref.bin` の論理構造

```rust
struct EcologyRef {
    tree_cover:      Vec<f32>,  // 0..1, 欠損は NaN
    ground_cover:    Vec<f32>,  // 0..1, 欠損は NaN
    soil_fertility:  Vec<f32>,  // 0..1, 欠損は NaN
    biome:           Vec<u8>,   // RefBiome を u8 で保存、255 = 除外
    natural_mask:    Vec<u8>,   // 1 = 評価対象, 0 = 除外
    open_canopy_mask: Vec<u8>,  // 1 = 開放系植生, 0 = それ以外
}
```

`RefBiome` の符号化は、シミュレーション側の `Biome` と同順に固定する。
未定義セルや除外セルは `255` とする。

## `ecology_ref.bin` の物理バイナリ形式

`bench/scripts/resample.py` / `rust/benches/ecology_solo.rs` 実装。

1. magic: `ECOREF01`（8 bytes）
2. version: `u32` little-endian（現行 `1`）
3. cell_count: `u64` little-endian
4. `tree_cover` の `f32` little-endian 配列
5. `ground_cover` の `f32` little-endian 配列
6. `soil_fertility` の `f32` little-endian 配列
7. `biome` の `u8` 配列
8. `natural_mask` の `u8` 配列
9. `open_canopy_mask` の `u8` 配列

---

## 出力フォーマット

標準出力に以下の形式で出力する。

```text
=== Ecology Solo Bench ===

-- Run State --
converged:        true
ticks_to_converge: 41

-- Main Evaluation --
tree_cover:       rho=0.781
ground_cover:     rho=0.644
biome:            macro_f1=0.572 accuracy=0.691

-- Reference Evaluation --
soil_fertility:   rho=0.318

-- Diagnostic Evaluation: Assertions --
[biome]           matched=9/10 coverage_ratio=0.900
[tree_cover]      matched=4/4 coverage_ratio=1.000
[ground_cover]    matched=3/3 coverage_ratio=1.000

-- Main Evaluation Summary: metrics_reported=3 --
-- Reference Evaluation Summary: metrics_reported=1 --
-- Diagnostic Evaluation Summary: metrics=3 mean_coverage_ratio=0.967 --
-- Main Evaluation State: READY --
-- Score Save: OK --
```

収束に失敗した場合は次のように出力する。

```text
-- Run State --
converged:        false
ticks_to_converge: 256
run_state:        NOT_CONVERGED
```

この場合でも暫定スコアは出力してよいが、比較時には別扱いにする。

---

## 既知の限界

- `ground_cover` は森林下層植生を直接観測できないため、開放系植生に限定して評価する
- `soil_fertility` は直接観測量ではなく proxy 比較である
- `disturbance` は人為活動と自然撹乱が混ざるため、Soloベンチでは評価しない
- 農地・都市は人為改変が強く、自然植生 benchmark の母集団から除外する
