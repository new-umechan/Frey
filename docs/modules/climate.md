# Climateの詳細仕様

## 目的

Climateは、地形と固定地理量から各セルの年平均気候場を近似計算する。
毎tickで次の値を `World State` に書く。

- 気温（`climate.temperature`）
- 降水量（`climate.precipitation`）
- 実蒸発散量（`climate.evapotranspiration`）
- 流出量（`climate.runoff`）
- 乾燥指数（`climate.aridity`）
- 海水温（`climate.ocean_temperature`）
- 東西風成分（`climate.wind_u`）
- 南北風成分（`climate.wind_v`）
- 湿潤フラックス東西成分（`climate.moisture_flux_u`）
- 湿潤フラックス南北成分（`climate.moisture_flux_v`）

更新は `budget` に応じたブレンド係数 `alpha` で平滑化し、急変を抑える。

## 入力

Climateが読む主な値は次のとおり。

- `geology.height`
- `geo.latitude`（互換入力として `latitude_deg` も受理）
- `geo.distance_from_ocean`（互換入力として `distance_from_ocean_km` も受理）
- `geo.coast_side`
- `geo.is_coastal`
- `ecology.tree_cover`
- `ecology.ground_cover`
- `clock.epoch`

`Crust` / `Environment` では植生密度は既定値 `0.5` を使う。
`Life` 以降は `tree_cover` と `ground_cover` から次の proxy を使う。

```text
vegetation_density_proxy = clamp(
  tree_cover + 0.6 * ground_cover * (1 - tree_cover),
  0, 1
)
```

## 降水モデルの実装フロー

降水は「緯度帯背景 + 風・地形・海陸効果」の合成で計算する。
実装上の処理順は以下。

1. 風・水蒸気供給の前計算
- Hadley/中緯度/極域帯から `wind_u` / `wind_v` を計算
- 海水温と海からの距離から `moisture_source` を計算
- 風ベクトルと `moisture_source` から湿潤フラックスを構築

2. 陸セル降水の一次推定
- 緯度帯背景降水 `P_bg`
- フラックス収束由来の `P_conv`
- 風上トレース由来の地形性増雨 `P_orog`
- モンスーン加算 `P_monsoon`
- 雨陰係数 `F_shadow`
- 大陸性係数 `F_continental`（収束・増雨・モンスーンに応じて緩和）
- 可用水蒸気上限 `P_cap`（固定係数 + 動的ブースト）

概念式:

```text
P0 = (P_bg + P_conv + P_orog + P_monsoon) * F_shadow * F_continental
P1 = min(P0, P_cap)
```

3. 風下枯渇の反復
- `downwind_depletion_*` パラメータで、風上側降水消費を風下へ反復伝播

4. 寒流沿岸補正
- 海岸セルで寒流偏差がある場合に降水係数を減衰
- ただし収束・増雨・モンスーンが強い場合は減衰を緩和

## 気温・蒸発散・流出

### 気温

年平均気温:

```text
T_land = 30 * cos(lat_rad) - 5 - lapse_rate * elev_km
```

海水温は別式 `28 * cos(lat_rad) - 2` を基準に、海岸セルでは沿岸流補正を加える。

### 蒸発散

潜在蒸発散量（PET）は Thornthwaite を使う。
年平均気温しか公開保持しないため、内部で緯度依存の12か月仮想気温を生成して年積算する。

実蒸発散量（AET）は Fu式:

```text
phi = PET / P
w = 1.5 + 1.5 * vegetation_density
AET = P * (1 - (1 + phi^(-w))^(-1 / w))
```

### 流出・乾燥指数

```text
runoff = max(0, P - AET)
aridity = PET / max(P, eps)
```

## パラメータ管理

気候パラメータは `config/climate.yaml` を正本とし、同期スクリプト
`tools/sync/sync-climate-params.ts` で
`rust/src/generated/climate_params_defaults.rs` を再生成する。

## 地理固定場

Climateの補助入力として、各セルに次の固定地理量を持つ。

- 緯度
- 海からの近似距離
- 海岸セルかどうか
- 東岸か西岸か

これらは地形初期化時に前計算して `World State` に保持し、毎tick再構築しない。

関連:

- `docs/architecture/module_boundaries.md`
- `docs/architecture/data_model.md`
- `docs/modules/hydrology.md`
