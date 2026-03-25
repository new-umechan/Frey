## Climate単体ベンチ（詳細仕様）

### 概要

入力として実地形（`geology.height`、固定地理量）と固定植生（`tree_cover = 0.5`、`ground_cover = 0.5` で全セル統一）を与え、
1 tick実行した結果の `temperature`・`precipitation`・`aridity`・`evapotranspiration`・`runoff` を評価する。

`ocean_temperature` は信頼度が低いため、このベンチでは参考出力にとどめ、合否判定に含めない。
実行seedは `earth` 固定とし、参照実データと地形前提を一致させる。

### 実行コマンド（現行）

```
# repo root から実行
cargo bench --manifest-path rust/Cargo.toml --bench climate_solo

# もしくは rust/ 配下で実行
cd rust
cargo bench --bench climate_solo
```

### 入力の準備

このベンチは、実地形キャッシュ `bench/data/terrain_ref.bin` と、以下5変数の気候キャッシュ `bench/data/climate_ref.bin` を使って比較する。

- `temperature`
- `precipitation`
- `evapotranspiration`
- `runoff`
- `aridity`

現在の実運用では、リポジトリルートで次の順に準備する。

1. `npm run bench:dump-centroids`
2. `npm run bench:resample:terrain -- --height data/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`
3. `npm run bench:prepare:worldclim`
4. `npm run bench:prepare:era5`
5. `npm run bench:resample:climate -- --temperature data/raw/climate/worldclim_tavg_annual_c.tif --precipitation data/raw/climate/worldclim_prec_annual_mm.tif --evapotranspiration data/raw/climate/era5_land_annual_1970_2000.nc --var-name evapotranspiration=evapotranspiration_mm_yr --runoff data/raw/climate/era5_land_annual_1970_2000.nc --var-name runoff=runoff_mm_yr --aridity data/raw/climate/ai_et0.tif --aridity-source precip_over_pet_x10000`

`bench:prepare:worldclim` の前提として、`data/raw/climate/` に `wc2.1_30s_tavg_01..12.tif` と `wc2.1_30s_prec_01..12.tif` を置く。
`bench:prepare:era5` の前提として、`data/raw/climate/era5_land_monthly_1970_2000.zip` を用意する（`npm run bench:fetch:era5` で取得可）。
`aridity` は `data/raw/climate/ai_et0.tif` を参照する。
`terrain` は海抜mのDEM（ETOPO 2022 **Ice Surface** 推奨）を指定し、内部標高単位（`height * 6000m`）へ変換して保存する。

| フィールド | 型 | 値 |
|---|---|---|
| `geology.height` | `Vec<f32>` | 実地形データを内部標高単位へ変換した値（`height * 6000 = m`） |
| `geo.latitude` | `Vec<f32>` | セル重心緯度（単位: 度、-90〜90） |
| `geo.distance_from_ocean` | `Vec<f32>` | 実データからリサンプリング済みの値（単位: km） |
| `geo.coast_side` | `Vec<CoastSide>` | 実データから導出済み |
| `geo.is_coastal` | `Vec<bool>` | 実データから導出済み |
| `ecology.tree_cover` | `Vec<f32>` | 全セル `0.5` で固定 |
| `ecology.ground_cover` | `Vec<f32>` | 全セル `0.5` で固定 |

---

### セル選定の方法

代表セルは緯度・経度の近傍探索で選ぶ。
CellStoreは正二十面体分割由来のため格子が非均一であり、ピンポイントの一致は期待しない。
「指定した緯度経度に最も近い重心を持つセル」を選定セルとする。

