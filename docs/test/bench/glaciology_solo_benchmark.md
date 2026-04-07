# Glaciology単体ベンチ（詳細仕様）

## 概要

入力として実地形（`geology.height`、固定）と実気候データ（`climate.temperature`、`climate.precipitation`）を与え、
1 tick実行した結果の `ice_thickness`・`glacial_melt_runoff` を評価する。
`accumulation`・`ablation` は直接実測が困難なため参考値として記録する。

Glaciologyモジュール単体の評価が目的であり、Climateの誤差を混入させないため
地形と気候は他モジュールの出力ではなく実データ由来の参照値を直接入力する。

実行seedは `earth` 固定とし、参照実データと地形前提を一致させる。

## 実行コマンド

```sh
# repo root から pnpm wrapper で実行
pnpm run bench --suite glaciology_solo

# 旧形式（互換）
pnpm run bench -- --suite glaciology_solo

# または cargo bench を直接実行
# repo root から実行
cargo bench --manifest-path rust/Cargo.toml --bench glaciology_solo

# もしくは rust/ 配下で実行
cd rust
cargo bench --bench glaciology_solo
```

## 入力の準備

このベンチは、実地形キャッシュ `benches/data/terrain_ref.bin` と、
実気候キャッシュ `benches/data/climate_ref.bin`、
氷厚参照キャッシュ `benches/data/glaciology_ref.bin` を使って比較する。

- `terrain_ref.bin`
  - `height`
- `climate_ref.bin`
  - `temperature`
  - `precipitation`
- `glaciology_ref.bin`
  - `ice_thickness`

現在の実運用では、リポジトリルートで次の順に準備する。

1. `pnpm bench:dump-centroids`（未実行の場合のみ）
2. `pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`（未実行の場合のみ）
3. `pnpm bench:prepare:worldclim`（未実行の場合のみ）
4. `pnpm bench:prepare:era5`（未実行の場合のみ）
5. `pnpm bench:resample:climate -- --temperature benches/raw/climate/worldclim_tavg_annual_c.tif --precipitation benches/raw/climate/worldclim_prec_annual_mm.tif --evapotranspiration benches/raw/climate/era5_land_annual_1970_2000.nc --var-name evapotranspiration=evapotranspiration_mm_yr --runoff benches/raw/climate/era5_land_annual_1970_2000.nc --var-name runoff=runoff_mm_yr --aridity benches/raw/climate/ai_et0.tif --aridity-source precip_over_pet_x10000`
6. `pnpm bench:resample:glaciology-ref -- --ice-thickness benches/raw/glaciology/millan_2022_ice_thickness.tif`

氷厚データには Millan et al. 2022 の全球氷厚推定データを使用する。
生データ配置が `benches/raw/glaciology/` 配下でネストされている場合は、
`--ice-thickness` に実ファイルパスを直接指定する。

| フィールド | 型 | 値 |
|---|---|---|
| `geology.height` | `Vec<f32>` | 実地形データを内部標高単位へ変換した値（`height * 6000 = m`） |
| `climate.temperature` | `Vec<f32>` | `climate_ref.bin` から読む年平均気温（単位: ℃） |
| `climate.precipitation` | `Vec<f32>` | `climate_ref.bin` から読む年間降水量（単位: mm/年） |
| `ecology.tree_cover` | `Vec<f32>` | 全セル `0.5` で固定 |
| `ecology.ground_cover` | `Vec<f32>` | 全セル `0.5` で固定 |

---

## セル選定の方法

Climate単体ベンチと同じ方式を踏襲する。
「指定した緯度経度に最も近い重心を持つセル」を選定セルとする。

```rust
fn nearest_cell(positions: &[[f32; 3]], lat: f32, lon: f32) -> usize {
    positions
        .iter()
        .enumerate()
        .map(|(index, pos)| {
            let cell_lat = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
            let cell_lon = pos[2].atan2(pos[0]).to_degrees();
            let dist = haversine_km(cell_lat, cell_lon, lat, lon);
            (index, dist)
        })
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}
```

代表地域の指定緯度経度は以下の通り。
各地域は氷河・氷床の分布バリエーションを網羅するよう選定した。

