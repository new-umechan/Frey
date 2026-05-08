# Subsistence の詳細仕様

## 目的

`Subsistence` は、`Hydrology` / `Ecology` / `Domesticates` / `Population` を読み、
生業構成と食料供給特性を更新する。

`Domesticates` の後、`Population` / `Settlement` の前に実行される。

## 公開 state

- `subsistence_mix`
- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`
- `mobility_capacity`
- `land_use_intensity`

## 入力

- `geology.height`
- `hydrology.surface_water_access`
- `hydrology.river_flow`
- `hydrology.is_lake`
- `ecology.tree_cover`
- `ecology.ground_cover`
- `ecology.soil_fertility`
- `domesticates.crop_adoption`
- `domesticates.livestock_adoption`
- `population.population`
- `projection.terrain.is_coastal`
- 前 tick の `subsistence_mix`

## `SubsistenceMix`

```rust
struct SubsistenceMix {
    gathering:   f32,
    hunting:     f32,
    fishing:     f32,
    cultivation: f32,
    herding:     f32,
}
```

各軸は `0.0..=1.0`、合計は `1.0` に正規化する。

## 更新の要点

1. `Hydrology.surface_water_access` を水アクセス入力として使う。
2. `fishing` は内部で内水面と沿岸アクセスを合成する。
3. `Population.population` から人口圧を導き、`cultivation` と土地利用強度へ反映する。
4. `food_energy_mean`（供給平均）と `food_energy_variance`（供給変動）を分ける。
5. `buffer_capacity` と `mobility_capacity` を別列で保持する。

## 下流利用

- `Population`:
  `food_energy_mean` / `food_energy_variance` / `buffer_capacity` / `surface_water_access`
- `Settlement`:
  `subsistence_mix` / `food_energy_mean` / `food_energy_variance` /
  `buffer_capacity` / `mobility_capacity` / `surface_water_access`

## 補足

`Hydrology.surface_water_access` を水アクセスの正本として扱う。