```rust
fn nearest_cell(cells: &CellStore, lat: f32, lon: f32) -> CellId {
	// haversine距離で全セルを走査し最近傍を返す
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

| 地域ID | 地域名 | 緯度 | 経度 | 気候特性 |
|---|---|---|---|---|
| `sahara` | サハラ中部 | 23.0 | 13.0 | 極乾燥・高温 |
| `arabia` | アラビア半島内陸 | 23.0 | 45.0 | 極乾燥・高温 |
| `amazon` | アマゾン盆地中央 | -3.0 | -60.0 | 極湿潤・高温 |
| `congo` | コンゴ盆地中央 | -1.0 | 24.0 | 極湿潤・高温 |
| `mediterranean` | 地中海沿岸（スペイン） | 40.0 | 0.0 | 夏乾燥・温暖 |
| `monsoon_india` | インド・デカン高原 | 20.0 | 77.0 | モンスーン（苦手領域） |
| `maritime_europe` | 西ヨーロッパ（フランス） | 47.0 | 2.0 | 西岸海洋性（苦手領域） |
| `siberia` | シベリア内陸 | 62.0 | 105.0 | 亜寒帯・極乾燥 |
| `tropics_maritime` | 熱帯海洋（太平洋） | 5.0 | 160.0 | 熱帯湿潤 |
| `andes_high` | アンデス高地 | -15.0 | -70.0 | 高標高・低温 |
| `arctic` | 北極圏 | 80.0 | 0.0 | 極寒 |
| `equator_africa` | 東アフリカ高原 | 0.0 | 37.0 | 高標高赤道 |

---

### Phase 2：Spearman相関（主指標）

#### 実データソース

| 変数 | データソース | 解像度 | 取得先 |
|---|---|---|---|
| `temperature` | WorldClim v2.1（年平均気温 `tavg`） | 2.5分（約5km） | https://worldclim.org/data/worldclim21.html |
| `precipitation` | WorldClim v2.1（年間降水量 `prec`） | 2.5分 | https://worldclim.org/data/worldclim21.html |
| `evapotranspiration` | ERA5-Land（年平均蒸発散） | 0.1度（約11km） | https://cds.climate.copernicus.eu |
| `runoff` | ERA5-Land（年平均流出） | 0.1度 | https://cds.climate.copernicus.eu |
| `aridity` | CGIAR Global Aridity Index v3 | 30秒（約1km） | https://cgiarcsi.community/data/global-aridity-and-pet-database/ |

WorldClimは月別ファイルが12枚あるため、年平均または年合計に集約してから使う（`temperature` は平均、`precipitation`・`runoff`・`evapotranspiration` は合計）。

#### リサンプリング手順

CellStore（正二十面体分割の約4万セル）と実データグリッドは解像度・投影が異なる。
比較前に実データをCellStoreのセル単位に変換する。

```
実データグリッド（緯度経度ラスタ）
  → 各セルの重心座標（latitude, longitude）でバイリニア補間
  → セルごとの実データ値 Vec<f32>
```

補間はバイリニアを基本とする（最近傍でも可。差は小さい）。
この変換を実行するツールは `tools/bench/resample.py` に実装する（後述）。
変換結果はバイナリキャッシュ（`bench/data/climate_ref.bin`）に保存し、毎回再計算しない。

```
bench/data/
  climate_ref.bin   # リサンプリング済み実データ（変数ごとのVec<f32>を直列化）
  hydro_ref.bin     # Hydrology用（別途）
  ecology_ref.bin   # Ecology用（別途）
