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
- 大気中水蒸気量（`climate.precipitable_water`）— 内部状態

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
実装はsubstep方式の水蒸気収支計算に基づく。

### 水蒸気・風束の前計算

1. 風場の構築
- 温度勾配から局地力学的強制（`baroclinic_grad`、`thermal_contrast`）を計算
- Hadley 循環・中緯度低圧・極東風帯から `wind_u` / `wind_v` を計算
- 垂直運動 proxy（`vertical_motion`）を循環・収束・地形リフトから導出

2. 水蒸気供給
- 飽和水蒸気量 `qsat` を気温から計算（Clausius-Clapeyron 式）
- 海洋セル：`evap = k_ocean * (qsat - humidity)^+`
- 陸セル：`evap = k_land * (0.35 * ET_prev + 0.65 * ocean_reach * (qsat - humidity)^+)`
- 初期湿度：`humidity = lerp(prior, 0.90 * qsat, spinup_relax)`

3. 湿潤収束・地形信号
- 風束収束 proxy：`convergence = -div(wind * humidity)`
- 地形性上昇信号：`rise_m`（風上トレースで標高差を積算）、`ocean_fetch`（風上側の海洋通過率）
- 昇降ゲート：`ascent_gate`（上昇運動）、`subsidence_gate`（沈降運動）

### 水蒸気収支の反復計算（substep）

各 substep で次を順に実行：

1. **蒸発供給**：海洋・陸からの蒸発散を `humidity` に加算
2. **平流輸送**：風向に沿って隣接セルへ水蒸気を再分配（`moisture_advection`）
3. **凝結・降水**：
   - 超過凝結：`excess_cond = k_excess * (humidity - qsat)^+ * (0.35 + 0.65 * ascent_gate)`
   - 上昇凝結：`lift_cond = k_lift * humidity * near_saturation * ascent_gate`
   - 地形性凝結：`orog_cond = k_orog * rise_m * (0.40 + 0.60 * ocean_fetch) * humidity * ascent_gate`
   - 全凝結量：`condensation = (excess_cond + lift_cond + orog_cond) * (1.0 - 0.55 * subsidence_gate)`
   - `humidity -= condensation`、`precip_column += condensation`

substep 数は `core_substeps + log10(real_years_per_tick)` で動的に増加（最大 24）。

### 陸セル降水の一次推定

substep 計算後に、陸セルの降水目標値を次で構成：

- 緯度帯背景降水 `P_bg`（ITCZ・中緯度・亜熱帯高圧帯・極乾燥帯のガウス重み）
- 水蒸気収束項：`P_conv = k_conv * convergence_gate`
- 地形性増雨：`P_orog = k_orog * rise_m * ocean_fetch`
- モンスーンブースト：`P_monsoon = 760 * gaussian(lat, 18, 14) * distance_weight * upwind_ocean * boost_gate`
- ホットスポットブースト：`P_hotspot = k_hotspot * distance_weight * ocean_fetch * upwind_ocean * rise_m`

概念式：

```text
P0 = (P_bg + P_conv + P_orog + P_monsoon + P_hotspot) * F_shadow * F_continental
P1 = min(P0, P_cap)
```

ここで：
- `F_shadow`：雨陰係数（風下側の地形下降で減衰）
- `F_continental`：大陸性係数（海からの距離で減衰）
- `P_cap`：可用水蒸気上限（`humidity * precip_cap_from_moisture`）

### 大気水収支のスケール調整

全セルの降水目標値合計が凝結供給量を超えないよう、グローバルスケールファクタを適用：
```text
scale = (condense_supply / precip_target_sum).clamp(0.55, 1.15)
precipitation = (P_final * scale).clamp(precip_min, precip_max)
```

## 気温・蒸発散・流出

### 気温

年平均気温:

```text
T_land = 30 * cos(lat_rad) - 5 - lapse_rate * elev_km
```

海水温は別式 `28 * cos(lat_rad) - 2` を基準に、海岸セルでは風向・湧昇流ベースの補正を加える。

