# Ecologyの詳細設計

## 目的

物理的な地形や気候レイヤーから植生に落とし込むこと。
詳細な植生シミュレータではなく、あくまで人類の文化圏に
影響を与えうる範囲での近似計算をすることが目的。
毎tickで次の値を`World State`へ書く。

- `biome`
- `tree_cover`
- `ground_cover`
- `disturbance`
- `soil_fertility`

## 入力

- `ClimateState` （temperature, precipitation）
- `HydrologyState` （river_flow）
- `GeoState` （height）
- `FeedbackQueue` （Subsistence・Geology・Hydrologyからの変化量）
- 前tickの `EcologyState`

## 出力

全セル分の `EcologyState`：

```rust
struct EcologyState {
    // 公開I/O（他モジュールが読む）
    biome: Biome,

    // 永続状態（因果・長期記憶）
    tree_cover:     f32,  // 0..1
    ground_cover:   f32,  // 0..1
    disturbance:    f32,  // 0..1
    soil_fertility: f32,  // 0..1
}
```

### データ型の補足: biome

`docs/reference/architecture/data_model.md`の補足

非連続で恣意的なラベル。
下流で使うためにあるが、使用は最低限にとどめ、なるべく連続状態を参照するように。
内部計算には使用しない。

```rust
enum Biome {
    TropicalForest,
    Savanna,
    // Steppe: Tier2で Domesticates の実装時に必要なら追加
    Desert,
    Grassland,
    TemperateForest,
    BorealForest,
    Tundra,
    Wetland,
    Alpine,
}
```

## 処理ロジック

### 共通ヘルパー

```rust
/// ポテンシャルに向かって指数的に収束する
fn converge_toward(
    current: f32,
    potential: f32,
    rate_up: f32,    // potentialがcurrentより高い場合の速度
    rate_down: f32,  // potentialがcurrentより低い場合の速度
    dt: f32,
) -> f32 {
    let rate = if potential > current { rate_up } else { rate_down };
    current + (potential - current) * rate * dt
}
```

### biome

決定木。外部から参照するためのラベルとしての扱いで、内部計算には使用しない。

```rust
fn classify_biome(
    tree_cover: f32,
    ground_cover: f32,
    climate: &ClimateState,
    hydrology: &HydrologyState,
    geo: &GeoState,
) -> Biome {
    let temp     = climate.temperature;
    let precip   = climate.precipitation;
    let height   = geo.height;
    let flooding = derive_flooding(hydrology.river_flow, geo.height);

    // 1. 標高・気温優先
    if height > ALPINE_THRESHOLD {
        return Biome::Alpine;
    }
    if temp < TUNDRA_THRESHOLD {
        return Biome::Tundra;
    }

    // 2. 乾燥
    if precip < DESERT_THRESHOLD {
        return Biome::Desert;
    }

    // 3. 湛水
    if flooding > WETLAND_THRESHOLD && tree_cover < WETLAND_TREE_THRESHOLD {
        return Biome::Wetland;
    }

    // 4. 気温帯 × tree_cover × ground_cover
    if temp > TROPICAL_TEMP_THRESHOLD {
        return if tree_cover > FOREST_THRESHOLD {
            Biome::TropicalForest
        } else {
            Biome::Savanna  // 高温・低tree_cover（ground_coverの有無は問わない）
        };
    }

    if temp > BOREAL_TEMP_THRESHOLD {
        return if tree_cover > FOREST_THRESHOLD {
            Biome::TemperateForest
        } else {
            // tree_cover低・ground_cover高低どちらもGrasslandにまとめる
            // Tier2でSteppeにするか検討
            Biome::Grassland
        };
    }

    Biome::BorealForest
}

fn derive_flooding(river_flow: f32, height: f32) -> f32 {
    // 流量が多く、標高が低いほど湛水しやすい
    // 具体的な式はチューニング時に決める
    todo!()
}
```

### disturbance

詳細定義はSubsistence設計時に確定。

### tree_cover

