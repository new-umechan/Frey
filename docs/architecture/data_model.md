# Data Model

## 目的

この文書は、`Simulation` を構成する各データ構造の定義と配置方針を記述する。

設計上の原則は次の通りである。

- 全セルが持つ現在値は `CellStore` にSoAで置く
- 他Systemが読むComponentと内部状態Componentは命名で分離する（`_internal` サフィックス）
- 疎なEntity（国家・集落・地域など）は `hecs::World` で管理する
- 国家間関係は `Simulation` 直下に `HashMap` で保持する
- tick進行や履歴管理のための進行管理状態は `Clock`・`FeedbackQueue`・`Archive` に分割して置く

## 目標構造

```rust
struct Simulation {
    cells:            CellStore,
    entities:         hecs::World,
    polity_relations: HashMap<(PolityId, PolityId), PolityRelation>,
    polity_groups:    Vec<PolityGroup>,
    clock:            Clock,
    feedback:         FeedbackQueue,
    archive:          Archive,
}
```

---

## CellStore

全セルのComponentをSoA（Structure of Arrays）で保持する。
セルは常に全数存在し、全Componentを保持する。
インデックスがそのままCellIdになる。

グリッドは正二十面体分割由来のため、6角形セルが大多数だが5角形セルが12個存在する。
このため隣接数は5または6の可変長になり、`neighbors` は `SmallVec<[CellId; 6]>` で保持する。
隣接セル情報は初期化時に一度計算し、その後は固定とする。

```rust
struct CellStore {
    // --- Geo（固定地理量）---
    latitude:             Vec<f32>,
    distance_from_ocean:  Vec<f32>,
    coast_side:           Vec<CoastSide>,
    is_coastal:           Vec<bool>,
    neighbors:            Vec<SmallVec<[CellId; 6]>>,  // 5角形は5要素、6角形は6要素

    // --- Geology ---
    height:               Vec<f32>,
    plate_id:             Vec<PlateId>,

    // --- Climate ---
    temperature:          Vec<f32>,
    precipitation:        Vec<f32>,
    evapotranspiration:   Vec<f32>,
    runoff:               Vec<f32>,
    aridity:              Vec<f32>,
    ocean_temperature:    Vec<f32>,

    // --- Hydrology ---
    river_upstream:       Vec<Option<CellId>>,
    river_downstream:     Vec<Option<CellId>>,
    river_flow:           Vec<f32>,
    river_transport_cost: Vec<f32>,
    erosion_rate:         Vec<f32>,
    deposition_rate:      Vec<f32>,

    // --- Ecology（公開）---
    biome:                Vec<Biome>,
    tree_cover:           Vec<f32>,   // 0..1
    ground_cover:         Vec<f32>,   // 0..1
    disturbance:          Vec<f32>,   // 0..1
    soil_fertility:       Vec<f32>,   // 0..1

    // --- Ecology（内部状態）---
    // EcologySystem以外は読まない
    ecology_internal:     Vec<EcologyInternal>,

    // --- Domesticates（公開）---
    crop_available:       Vec<CropBitmap>,
    crop_adopted:         Vec<CropBitmap>,
    livestock_available:  Vec<LivestockBitmap>,
    livestock_adopted:    Vec<LivestockBitmap>,

    // --- Domesticates（内部状態）---
    // DomesticatesSystem以外は読まない
    domesticates_internal: Vec<DomesticatesInternal>,

    // --- Subsistence ---
    subsistence_mix:      Vec<SubsistenceMix>,
    productivity:         Vec<f32>,
    food_production:      Vec<f32>,
    habitability:         Vec<f32>,
    land_use:             Vec<LandUse>,

    // --- Population ---
    population:           Vec<f32>,
    population_density:   Vec<f32>,
    migration_pressure:   Vec<f32>,

    // --- Settlement ---
    settlement_size:      Vec<f32>,
    urbanization:         Vec<f32>,
    centrality:           Vec<f32>,

    // --- Polity ---
    polity_id:            Vec<Option<PolityId>>,
    territory_status:     Vec<TerritoryStatus>,
    language_group:       Vec<Option<LanguageGroupId>>,
    polity_stability:     Vec<f32>,

    // --- Conflict ---
    war_state:            Vec<bool>,
    frontline_pressure:   Vec<f32>,   // 0..1, 戦線強度
    occupier_id:          Vec<Option<PolityId>>,
}
```

### 内部状態Componentの型定義

他モジュールが読む公開Componentと、所有モジュール以外が読まない内部状態Componentは、
同じ `CellStore` 内に置いたまま命名で分離する（`_internal` サフィックス）。
読み取り規約の境界は `docs/architecture/module_boundaries.md` で定義する。

## hecs::World

Polity・Settlement・Regionなど、数が少なく動的に生滅する疎なEntityを管理する。
各EntityはComponentの組み合わせとして表現する。

```rust
// Polity Entity
struct PolityComponent {
    polity_id:    PolityId,
    capital_cell: CellId,
    stability:    f32,
}

struct LanguageGroupComponent {
    group_id: LanguageGroupId,
}

// Settlement Entity
struct SettlementComponent {
    settlement_id: SettlementId,
    cell:          CellId,
    size:          f32,
    urbanization:  f32,
}

// Region Entity（流域・文化圏・前線帯など）
struct RegionComponent {
    region_id: RegionId,
    cells:     Vec<CellId>,
}
```

