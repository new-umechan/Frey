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

## 内部システム

- `AccessSystem`:
  `ecology.*`、`hydrology.surface_water_access`、`terrain.is_coastal` から
  `inland_aquatic_access` / `coastal_aquatic_access` を含むアクセス状態を導出する
- `CapabilitySystem`:
  `crop_adoption` / `livestock_adoption` から利用能力を導出する
- `PressureSystem`:
  `population.population` から人口圧を導出する
- `StrategySystem`:
  access / capability / pressure と前 tick の `subsistence_mix` から
  次の `SubsistenceMix` を更新する
- `OutputSystem`:
  `SubsistenceMix` とアクセス・圧力状態から
  `food_energy_mean` / `food_energy_variance` / `buffer_capacity` /
  `mobility_capacity` / `land_use_intensity` を更新する

## 下流利用

- `Population`:
  `food_energy_mean` / `food_energy_variance` / `buffer_capacity` / `surface_water_access`
- `Settlement`:
  `subsistence_mix` / `food_energy_mean` / `food_energy_variance` /
  `buffer_capacity` / `mobility_capacity` / `surface_water_access`

## 補足

`Hydrology.surface_water_access` を水アクセスの正本として扱う。