```rust
fn update_tree_cover(
    current: f32,
    climate: &ClimateState,
    feedback: &TreeCoverFeedback,
    dt: f32,
) -> f32 {
    // 1. 気候から潜在的なtree_coverを計算
    let potential = tree_cover_potential(climate.temperature, climate.precipitation);

    // 2. FeedbackQueueから受け取る変化量
    let logging_loss    = feedback.logging * dt;     // 伐採
    let slash_burn_loss = feedback.slash_burn * dt;  // 焼畑

    // 3. ポテンシャルへ収束した次の値からlossを引く
    (converge_toward(current, potential, TREE_GROWTH_RATE, TREE_DECLINE_RATE, dt)
        - logging_loss
        - slash_burn_loss
    ).clamp(0.0, 1.0)
}

fn tree_cover_potential(temperature: f32, precipitation: f32) -> f32 {
    // 高温多雨→高い、乾燥・極寒→低い
    todo!()
}

struct TreeCoverFeedback {
    logging:    f32,  // Subsistenceが積む（伐採強度）
    slash_burn: f32,  // Subsistenceが積む（焼畑）
}
```

### ground_cover

```rust
fn update_ground_cover(
    current: f32,
    climate: &ClimateState,
    feedback: &GroundCoverFeedback,
    dt: f32,
) -> f32 {
    // 1. 気候から潜在的なground_coverを計算
    let potential = ground_cover_potential(climate.temperature, climate.precipitation);

    // 2. FeedbackQueueから受け取る変化量
    let grazing_loss    = feedback.grazing * dt;     // 放牧による草地消耗
    let slash_burn_loss = feedback.slash_burn * dt;  // 焼畑（tree_coverと共有）

    // 3. ポテンシャルへ収束した次の値からlossを引く
    (converge_toward(current, potential, GROUND_GROWTH_RATE, GROUND_DECLINE_RATE, dt)
        - grazing_loss
        - slash_burn_loss
    ).clamp(0.0, 1.0)
}

fn ground_cover_potential(temperature: f32, precipitation: f32) -> f32 {
    // 草本は樹木より乾燥・寒冷に強い→ポテンシャルが下がりにくい
    todo!()
}

struct GroundCoverFeedback {
    grazing:    f32,  // Subsistenceが積む（放牧強度）
    slash_burn: f32,  // Subsistenceが積む（焼畑）
}
```

### soil_fertility

```rust
fn update_soil_fertility(
    current: f32,
    tree_cover: f32,
    ground_cover: f32,
    climate: &ClimateState,
    feedback: &SoilFertilityFeedback,
    dt: f32,
) -> f32 {
    // 1. 自然回復（非常に遅い）
    let natural_recovery = natural_recovery_rate(tree_cover, ground_cover, climate)
        * dt
        * (1.0 - current);

    // 2. FeedbackQueueから受け取る変化量
    let farming_loss     = feedback.farming_consumption * dt;  // 農耕・連作
    let erosion_loss     = feedback.erosion_loss * dt;         // 侵食による表土流出
    let flood_gain       = feedback.flood_deposition * dt;     // 洪水・堆積
    let slash_burn_delta = feedback.slash_burn_delta * dt;     // 焼畑（正負あり）

    let next = current
        + natural_recovery
        + flood_gain
        + slash_burn_delta
        - farming_loss
        - erosion_loss;

    next.clamp(0.0, 1.0)
}

fn natural_recovery_rate(
    tree_cover: f32,
    ground_cover: f32,
    climate: &ClimateState,
) -> f32 {
    // 植生被覆と気候から直接計算。Biomeを経由しない。
    let cover         = tree_cover + ground_cover * GROUND_COVER_WEIGHT;
    let temp_factor   = f(climate.temperature);
    let precip_factor = g(climate.precipitation);

    cover * temp_factor * precip_factor * BASE_RECOVERY_RATE
}

struct SoilFertilityFeedback {
    farming_consumption: f32,  // Subsistenceが積む（農耕・連作強度）
    erosion_loss:        f32,  // Geologyが積む（侵食量から導出）
    flood_deposition:    f32,  // Hydrologyが積む（氾濫・堆積量から導出）
    slash_burn_delta:    f32,  // Subsistenceが積む（焼畑、正負あり）
}
```

## 責務分離

- `productivity` は `Subsistence` の責務とする
- `habitability` は `Settlement` の責務とする
- `riparian_vegetation` は `Geology` の更新関数内で局所計算する派生値とする
- `biome` は外部参照用ラベルであり、Ecology内部計算には使用しない

## Tier2拡張予定

```rust
// shrub_cover: 0..1（低木層。Domesticatesの牧畜判定で必要になれば追加）
// soil_moisture: 0..1（歴史展開期の灌漑・干ばつ表現で必要になれば追加）
```

## 関連

- `docs/reference/architecture/module_boundaries.md`
- `docs/reference/architecture/data_model.md`
- `docs/reference/modules/climate.md`（`tree_cover` / `ground_cover` 由来の
  `vegetation_density_proxy` をClimateが内部計算して利用）