`SettlementComponent.cell` を集落位置の正本とする。
居住地分布は `CellStore` の `settlement_size`・`urbanization`・`centrality` などの公開列から導出して扱う。

---

## polity_relations

国家間の二者間関係グラフ。
hecsのArchetype最適化の恩恵を受けにくいため、`Simulation` 直下に `HashMap` で保持する。
関係は有向であり、`(from, to)` と `(to, from)` は独立したエントリを持つ。

```rust
// (from, to) → 二者間関係
HashMap<(PolityId, PolityId), PolityRelation>

struct PolityRelation {
    alliance: f32,             // -1.0（敵対）〜 +1.0（同盟）
    trade:    f32,             // 0.0（無交流）〜 1.0（強い交易依存）
    at_war:   bool,
    suzerain: Option<PolityId>, // この国の宗主国（親子関係）
}
```

`suzerain` は `from` 側が `to` 側を宗主国として認めていることを表す。
宗主関係の集約（衛星国家の一覧取得など）はクエリ時にHashMapを走査して導出する。

---

## polity_groups

経済圏・軍事同盟・文化宗教圏など、複数国家をまとめるグループ。
二者間関係では表現できない多者間の連帯を保持する。

```rust
Vec<PolityGroup>

struct PolityGroup {
    id:      PolityGroupId,
    kind:    GroupKind,
    members: Vec<PolityId>,
    leader:  Option<PolityId>,  // 盟主・中心国（任意）
}

enum GroupKind {
    EconomicZone,      // 経済圏
    MilitaryAlliance,  // 軍事同盟
    CulturalSphere,    // 文化・宗教圏（ゆるやかな連帯）
}
```

グループへの加入・脱退・解散は `Polity` モジュールが `FeedbackQueue` 経由で次tickに適用する。

---

## Clock

tick進行・時代・予算を管理する。「世界の状態」ではなく「世界を進めるための時間制御」を担う。

```rust
struct Clock {
    tick:    Tick,
    epoch:   Epoch,
    budgets: SubsystemBudgets,
}
```

`Tick` の定義は `docs/architecture/phase_control.md` を参照。

---

## FeedbackQueue

同一tick内で循環依存を作らないための遅延反映キュー。
tick開始時に `ExecSystem` が一括で `CellStore` と `hecs::World` に適用する。

`ModuleId` は実行単位の `System` ID ではない。
予算配分、責務境界、フィードバック帰属を表す `Module` 識別子として扱う。

```rust
struct FeedbackQueue {
    entries: Vec<FeedbackEntry>,
}

struct FeedbackEntry {
    source:        ModuleId,
    target_module: ModuleId,
    target_ref:    TargetRef,
    enqueued_tick: Tick,
    payload:       FeedbackPayload,
}

enum TargetRef {
    Cell(CellId),
    Polity(PolityId),
    Settlement(SettlementId),
    Edge(GraphEdgeId),
    Region(RegionId),
    Global,
}

enum FeedbackPayload {
    // セルのf32フィールドに加算する（競合時は単純加算）
    DeltaF32     { field: CellFieldId, cell: CellId, delta: f32 },
    // セルのフィールドを直接上書きする
    SetValue     { field: CellFieldId, cell: CellId, value: FieldValue },
    // hecs::World 側のエンティティ操作
    SpawnEntity  { bundle: EntityBundle },
    DestroyEntity{ id: EntityId },
    MutateEntity { id: EntityId, patch: ComponentPatch },
    // 将来の拡張用
    TriggerEpochTransition { to: Epoch },
}
```

`FeedbackEntry.source` と `FeedbackEntry.target_module` は、どの `Module` 境界から出た影響か、
どの `Module` 境界へ渡す影響かを示す。
同一 `Module` 内でどの `System` を実行したかは `ExecSystem` の実行計画で管理し、
`FeedbackQueue` の型には直接持たせない。

複数エントリが同一フィールド・同一セルに `DeltaF32` を積んだ場合、単純加算で解決する。
適用タイミングと更新順序は `docs/architecture/phase_control.md` を参照。

---

## Archive

履歴と過去スナップショットの記録。世界の現在状態ではなく観測・再生用途。

```rust
struct Archive {
    history:   History,
    snapshots: SnapshotStore,
}
```

---

## Tier 2 追加時の拡張予定

Tier2モジュールが有効化された際にCellStoreへ追加されるComponent列。

```rust
// Disease
infection_rate:       Vec<f32>,
mortality_modifier:   Vec<f32>,

// Resources
energy_deposit:       Vec<f32>,
mineral_deposit:      Vec<f32>,
extraction_rate:      Vec<f32>,

// Trade
trade_flow:           Vec<f32>,
market_access:        Vec<f32>,

// Technology
ag_tools:             Vec<bool>,
metallurgy:           Vec<bool>,
navigation:           Vec<bool>,
military_tech:        Vec<bool>,
recording:            Vec<bool>,
transport:            Vec<bool>,

// Infrastructure
road_cost_modifier:   Vec<f32>,
irrigation:           Vec<f32>,
```