湧昇流補正は沿岸風向とコリオリ力からエクマン輸送を計算:
```text
coriolis = |sin(lat_rad)|.max(0.05)
alongshore_wind = wind_v
ekman_transport = alongshore_wind / coriolis
upwelling_signal = -sign(lat) * ekman_transport.clamp(-8, 8) * 0.75
lat_mod = 1.0 + 0.3 * gaussian(|lat|, 20, 15)
coastal_decay = exp(-distance_from_ocean / 600)
offset = (upwelling_signal * lat_mod * coastal_decay).clamp(-8, 8)
```

初回tick（風向未初期化）では大気循環モデルに基づくフォールバック値を使用。

### 蒸発散

大気蒸発散需要（`atmospheric_evaporative_demand_mm`）は放射・空力・沈降項の和：

```text
insolation = (0.40 + 0.85 * cos(lat_rad)).clamp(0.18, 1.25)
cloudiness = (humidity / qsat).clamp(0.15, 1.6)
transmittance = (1.12 - 0.36 * cloudiness).clamp(0.45, 1.08)
radiation_limit = 210 + 1140 * insolation * transmittance

saturation_deficit = (qsat - humidity) / qsat
coastal_aero = exp(-distance / 900)
continentality = smoothstep(200, 2600, distance)
subtropical_subsidence = gaussian(|lat|, 27, 9)
thermal_contrast = ((T_land - T_ocean + 2) / 12).clamp(0, 1.8)

aerodynamic_term = 260 * saturation_deficit
                   * (0.55 + 0.25 * coastal_aero + 0.45 * continentality)
                   * (0.70 + 0.30 * thermal_contrast)
                   * dryness_boost

subsidence_demand = 180 * subtropical_subsidence * (0.4 + 0.6 * continentality)

PET = (radiation_limit + aerodynamic_term + subsidence_demand) * temp_gate
```

実蒸発散量（AET）は供給制限付き：

```text
vegetation_density = tree_cover + 0.6 * ground_cover * (1 - tree_cover)
et_potential = PET * (0.16 + 0.84 * vegetation_density).clamp(0.16, 1.0)
available = storage + precipitation
AET = min(et_potential, available)
```

### 流出・貯留・乾燥指数

バケツモデルで貯留・流出を計算：

```text
storage_cap = core_land_bucket_capacity_mm
climate_storage = storage_cap * (0.28 + 0.44 * humidity_ratio)
storage_next = lerp(prev_storage, climate_storage, land_relax)
  .clamp_by_gain(prev_storage, max_storage_gain = 0.28 * precip)

relief = local_relief_proxy()
relief_runoff = ((precip - AET).max(0) * (0.10 + 0.32 * relief)
                 * (1.0 - 0.35 * humidity_ratio)).clamp(0.55, 1.0)

runoff = (precip - AET - storage_change).max(0)
aridity = PET / max(precip, eps)
```

## 内部状態

### 大気中水蒸気量（`precipitable_water`）

水蒸気収支計算の内部状態。単位は mm。

- 初期値：`0.90 * qsat`（spinup 時は prior から緩和）
- 更新：蒸発供給、平流輸送、凝結沈殿の収支を substep で計算
- 下限：`core_humidity_floor_mm`（4.0 mm）

### 風・湿潤フラックス

- `wind_u` / `wind_v`：東西・南北風成分（単位なし、正規化ベクトル）
- `moisture_flux_u` / `moisture_flux_v`：湿潤フラックス（`humidity * |wind| * 0.75`）

## 診断出力

`PrecipDiagnosticsSummary` で最終 tick の降水補正比率を記録：

- `continental_reduction_ratio`：大陸性補正による減衰率
- `cap_reduction_ratio`：水蒸気上限による減衰率
- `cap_hit_ratio`：上限値に達した陸セルの比率
- `budget_residual_ratio`：大気水収支の残差比率

## パラメータ管理

気候パラメータは `config/climate.yaml` を正本とし、
`pnpm run config:sync` で
`rust/src/generated/climate_params_defaults.rs` を再生成する。

## 地理固定場

Climateの補助入力として、各セルに次の固定地理量を持つ。

- 緯度
- 海からの近似距離
- 海岸セルかどうか
- 東岸か西岸か

これらは地形初期化時に前計算して `World State` に保持し、毎tick再構築しない。

関連:

- `docs/reference/architecture/module_boundaries.md`
- `docs/reference/architecture/data_model.md`
- `docs/reference/modules/hydrology.md`
- `docs/reference/modules/glaciology.md`
