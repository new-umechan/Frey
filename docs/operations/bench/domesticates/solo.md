# Domesticates単体ベンチ

## 概要

`domesticates` の単体ベンチは、生成世界の `niche_score` と `available` を
**取得可能な現代分布 proxy** に照らして評価する。

v1 の主評価は次の 4 指標。

- `crop_intensity_rho`
- `crop_presence_f1`
- `livestock_intensity_rho`
- `livestock_presence_f1`

補助として `regional_assertion_coverage` を記録する。
`origin_seed` と `adoption` は v1 の gate 対象にしない。

## v1 の対象種

定量評価する species:

- crops: `Wheat`, `Rice`, `Maize`, `Millet`, `Potato`, `Cassava`, `Sorghum`
- livestock: `Cattle`, `Horse`, `Sheep`, `Pig`

diagnostic assertion のみ:

- crops: `Yam`
- livestock: `Camel`

## 入力

既存 bench cache に加えて `benches/data/domesticates_ref.bin` を使う。

- `terrain_ref.bin`
- `climate_ref.bin`
- `hydro_ref.bin`
- `ecology_ref.bin`
- `domesticates_ref.bin`

準備順:

1. `pnpm bench:dump-centroids`
2. `pnpm bench:resample:terrain`
3. `pnpm bench:resample:climate`
4. `pnpm bench:resample:hydro-ref`
5. `pnpm bench:resample:ecology-ref:with-soil`
6. `pnpm bench:resample:domesticates-ref`

## `domesticates_ref.bin`

v1 形式:

1. magic: `DOMEREF2`
2. version: `u32`
3. `cell_count: u64`
4. crop observed intensity: `f32[cell_count * 7]`
5. livestock observed intensity: `f32[cell_count * 4]`
6. crop observed presence bitmap: `u8[cell_count]`
7. livestock observed presence bitmap: `u8[cell_count]`
8. crop evaluation mask: `u8[cell_count * 7]`
9. livestock evaluation mask: `u8[cell_count * 4]`

行レイアウトは row-major。

- crop index: `cell_id * 7 + species_idx`
- livestock index: `cell_id * 4 + species_idx`

## 参照値の意味

v1 は `suitability_ref` / `origin_mask` を持たない。
代わりに次の 2 層だけを持つ。

- `observed_intensity_ref`
  現代分布 raster を `log1p -> 1%/99% clip -> 0..1` 正規化した値
- `observed_presence_ref`
  species 固有 threshold で intensity を二値化したもの

この benchmark は「歴史的起源の再現」ではなく、
**環境適地モデルが実測 proxy とどれだけ整合するか** を測る。

## 取得元

v1 は半自動 curated 運用。
取得元 URL とローカル配置は `benches/raw/domesticates/manifest.json` を正本とする。

- crops: EarthStat harvested area raster
- livestock: FAO GLW density raster

`manifest.json` には少なくとも次を持つ。

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

## 評価

### 全球定量

- intensity: Spearman rho
- presence: F1

比較対象は陸セルかつ species ごとの `evaluation_mask == 1` のセルだけ。

### 診断 assertion

地域 assertion は quality gate の主指標ではないが、モデルの向きが壊れていないかを見る。

最低限残す比較:

- `Rice`: 東南アジア低地 > チベット高地
- `Millet` / `Sorghum`: サヘル > アマゾン
- `Potato`: アンデス > アマゾン
- `Horse`: ステップ > 湿潤森林縁
- `Pig`: 湿潤森林縁 > アラビア乾燥帯
- `Yam`, `Camel`: assertion only

## quality gate

v1 gate:

- runtime: baseline 比 +20% 以内
- `crop_intensity_rho`: baseline から 0.03 以上落とさない
- `crop_presence_f1`: baseline から 0.03 以上落とさない
- `livestock_intensity_rho`: baseline から 0.03 以上落とさない
- `livestock_presence_f1`: baseline から 0.03 以上落とさない
- `regional_assertion_coverage`: baseline から 0.05 以上落とさない

## 非対象

v1 では次をやらない。

- `origin_seed` の定量 gate
- `adoption` の実世界比較
- `Yam` / `Camel` の raster ベース定量比較