```

キャッシュの論理構造：

```rust
struct ClimateRef {
	temperature:        Vec<f32>,   // セル数と同じ長さ。単位: ℃
	precipitation:      Vec<f32>,   // 単位: mm/年
	evapotranspiration: Vec<f32>,   // 単位: mm/年
	runoff:             Vec<f32>,   // 単位: mm/年
	aridity:            Vec<f32>,   // 無次元（高いほど乾燥）
}
```

欠損値（海セル等）は `f32::NAN` で格納し、Spearman計算時にペアごとに除外する。

キャッシュの物理バイナリ形式（`tools/bench/resample.py` / `rust/benches/climate_solo.rs` 実装）：

1. magic: `CLIMREF1`（8 bytes）
2. version: `u32` little-endian（現行 `1`）
3. cell_count: `u64` little-endian
4. `temperature` の `f32` little-endian 配列（`cell_count` 件）
5. `precipitation` の `f32` little-endian 配列
6. `evapotranspiration` の `f32` little-endian 配列
7. `runoff` の `f32` little-endian 配列
8. `aridity` の `f32` little-endian 配列

#### Spearman相関の計算手順

```rust
fn spearman(a: &[f32], b: &[f32]) -> f32 {
	// 1. NANペアを除外
	// 2. 両者を独立にランク変換（同率は平均ランク）
	// 3. ランク差の二乗和からρを計算: 1 - 6Σd²/(n(n²-1))
}
```

陸セルのみで計算する（`geology.height > 0` のセルに限定）。
海セルを含めると `temperature` の相関が見かけ上高くなり（海は均質）、モデル評価として意味がなくなる。

#### 評価方針（Phase 2）

Phase 2は閾値判定を行わず、`rho` の生スコアを記録して比較する。
モデル変更の判断は、同一条件での前後差（どの変数がどれだけ上がったか/下がったか）で行う。

---

### Phase 1：代表地域ランキング（診断ツール）

Phase 2のスコアが変動した原因を掘り下げるために使う。
合否判定は「アサーション一覧の何割が通るか」で表す。

#### 合否基準（Phase 1）

- **Pass**：アサーション通過率 ≥ 80%
- **Warn**：通過率 60〜80%
- **Fail**：通過率 < 60%

苦手領域（`monsoon_india`・`maritime_europe`）のアサーションは参考扱いとし、通過率の分母から除外してよい。

#### `temperature` アサーション

各行は `left > right`（左が右より高温）であるべき関係を示す。

| # | left | right | 根拠 |
|---|---|---|---|
| T-01 | `amazon` | `arctic` | 赤道 vs 北極圏 |
| T-02 | `sahara` | `siberia` | 亜熱帯砂漠 vs 亜寒帯 |
| T-03 | `congo` | `mediterranean` | 赤道 vs 中緯度 |
| T-04 | `mediterranean` | `siberia` | 中緯度温暖 vs 亜寒帯 |
| T-05 | `amazon` | `andes_high` | 低地熱帯 vs 高地（同緯度で標高差） |
| T-06 | `amazon` | `equator_africa` | 低地赤道 vs 高標高赤道（東アフリカ高原） |
| T-07 | `sahara` | `arctic` | 亜熱帯 vs 極地 |

#### `precipitation` アサーション

| # | left（高降水） | right（低降水） | 根拠 |
|---|---|---|---|
| P-01 | `amazon` | `sahara` | 熱帯雨林 vs 砂漠 |
| P-02 | `congo` | `arabia` | 熱帯雨林 vs 砂漠 |
| P-03 | `tropics_maritime` | `sahara` | 熱帯海洋 vs 砂漠 |
| P-04 | `amazon` | `siberia` | 熱帯 vs 亜寒帯内陸 |
| P-05 | `congo` | `mediterranean` | 熱帯湿潤 vs 地中海性 |
| P-06 ⚠️ | `maritime_europe` | `siberia` | 西岸海洋性 vs 大陸性内陸（苦手領域・参考） |
| P-07 ⚠️ | `monsoon_india` | `arabia` | モンスーン vs 砂漠（苦手領域・参考） |

⚠️ は苦手領域フラグ。通過率の分母から除外する。

#### `aridity` アサーション

| # | left（高aridity・乾燥） | right（低aridity・湿潤） | 根拠 |
|---|---|---|---|
| A-01 | `sahara` | `amazon` | 砂漠 vs 熱帯雨林 |
| A-02 | `arabia` | `congo` | 砂漠 vs 熱帯雨林 |
| A-03 | `siberia` | `amazon` | 亜寒帯内陸（乾燥） vs 熱帯湿潤 |
| A-04 | `sahara` | `mediterranean` | 極乾燥 vs 地中海性 |
| A-05 | `arabia` | `tropics_maritime` | 砂漠 vs 熱帯海洋 |

---

### 出力フォーマット

標準出力に以下の形式で出力する。

```
=== Climate Solo Bench ===

