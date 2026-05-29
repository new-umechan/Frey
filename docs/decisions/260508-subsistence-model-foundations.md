# Subsistence model は access ではなく capability・pressure・risk を含めて構成する

## Status

Accepted

## Context

`Subsistence` の従来案は、
環境条件から生業 mix と供給 proxy を主に導く簡略構成だった。

しかしこの形では、少なくとも次を落としてしまう。

- 漁撈の内水面 / 沿岸差
- 牧畜における mobility の中心性
- 人口圧による intensification
- 貯蔵や混合戦略による risk reduction
- 平均供給と供給変動の分離

この欠落は、考古学・人類学・人間生態学の一般的知見とズレる。

## Decision

`Subsistence` v1 は、単なる環境適合モデルにしない。
少なくとも次の因果を必須要件として持つ。

1. 資源 access と利用 capability を分ける
2. 平均供給と供給変動を分ける
3. 貯蔵・移動・混合戦略で安定性が改善する
4. 人口圧が intensification を押す
5. `fishing` は内部的に内水面と沿岸を分ける
6. 牧畜の安定性には mobility を効かせる
7. 定住性は農耕単独でなく、buffer と tethered resource にも依存させる

そのため、`Subsistence` は少なくとも次の system で構成する。

- `AccessSystem`
- `CapabilitySystem`
- `StrategySystem`
- `OutputSystem`
- `PressureSystem`

また、単一の `food_production` は廃止し、公開 state は少なくとも次へ移行する。

- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`
- `mobility_capacity`
- `land_use_intensity`

`surface_water_access` は `Hydrology` の責務とする。

## Consequences

- `Subsistence` は `Population.population` を読む
- `PressureSystem` は利用強度計算のため人口を読む
- `Settlement` は定住判定で `buffer_capacity` と `mobility_capacity` を読む
- 既存の `food_production` / `freshwater_access` 前提は破棄される
- 実装コストは増えるが、説明力は大きく上がる

## Follow-up

- 本文書の判断に合わせて `docs/reference/` の Subsistence 仕様を更新する
- `docs/reference/architecture/data_model.md` を新 state に合わせて更新する
- `docs/reference/architecture/module_boundaries.md` を新 read/write 境界に合わせて更新する
- `docs/reference/modules/subsistence.md` を再記述する