| 地域ID | 地域名 | 緯度 | 経度 | 氷河特性 |
|---|---|---|---|---|
| `greenland_center` | グリーンランド中央部 | 75.0 | -40.0 | 大陸氷床・最大級氷厚 |
| `antarctica_inland` | 南極内陸 | -80.0 | 0.0 | 大陸氷床・最大級氷厚 |
| `patagonia` | パタゴニア氷原 | -50.0 | -73.0 | 中緯度氷原 |
| `alaska_range` | アラスカ山脈 | 63.0 | -150.0 | 山岳氷河 |
| `himalaya_core` | ヒマラヤ中央部 | 28.0 | 86.0 | 山岳氷河・氷厚大 |
| `karakoram` | カラコルム | 36.0 | 76.0 | 山岳氷河・氷厚大 |
| `alps` | アルプス | 46.5 | 8.0 | 山岳氷河・氷厚中 |
| `rockies` | ロッキー山脈 | 51.0 | -116.0 | 山岳氷河・氷厚小〜中 |
| `andes_tropical` | 熱帯アンデス | -8.0 | -77.0 | 熱帯山岳氷河 |
| `sahara` | サハラ中部 | 23.0 | 13.0 | 氷河なし（対照） |

---

## 主評価：全球比較

### 1-A：`ice_thickness` の Spearman 相関

#### 実データソース

| 変数 | データソース | 解像度 | 取得先 |
|---|---|---|---|
| `ice_thickness` | Millan et al. 2022（全球氷厚推定） | 変解像度 | https://doi.org/10.1038/s41561-021-00885-z |

#### リサンプリング手順

Climate単体ベンチのリサンプリング基盤と同一の手順を踏襲する。

```
氷厚グリッド（緯度経度ラスタ）
  → 各セルの重心座標（latitude, longitude）でバイリニア補間
  → セルごとの実データ値 Vec<f32>
```

#### 陸セル限定

陸セルのみで計算する（`geology.height > 0` のセルに限定）。

#### 評価方針

主評価は閾値判定を行わず、`rho` の生スコアを記録して比較する。
モデル変更の判断は同一条件での前後差で行う。

---

## 補助評価：代表地域診断

主評価のスコアが変動した原因を掘り下げるために使う。
アサーションは `matched/total` と `coverage_ratio` を記録し、前後差で診断する。

### 2-A：`ice_thickness` の大小関係

各行は `left > right`（左が右より氷厚が厚い）であるべき関係を示す。

| # | left（氷厚大） | right（氷厚小） | 根拠 |
|---|---|---|---|
| ICE-01 | `greenland_center` | `himalaya_core` | 大陸氷床 > 山岳氷河 |
| ICE-02 | `antarctica_inland` | `patagonia` | 南極氷床 > パタゴニア氷原 |
| ICE-03 | `alps` | `andes_tropical` | アルプス > 熱帯アンデス（氷河規模） |
| ICE-04 | `himalaya_core` | `alps` | ヒマラヤ > アルプス |
| ICE-05 | `alaska_range` | `rockies` | アラスカ > ロッキー |
| ICE-06 | `patagonia` | `alaska_range` | パタゴニア氷原 > アラスカ山岳氷河 |
| ICE-07 | `karakoram` | `andes_tropical` | カラコルム > 熱帯アンデス |
| ICE-08 | `greenland_center` | `alps` | グリーンランド > アルプス |
| ICE-09 | `antarctica_inland` | `himalaya_core` | 南極 > ヒマラヤ |
| ICE-10 | `greenland_center` | `sahara` | 氷床 > 氷河なし |

### 2-B：`glacial_melt_runoff` の大小関係（known-hard）

各行は `left > right` であるべき関係を示す。
実測データとの直接比較が困難なため、すべて known-hard 扱いとし、
通過率の分母から除外する。

| # | left | right | 根拠 |
|---|---|---|---|
| MELT-01 ⚠️ | `alps` | `greenland_center` | 温帯山岳氷河 > 極地氷床（融解量） |
| MELT-02 ⚠️ | `andes_tropical` | `antarctica_inland` | 熱帯山岳 > 南極内陸 |
| MELT-03 ⚠️ | `patagonia` | `himalaya_core` | 海洋性氷原 > 大陸性山岳氷河 |

⚠️ は known-hard フラグ。通過率の分母から除外する。

---

## キャッシュのバイナリ形式

`benches/scripts/resample.py` / `rust/benches/glaciology_solo.rs` 実装。

### glaciology_ref.bin

1. magic: `GLACREF1`（8 bytes）
2. version: `u32` little-endian（現行 `1`）
3. cell_count: `u64` little-endian
4. `ice_thickness` の `f32` little-endian 配列（`cell_count` 件、単位: m）

欠損値（氷河なしセル等）は `f32::NAN` とする。

---

## 出力フォーマット

標準出力に以下の形式で出力する。

