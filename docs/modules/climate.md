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

Climateは`World State`と`Exec State`だけを読む。
他モジュールのメソッドは直接呼ばない。

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

降水量は、現実の地球を模した経験的近似で求める。
大気大循環そのものは計算しない。

理由は次のとおり。

- 毎tickで全球大気循環を解くのは重い
- 現行の世界生成と時代進行に対して費用対効果が低い
- 必要なのは厳密な天気予報ではなく、生態と水文に使える安定した気候場である

降水は次の順で補正する。

1. 緯度帯ごとの基準降水量
2. 卓越風向に対する風上増雨と風下減雨
3. 海からの距離による大陸度補正
4. 寒流沿岸とその風下1から3セルへの乾燥補正

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

## Hydrologyとの責務分担

Climateは局所的な水収支を担当する。
河川の流路決定と集積流量はHydrologyが担当する。

したがって、Climateは`runoff`までを書き、`river_flux`は書かない。
HydrologyはClimateが書いた`runoff`を読んで河川流量を集積する。

関連:

- `docs/architecture/module_boundaries.md`
- `docs/architecture/data_model.md`
- `docs/modules/hydrology/hydrology.md`
