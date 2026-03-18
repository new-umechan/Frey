# World State / Graph State / Exec State

## 目的

この文書は、何を `World State` に置き、何を `Graph State` に置き、何を `Exec State` に置くかを定義する。

設計上の原則は次の通りである。

- 各モジュールが共有して読む**セル単位の現在値**は `World State` に置く
- セルに還元できない**グラフ構造の現在値**は `Graph State` に置く
- tick進行や履歴管理のための**進行管理状態**は `Exec State` に置く

## 目標構造

```rust
struct World {
    state: WorldState,
    graph: GraphState,
    exec: ExecState,
}
```

---

## WorldState

各セルが持つ属性の現在値である。
モジュールはこれを読んで書く。

```rust
struct WorldState {
    geo: GeoState,
    geology: GeologyState,
    climate: ClimateState,
    hydrology: HydrologyState,
    ecology: EcologyState,
    domesticates: DomesticatesState,
    subsistence: SubsistenceState,
    population: PopulationState,
    settlement: SettlementState,
    polity: PolityState,
    conflict: ConflictState,
}

// 固定地理量（tickごとに変化しないが全モジュールが読む）
struct GeoState {
    latitude_deg: f32,
    distance_from_ocean_km: f32,
    coast_side: CoastSide,
    is_coastal: bool,
}

struct GeologyState {
    height: f32,
    plate_id: PlateId,
    erosion_rate: f32,
    deposition_rate: f32,
}

struct ClimateState {
    precipitation: f32,
    temperature: f32,
    evapotranspiration: f32,
    runoff: f32,
    aridity: f32,
    ocean_temperature: f32,
}

struct HydrologyState {
    river_path: RiverPath,
    river_flow: f32,
    river_transport_cost: f32,
}

struct EcologyState {
    biome: Biome, // 派生, enum (詳細: docs/modules/ecology/ecology.md)
    tree_cover: f32, // 0..1
    ground_cover: f32, // 0..1  草本層（tree_coverと独立、重複あり）
    disturbance: f32, // 0..1（減衰あり）
    soil_fertility: f32, // 0..1（遅い）
}

struct DomesticatesState {
    crop_available: CropBitmap, // 栽培可能種ビットマップ
    crop_adopted: CropBitmap, // 栽培実績ビットマップ
    livestock_available: LivestockBitmap, // 利用可能種ビットマップ
    livestock_adopted: LivestockBitmap, // 利用実績ビットマップ
}

struct SubsistenceState {
    subsistence_mix: SubsistenceMix, // 生業構成（採集・狩猟・漁撈・農耕・牧畜・混合の比率）
    productivity: f32, // 生産性
    food_production: f32, // 食料生産量
    habitability: f32, // biome + productivity + river_flow + height → 立地適性
    land_use: LandUse, // 土地利用（Ecologyへのフィードバック元）
}

struct PopulationState {
    population: f32, // 人口
    population_density: f32, // 人口密度
    migration_pressure: f32, // 人口移動圧（Settlementが読む）
}

struct SettlementState {
    settlement_size: f32, // 集落規模
    urbanization: f32, // 都市化度
    centrality: f32, // 中心地階層
}

struct PolityState {
    polity_id: Option<PolityId>,
    territory_status: TerritoryStatus, // settled / occupied / neutral
    language_group: Option<LanguageGroupId>, // 言語・文化圏ID
    polity_stability: f32, // 国家安定度
}

struct ConflictState {
    war_state: bool, // このセルが戦闘地帯かどうか
    occupier_id: Option<PolityId>, // 占領中の国家ID（中立なら null）
}
```

---

## GraphState

セルに還元できないグラフ構造の現在値である。
国家間関係のように「セルAとセルB」ではなく「国家Aと国家B」の関係として自然に表現されるものを置く。

```rust
struct GraphState {
    polity_relations: HashMap<(PolityId, PolityId), f32>, // 重み付きグラフ: (polity_id, polity_id) → relation_weight
                                                          // 同盟(+1.0) ～ 戦争中(-1.0)

    // Tier 2 追加時の拡張予定
    // trade_network: HashMap<(PolityId, PolityId), f32>,
    // diffusion_graph: DiffusionGraph,
}
```

---

## ExecState

世界を進めるための進行管理状態である。
各モジュールの対象世界そのものではない。

```rust
struct ExecState {
    tick: Tick,
    epoch: Epoch,
    budgets: SubsystemBudgets, // SubsystemBudgets
    feedback_queue: FeedbackQueue, // FeedbackQueue
    history: History,
    snapshots: SnapshotStore,
}
```

---

## 更新器との関係

更新器はステートレスに保つ。
`World State`・`Graph State`・`Exec State` を引数として受け取り、次の状態を書き戻すだけにする。

```rust
fn update_geology(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_climate(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_hydrology(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_ecology(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_domesticates(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_subsistence(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_population(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_settlement(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_polity(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }

fn update_conflict(
    world_state: &mut WorldState,
    graph_state: &mut GraphState,
    exec_state: &ExecState,
) { }
```

`FeedbackQueue` は `Exec State` に置く。
tick N で各モジュールが書き込み、tick N+1 の開始時に `Exec` が `World State` および `Graph State` へ適用する。

---

## Tier 2 追加時の拡張予定

Tier 2モジュールが有効化された際に追加される名前空間。

```rust
struct DiseaseState {
    infection_rate: f32,
    mortality_modifier: f32,
}

struct ResourcesState {
    energy_deposit: f32, // エネルギー資源埋蔵量
    mineral_deposit: f32, // 鉱産資源埋蔵量
    extraction_rate: f32, // 採掘量
}

struct TradeState {
    trade_flow: f32, // 交易流量
    market_access: f32, // 市場アクセス度
}

struct TechnologyState {
    ag_tools: bool, // 農具・灌漑
    metallurgy: bool, // 金属器
    navigation: bool, // 航海術
    military_tech: bool, // 軍事技術
    recording: bool, // 記録技術
    transport: bool, // 輸送技術
}

struct InfrastructureState {
    road_cost_modifier: f32, // 地上移動コスト修正
    irrigation: f32, // 灌漑
}
```

---

## 現行実装との差分

主な変更点は次の通り。

- `Civilization` 名前空間を解体し、`subsistence` / `population` / `settlement` / `polity` / `conflict` に分割
- `hydrology` を `geology` から切り出して独立した名前空間とした
- `domesticates` を新設
- `polity` に `language_group`・`polity_stability` を追加
- Rust側に残っていた `terrain_dynamics`・`river_erosion_state` などの実装都合の保持は、目標アーキテクチャでは `geology`・`hydrology` に吸収する