-- Phase 2: Spearman Correlation (land cells only) --
temperature:      rho=0.923
precipitation:    rho=0.612
aridity:          rho=0.588
evapotranspiration: rho=0.541
runoff:           rho=0.498

-- Phase 1: Ranking Assertions --
[temperature]  7/7 passed                        PASS
[precipitation] 5/5 passed  (excl. 2 known-hard) PASS
[aridity]       5/5 passed                       PASS

-- Known-Hard Assertions (reference only, not counted) --
P-06  maritime_europe > siberia:  FAIL  (624.0 vs 487.0)
P-07  monsoon_india > arabia:     PASS  (792.0 vs 88.0)

-- Phase 2 Summary: metrics_reported=5 --
-- Phase 1 Summary: 3/3 PASS (excl. known-hard) --
-- Phase 2 State: READY --
-- Score Save: OK --
```

---

### 実データ未整備時の暫定運用

`bench/data/terrain_ref.bin` が存在しない場合、ベンチは実行せず終了する。
`bench/data/climate_ref.bin` が存在しない場合、Phase 2はスキップしてPhase 1のみ実行する。

Phase 1は実データ不要（代表セルのシミュレーション出力値同士を比較するだけ）のため、
実データ整備前から即座に実行できる。

```
=== Climate Solo Bench ===

-- Terrain Input: SKIPPED (bench/data/terrain_ref.bin not found) --
To generate:
  1) npm run bench:dump-centroids
  2) npm run bench:resample:terrain -- --height data/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
```

```
=== Climate Solo Bench ===

-- Phase 2: Spearman Correlation (land cells only) --
SKIPPED  (bench/data/climate_ref.bin not found)
To generate:
  npm run bench:resample:climate -- --temperature <path> --precipitation <path> --evapotranspiration <path> --runoff <path> --aridity <path>

-- Phase 1: Ranking Assertions --
（以下、通常通り出力）
```

---

### リサンプリングツール（`tools/bench/resample.py`）

実データをCellStoreのセル単位に変換してキャッシュに保存するスクリプト。
ベンチ本体（Rust）の外部ツールとして実装する。

```
python tools/bench/resample.py --module climate \
  --centroids bench/data/cell_centroids.csv \
  --temperature path/to/temperature.tif \
  --precipitation path/to/precipitation.tif \
  --evapotranspiration path/to/evapotranspiration.tif \
  --runoff path/to/runoff.tif \
  --aridity path/to/aridity.tif \
  --output bench/data/climate_ref.bin
```

```bash
python tools/bench/resample.py --module terrain \
  --centroids bench/data/cell_centroids.csv \
  --height data/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif \
  --output bench/data/terrain_ref.bin
```

事前に重心CSVが必要な場合は以下を実行する。

```
npm run bench:dump-centroids
```

処理手順：

1. 変数ごとのGeoTIFF/NetCDFを読む
2. CellStoreのセル重心座標一覧（`bench/data/cell_centroids.csv`）を読む
   - 形式：`cell_id,latitude,longitude`
   - このCSVはシミュレーション初期化時に一度だけ書き出す（`--dump-centroids` オプション等で）
3. 各セルの重心座標でバイリニア補間
4. 結果を `ClimateRef` と同等の固定バイナリ形式（上記）で保存する

### スコア保存フロー（実測後）

`cargo bench --manifest-path rust/Cargo.toml --bench climate_solo` 実行時に、Phase 1要約とPhase 2生スコアをJSONLへ追記保存する。

- 保存先: `bench/results/climate_phase2_scores.jsonl`
- 1実行 = 1行（時刻、seed、mesh_level、cell_count、各指標のrho、Phase 1要約）
- 実行ごとの差分比較はこのJSONLを入力に行う

依存ライブラリ（Pythonツール群）：

- 共通: `numpy`
- WorldClim集約 / GeoTIFF読込: `rasterio`
- ERA5整形 / NetCDF読込: `xarray`, `netCDF4`
- ERA5ダウンロード: `cdsapi`