```
=== Glaciology Solo Bench ===
-- Terrain Source: benches/data/terrain_ref.bin --
-- Climate Source: benches/data/climate_ref.bin --
-- Runtime Diagnostics: glaciology_step_ms=45.678 --

-- Main Evaluation: Spearman Correlation (land cells only) --
ice_thickness:    rho=0.712

-- Main Evaluation Summary: metrics_reported=1 --

-- Diagnostic Evaluation: Ranking Assertions --
[ice_thickness] matched=8/10  coverage_ratio=0.800
[glacial_melt_runoff] matched=2/3  coverage_ratio=0.667  (excl. 3 known-hard)

-- Known-Hard Assertions (reference only, not counted) --
MELT-01  alps > greenland_center:  match  (0.0234 vs 0.0012)
MELT-02  andes_tropical > antarctica_inland:  match  (0.0156 vs 0.0001)
MELT-03  patagonia > himalaya_core:  mismatch  (0.0189 vs 0.0201)

-- Diagnostic Evaluation Summary: metrics=2 mean_coverage_ratio=0.733 (excl. known-hard) --
-- Main Evaluation State: READY --
-- Score Save: OK --
```

---

## 実データ未整備時の暫定運用

`benches/data/terrain_ref.bin` が存在しない場合、ベンチは実行せず終了する。
`benches/data/climate_ref.bin` が存在しない場合、ベンチは実行せず終了する。
`benches/data/glaciology_ref.bin` が存在しない場合、主評価はスキップして補助評価のみ実行する。

補助評価は実データ不要（代表セルのシミュレーション出力値同士を比較するだけ）のため、
実データ整備前から即座に実行できる。

```
=== Glaciology Solo Bench ===

-- Terrain Input: SKIPPED (benches/data/terrain_ref.bin not found) --
To generate:
  1) pnpm bench:dump-centroids
  2) pnpm bench:resample:terrain -- --height benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif
```

```
=== Glaciology Solo Bench ===

-- Climate Input: SKIPPED (benches/data/climate_ref.bin not found) --
To generate:
  1) pnpm bench:dump-centroids
  2) pnpm bench:resample:climate -- --temperature <path> --precipitation <path> --evapotranspiration <path> --runoff <path> --aridity <path>
```

```
=== Glaciology Solo Bench ===

-- Main Evaluation: Spearman Correlation (land cells only) --
SKIPPED  (benches/data/glaciology_ref.bin not found)
To generate:
  1) pnpm bench:dump-centroids
  2) pnpm bench:resample:glaciology-ref -- --ice-thickness <path>

-- Diagnostic Evaluation: Ranking Assertions --
（以下、通常通り出力）
```

---

## リサンプリングツール（`benches/scripts/resample.py` への追加）

Glaciology単体ベンチ向けの入力生成は `resample.py` の `--module glaciology-ref` で行う。

CLI 契約は次の通りとする。

- `--module glaciology-ref`
- 必須引数は `--ice-thickness`
- 出力既定値は `benches/data/glaciology_ref.bin`

```bash
python benches/scripts/resample.py --module glaciology-ref \
  --centroids benches/data/cell_centroids.csv \
  --ice-thickness benches/raw/glaciology/millan_2022_ice_thickness.tif \
  --output benches/data/glaciology_ref.bin
```

ディレクトリ移動時の例:

```bash
python benches/scripts/resample.py --module glaciology-ref \
  --centroids benches/data/cell_centroids.csv \
  --ice-thickness benches/raw/glaciology/<nested-dir>/<ice-thickness-file>.tif \
  --output benches/data/glaciology_ref.bin
```

処理手順：

1. 氷厚 GeoTIFF/NetCDF を読む
2. CellStore のセル重心座標一覧（`benches/data/cell_centroids.csv`）を読む
3. 各セルの重心座標でバイリニア補間し、`ice_thickness` を `glaciology_ref.bin` に保存する

---

## スコア保存フロー

`cargo bench --manifest-path rust/Cargo.toml --bench glaciology_solo` 実行時に、
補助評価要約と主評価生スコアをJSONLへ追記保存する。

- 保存先: `benches/results/glaciology_main_scores.jsonl`
- 1実行 = 1行（`schema_version`、`run_id`、`repeat_index`、`git_commit`、`cache_fingerprint`、時刻、seed、mesh_level、cell_count、`runtime.glaciology_step_ms`、`runtime_stats`、Phase 2 メトリクス、Phase 1 要約）
- 実行ごとの差分比較はこのJSONLを入力に行う

---

## 既知の限界（モデルの表現範囲外）

以下は現行モデルの設計上の限界であり、ベンチでズレが出ても修正対象ではなく「モデルの限界」として記録する。

- 氷河動態の時間スケール（1 tick では平衡状態に達しない）
- 氷河流動の計算（氷厚は質量収支のみ、流動は考慮しない）
- 氷床の等静圧調整（簡易モデル）
- `glacial_melt_runoff` は水文モジュールへの入力として間接的に検証可能だが、直接実測データとの比較は困難
- `accumulation`・`ablation` は内部中間量であり、直接観測値との対応づけに恣意性が入る

関連:

- `docs/test/benchmark.md`
- `docs/modules/glaciology.md`
- `docs/architecture/module_boundaries.md`
