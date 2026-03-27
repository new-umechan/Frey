# Climateの詳細仕様

## 目的

Climateは、地形と固定地理量から各セルの年平均気候場を近似計算する。
毎tickで次の値を`World State`へ書く。

- 気温
- 降水量
- 実蒸発散量
- 流出量
- 乾燥指数
- 海水温
- 東西風成分
- 南北風成分
- 湿潤フラックス東西成分
- 湿潤フラックス南北成分

Climateは`World State`と`Exec State`だけを読む。

## 入力

Climateが読む主な値は次のとおり。

- `geology.height`
- `geo.latitude_deg`
- `geo.distance_from_ocean_km`
- `geo.coast_side`
- `geo.is_coastal`
- `ecology.tree_cover`
- `ecology.ground_cover`

`Crust` / `Environment` では植生密度は既定値0.5を使う。`Life`以降は
`tree_cover` と `ground_cover` から、次の proxy をClimate内部で計算して使う。

```text
vegetation_density_proxy = clamp(
  tree_cover + 0.6 * ground_cover * (1 - tree_cover),
  0, 1
)
```

## 出力

Climateは次の配列を全セル分持つ。

- `climate.temperature`
- `climate.precipitation`
- `climate.evapotranspiration`
- `climate.runoff`
- `climate.aridity`
- `climate.ocean_temperature`
- `climate.wind_u`
- `climate.wind_v`
- `climate.moisture_flux_u`
- `climate.moisture_flux_v`

`ocean_temperature`は全セルに保持する。
海岸セルでは海流補正込み、非海岸セルでは緯度基準海水温を入れる。

## 近似モデル

### 気温

年平均気温は、緯度基準温度から標高逓減を引いて求める。

```text
T(lat, elev) = 30 * cos(lat_rad) - 5 - 6.5 * elev_km
```

実装では地形の無次元標高を近似的にmへ換算して使う。

### 降水

降水量は、年平均の水蒸気収支モデルで求める。
Hadley循環を中心に風場（`wind_u` / `wind_v`）と湿潤フラックス（`moisture_flux_u` / `moisture_flux_v`）を計算し、
次の合成で年間降水量を計算する。

```text
P = (P_bg(lat) + P_conv + P_orog) * F_shadow * F_continental * F_depletion
```

- `P_bg(lat)`: ITCZ・亜熱帯沈降帯・中緯度擾乱帯を含む緯度帯背景降水
- `P_conv`: 近傍フラックスの収束/発散から計算した降水偏差
- `P_orog`: 風上方向を2〜3ステップ追跡した累積持ち上げ量に応じた地形増雨
- `F_shadow`: 風上障壁高と障壁距離の減衰で決まる雨陰係数
- `F_continental`: 海からの距離で減衰する大陸度係数
- `F_depletion`: 風上セルでの降水消費を風下へ伝播した水蒸気枯渇係数

最後に、可用水蒸気量から導く上限をかけ、寒流沿岸の乾燥補正を適用する。
係数は `config/climate.yaml` から管理し、`tools/sync/sync-climate-params.mjs` で
`rust/src/generated/climate_params_defaults.rs` を生成して反映する。

### 蒸発散と流出

潜在蒸発散量はThornthwaite式を使う。
ただし公開状態としては年平均気温しか持たないため、内部では緯度に応じた12か月の仮想月平均気温を生成して年積算する。

実蒸発散量はFu式を使う。

```text
phi = PET / P
w = 1.5 + 1.5 * vegetation_density
E = P * (1 - (1 + phi^(-w))^(-1 / w))
```

流出量と乾燥指数は次で求める。

```text
runoff = max(0, P - E)
aridity = PET / P
```

## 地理固定場

Climateの補助入力として、各セルに次の固定地理量を持つ。

- 緯度
- 海からの近似距離
- 海岸セルかどうか
- 東岸か西岸か

これらは地形初期化時に前計算して`World State`へ保持する。
毎tickで再構築しない。

関連:

- `docs/architecture/module_boundaries.md`
- `docs/architecture/data_model.md`
- `docs/modules/hydrology/hydrology.md`
