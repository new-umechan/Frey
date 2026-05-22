# Data Model

本書は reference 文書である。`World` を構成するデータ構造と配置方針の正本として扱う。

## 目的

この文書は、`World`を構成する各データ構造の定義と配置方針を記述する。

設計上の原則は次の通りである。

- 全セルが持つ現在値は `WorldState` 内の各State構造体にSoAで置く
- 他Moduleが読む公開列と内部状態列は命名で分離する（`_internal` サフィックス）
- 疎なEntity（国家・集落・地域など）は `EntityState` で直接管理する
- 国家間関係・プレート間関係は `World` 直下に保持する
- tick 計算に不要な派生情報は `WorldProjectionState` に分離する
- 実行時だけ必要な scratch 状態は `exec_scratch` として保持し、履歴や transport には持ち込まない
- 実行順・依存・feedback inbox・profiling group は module declaration を正本にする

## 目標構造

```rust
struct World {
    metadata:      WorldMetadata,          // mesh などの世界メタデータ
    state:         WorldState,             // 次 tick の計算に必要な SoA 正本
    projections:   WorldProjectionState,   // terrain などの派生 view
    entities:      EntityState,            // 疎な Entity の正本
    clock:         ClockState,
    control:       WorldControlState,      // simulation control / tunables
    exec_scratch:  ExecScratchState,       // exec 中だけ使う scratch
    relations:     WorldRelations,
}

struct WorldMetadata {
    mesh: WorldMesh,
}

struct WorldRelations {
    polity_relations: HashMap<(PolityId, PolityId), PolityRelation>,
    polity_groups:    Vec<PolityGroup>,
    plate_relations:  HashMap<(PlateId, PlateId), PlateRelation>,
}
```

checkpoint snapshot と seek 用の補助状態は `World` の正本には含めず、
管理層（現状は WASM 側の `ManagedWorld`）で保持する。

### Alpha 事前計算 Snapshot（dev 専用）

`alpha` の開発 bootstrap 短縮のため、era 境界 snapshot を補助 artifact として扱う。

- 対象 stage:
    - `environment` (`tick=800`)
    - `life` (`tick=1300`)
    - `civilization` (`tick=1395`)
    - `history` (`tick=1445`)
- 保存正本: `./.cache/frey/alpha-snapshots/`
- browser mirror: `web/public/.dev-precomputed/alpha/`
- artifact:
    - `manifest.json`（stage, tick, era, fingerprint, filename）
    - `*.bin`（`WorldCore` + dynamics state + metadata を含む envelope）
    - `*.json`（browser で直接開ける companion view。`*.bin` と同内容の pretty JSON）

復元は dev opt-in のみで有効化する。`seed != alpha` では常に通常の `Crust` 初期化を使う。
snapshot 不在・破損・fingerprint 不一致時は warning を出し、通常計算へフォールバックする。

## Managed 層（WASM transport）

`ManagedWorld` は `World` 正本の外側で、次を管理する。

- `hydrology_dynamics` / `geology_dynamics` などの実行補助状態
- 現在 world の進行状態
- transport 用の `TimelineViewCache`（view delta 返却用 shadow）

`hydrology_dynamics` が保持する `ErosionAutomatonState` は、`height` や `river_next` に加えて
その tick の `sea_level_offset` も保持する。
async erosion / fill-spill / river rebuild はこの値を参照し、
`height > sea_level_offset` を land 判定、
`height <= sea_level_offset + shallow_sea_floor` を深い海成帯の判定に使う。

時間軸専用の正本は `TimelineRuntime` に置く。

- `TimelineArchive`
  checkpoint と intervention の保存
- `TickUndoLog`
  巻き戻し用の tick 単位ログ
  現状は `geology.height`、climate の連続値列、glaciology の連続値列、
  `hydrology.river_flow` / `river_next` / sink 系 selected fields /
  `erosion_rate` / `deposition_rate`、`ecology.biome` / `tree_cover` /
  `ground_cover` / `disturbance` / `soil_fertility`、
  `domesticates.crop_available` / `crop_adoption` / `livestock_available` /
  `livestock_adoption` / `domesticates_internal`、
  `subsistence.subsistence_mix` / `food_energy_mean` / `food_energy_variance` /
  `buffer_capacity` / `mobility_capacity` / `land_use_intensity`、
  `population.population` / `birth_rate` / `death_rate`、
  `settlement.urbanization`、`polity.polity_id`、
  `conflict.conflict_intensity` / `occupier_id`、
  `entities` の create/update/delete を表す structured undo、
  `relations` の map before-value patch /
  `polity_groups` の upsert/remove + order_before structured undo、
  `clock` の scalar fields、`control` の scalar fields
  を selected sparse field として保持できる
- `TimelineCursor`
  現在 tick と timeline head tick を保持する
- `TimelineRetentionPolicy`
  checkpoint interval / checkpoint limit / undo log limit / max estimated bytes を保持する

`TimelineViewCache` の観測更新は、毎tickで一時配列を生成しないことを原則とする。
派生値（例: `plate_id` / `biome` / domesticates 系列）は shadow への直接比較更新で扱い、
不要な `Vec` 生成を避ける。

時間操作の公開整合点は `tick 完了境界` とする。
slice 実行中の partial tick は `ManagedWorldExecState` に閉じ込め、
timeline query / rewind / seek は完了済み tick に対してのみ成立する。
単一 timeline モデルなので、`seek` や `rewind` で future 側ログを破棄しない。

## ID型定義

すべてのIDはnewtypeパターンで定義する。異なるID型の混在はコンパイルエラーとなる。
セル数は現在約4万だが u32 を採用する。

```rust
struct CellId(u32);
struct PolityId(u32);
struct SettlementId(u32);
struct RegionId(u32);
struct PlateId(u32);
```

## WorldState

`WorldState` は「次 tick の計算に必要な正本」だけを持つ。
各 State は SoA 構造を持ち、セル index がそのまま `CellId` になる。
v1 の reservoir 分離では、すべての在庫量をただちにセル列へ落とし込まない。
`solid_earth_mass` のように状態正本ではなく全球 diagnostics として定義する量もある。
また、海陸判定に使う `surface_elevation` は正本の独立列ではなく、
`bedrock height + ice_thickness - sea_level_offset` から導出する。

### WorldState と module state

全セルのComponentは module ごとの State に分割し、各 State は SoA（Structure of Arrays）で保持する。
セルは常に全数存在し、各 `Vec` の index がそのまま `CellId` になる。

```rust
struct WorldState {
    geology:      GeologyState,
    climate:      ClimateState,
    glaciology:   GlaciologyState,
    hydrology:    HydrologyState,
    ecology:      EcologyState,
    domesticates: DomesticatesState,
    subsistence:  SubsistenceState,
    population:   PopulationState,
    settlement:   SettlementState,
    polity:       PolityState,
    conflict:     ConflictState,
}

struct GeologyState {
    height:               Vec<f32>,
    lake_depth:           Vec<f32>,
    plate_id:             Vec<PlateId>,
    volcanism:            Vec<f32>,
    vertex_buoyancy:      Vec<f32>,
    geology_internal:     Vec<GeologyInternal>,
    boundary_condition:   Vec<f32>,

    // smoothing / zero-mean diagnostics
    smoothing_limited_cells_ratio: f32,
    mean_smoothing_factor:        f32,
    zero_mean_adjusted_cells_ratio: f32,
    zero_mean_mean_abs_correction: f32,
    zero_mean_std_delta:          f32,
}

struct ClimateState {
    temperature:          Vec<f32>,
    precipitation:        Vec<f32>,
    evapotranspiration:   Vec<f32>,
    runoff:               Vec<f32>,
    aridity:              Vec<f32>,
    ocean_temperature:    Vec<f32>,
    precipitable_water:   Vec<f32>,
    cloud_water:          Vec<f32>,
    wind_u:               Vec<f32>,
    wind_v:               Vec<f32>,
    moisture_flux_u:      Vec<f32>,
    moisture_flux_v:      Vec<f32>,
}

struct GlaciologyState {
    ice_thickness:         Vec<f32>,
    ice_load:              Vec<f32>,           // 氷荷重。Geology が地盤応答計算に使用
    accumulation:          Vec<f32>,
    ablation:              Vec<f32>,
    isostatic_adjustment:  Vec<f32>,           // 地盤応答目標量。Geology が height に反映
    applied_isostatic_adjustment: Vec<f32>,
    glacial_erosion_rate:  Vec<f32>,
    glacial_melt_runoff:   Vec<f32>,           // 氷河融解流出量。Hydrology の流出入力へ加算
}

struct HydrologyState {
    river_downstream:     Vec<SmallVec<[(u32, f32); 4]>>,
    river_next:           Vec<i32>,
    river_flow:           Vec<f32>,
    river_transport_cost: Vec<f32>,      // 河川輸送コスト (0..1)。1.0 / (1.0 + river_flow.sqrt()) で計算。Trade/Route 計画で使用
    surface_water_access: Vec<f32>,      // 表流水アクセス (0..1)。Population・Settlement・Subsistence が読む
    erosion_rate:         Vec<f32>,
    deposition_rate:      Vec<f32>,
    is_lake:              Vec<bool>,      // 窪地を湖として扱うフラグ。湖セルは流量を吸収し鞍部から溢れる
    sink_id:              Vec<i32>,
    sink_route_next:      Vec<i32>,
    sink_member_offsets:  Vec<u32>,
    sink_member_cells:    Vec<u32>,
    sink_spill_cell:      Vec<i32>,
    sink_spill_to:        Vec<i32>,
    sink_spill_level:     Vec<f32>,
    sink_capacity_total:  Vec<f32>,
    sink_capacity_remaining: Vec<f32>,
    sink_storage_water:   Vec<f32>,
    sink_storage_sediment: Vec<f32>,
    sink_overflow_active: Vec<u8>,
}

struct EcologyState {
    biome:                Vec<Biome>,
    tree_cover:           Vec<f32>,   // 0..1
    ground_cover:         Vec<f32>,   // 0..1
    disturbance:          Vec<f32>,   // 0..1
    soil_fertility:       Vec<f32>,   // 0..1
    ecology_internal:     Vec<EcologyInternal>,
}

struct DomesticatesState {
    // crop_available / livestock_available は環境適性判定結果。adoption更新は Domesticates が書くが、デバッグ・可視化で確認可能な公開値として扱う。Subsistence は読まない。
    crop_available:       Vec<CropBitmap>,
    crop_adoption:        Vec<[f32; N_CROPS]>,       // 0.0〜1.0の普及度。Subsistenceが読む
    livestock_available:  Vec<LivestockBitmap>,
    livestock_adoption:   Vec<[f32; N_LIVESTOCK]>,   // 0.0〜1.0の普及度。Subsistenceが読む
    domesticates_internal: Vec<DomesticatesInternal>,
}

struct SubsistenceState {
    subsistence_mix:      Vec<SubsistenceMix>,
    food_energy_mean:     Vec<f32>,
    food_energy_variance: Vec<f32>,
    buffer_capacity:      Vec<f32>,
    mobility_capacity:    Vec<f32>,
    land_use_intensity:   Vec<f32>,
}

struct PopulationState {
    population:           Vec<f32>, // f32でも、数百万人のうち下位1桁しか変わらないため許容
    birth_rate:           Vec<f32>, // Subsistenceからの飢餓圧力を受ける
    death_rate:           Vec<f32>, // ConflictがFeedbackQueue経由で干渉する
}

struct SettlementState {
    urbanization:         Vec<f32>,
}

struct PolityState {
    polity_id:            Vec<Option<PolityId>>,
}

struct ConflictState {
    conflict_intensity:   Vec<f32>,            // 0..1、戦線からの距離減衰込みの戦闘強度。毎tick上書き
    occupier_id:          Vec<Option<PolityId>>,  // 実効支配国。主権(polity_id)とは独立して保持
}
```

Environment 期の入口では erosion_rate / deposition_rate を raw 変化量のまま公開せず、
hydrology spinup を掛けた applied rate を公開する。

`surface_elevation` は次の導出量として扱う。

```text
surface_elevation = geology.height + glaciology.ice_thickness - control.sea_level_offset
```

海陸判定は `surface_elevation > 0` を正本とし、`height > 0` の旧来判定は使わない。

`latitude` / `distance_from_ocean` / `coast_side` / `is_coastal` のような terrain 系の派生列は
`WorldState` には置かず、`WorldProjectionState` に分ける。
隣接トポロジは `WorldMesh` が正本であり、cell state 側に重複保持しない。

`river_downstream` は可変長複合構造であるため、
undo log では full hydrology clone ではなく `changed cell indices + route offsets + route payload`
を持つ compact patch を使って巻き戻す。

retention のメモリ予算は近似値で扱う。
正確な allocator 使用量ではなく、snapshot / undo patch が保持する主要配列長から見積もる。
structured undo では fixed-size の型だけでなく、`cells_cache` / `cells` / `members`
のような可変 payload も個別に加算して retention 判断へ反映する。
checkpoint prune は単純 oldest ではなく、初期 / 最新 / current 最寄り checkpoint を保護しつつ、
最も冗長な中間 checkpoint から落とす。
undo log prune も単純 oldest ではなく、`current_tick` の undo を優先保持し、
future 側と遠距離 tick から落とす current-centric 方針を使う。
このとき `undo_future_prune_grace_ticks` で future 優先 prune の強さを調整できる。
また、単一 timeline の seek 可用性を守るため、retention prune でも初期 checkpoint と最新 checkpoint は優先保持する。

### 内部状態Componentの型定義

```rust
struct GeologyInternal {  // geology_types.rs で定義
    crust_type:        CrustType,
    age:               f32,
    thickness:         f32,
    density:           f32,

    stress:            StressTensor,
    temperature:       f32,
    rigidity:          f32,

    arc_volcanism:     f32,
    ridge_volcanism:   f32,
    hotspot_volcanism: f32,
    backarc_volcanism: f32,
}

struct BoundaryEdgeInternal {
    convergence_memory: f32,
}

struct BoundaryDynamicsState {
    reclassify_interval_ticks: u32,
    steps_since_reclassify: u32,
    dominant_type: Vec<BoundaryType>,
    activity: Vec<f32>,
    edge_pairs: Vec<[u32; 2]>,
    edge_pairs_plate_hash: u64,
    edge_internal: Vec<BoundaryEdgeInternal>,
    rollback_fraction: Vec<f32>,
    backarc_tension: Vec<f32>,
    slab_convergence_component: Vec<f32>,
    slab_rollback_component: Vec<f32>,
}
```

`BoundaryEdgeInternal` は境界edgeごとの収束履歴のみを保持する。
境界edgeの対応関係（`edge_pairs`）とスラブ成分（`slab_*_component`）は
`BoundaryDynamicsState` で管理する。
`edge_pairs_plate_hash` は `plate_id` 変化有無を判定するためのキャッシュで、
未変化tickでは境界edge再構築を省略する。
`dominant_type` と `activity` は reclassify 間隔中に使う境界分類 cache として保持する。
境界分類の正本入力は `plate_id` / plate dynamics / edge geometry であり、
`dominant_type` は分類更新時に再計算可能な派生 cache として扱う。

`plate_id` と `crust_type` は離散属性として境界通過で切り替える。
`age`・`thickness`・`density` は連続属性として移流する。
`stress`・`temperature`・`rigidity` はその場で毎tick再計算する。

## EntityState

Polity・Settlement・Region など、数が少なく動的に生滅する疎な Entity を直接管理する。

`EntityState` は `slotmap` ベースの専用ストアとし、各ドメインIDと内部キーを分離する。

```rust
new_key_type! { struct PolityKey; }
new_key_type! { struct SettlementKey; }
new_key_type! { struct RegionKey; }

struct EntityState {
    polities: SlotMap<PolityKey, PolityRecord>,
    settlements: SlotMap<SettlementKey, SettlementRecord>,
    regions: SlotMap<RegionKey, RegionRecord>,
    polity_by_id: BTreeMap<PolityId, PolityKey>,
    settlement_by_id: BTreeMap<SettlementId, SettlementKey>,
    region_by_id: BTreeMap<RegionId, RegionKey>,
}
```

```rust
// Polity Entity
struct PolityComponent {
    polity_id:      PolityId,
    capital_cell:   CellId,
    legitimacy:     f32,   // 正統性。低いと辺境から離反・分裂が起きる
    centralization: f32,   // 集権度。高いと遠隔地まで支配コストを払える
    military_tech:  f32,   // 軍事技術水準。Conflictが戦闘力補正に使う
    // 正本は CellStore.polity_id。polity_id変化時に差分更新するキャッシュ
    cells_cache:    Vec<CellId>,
}

// Settlement Entity
struct SettlementComponent {
    settlement_id: SettlementId,
    cell:          CellId,
}

// Region Entity（流域・文化圏・前線帯など）
struct RegionComponent {
    region_id: RegionId,
    cells:     Vec<CellId>,
}
```

`SettlementComponent.cell` を集落位置の正本とする。
居住地分布は `SettlementState.urbanization` などの公開列から導出して扱う。

---

## polity_relations

国家間の二者間関係グラフ。
`EntityState` とは別に、`World` 直下に `HashMap` で保持する。
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

## ClockState

tick進行・時代・予算を管理する。「世界の状態」ではなく「世界を進めるための時間制御」を担う。

```rust
struct ClockState {
    tick:                u64,
    epoch:               EraKind,
    real_years_per_tick: f32,
    runtime_tick_ms:     u32,
    budgets:             SubsystemBudgets,
    transition:          TransitionState,
}
```

`Tick` の定義は `docs/concepts/phase_control.md` を参照。

---

## WorldProjectionState

projection は正本から再構成可能な派生 state をまとめる。
現在は terrain view がここに入る。

```rust
struct WorldProjectionState {
    terrain: TerrainState,
}

struct TerrainState {
    latitude:            Vec<f32>,
    distance_from_ocean: Vec<f32>,
    coast_side:          Vec<CoastSide>,
    is_coastal:          Vec<bool>,
}
```

terrain は `Glaciology` / `Hydrology` の更新や海面変化に応じて refresh されるが、
正本はあくまで `height` や hydrology/geology の基礎列であり、projection は必要に応じて再生成する。

## WorldControlState

tick 計算に必要だが、セル単位の SoA ではない control / parameter 群を持つ。

```rust
struct WorldControlState {
    geology_params:                GeologyParams,
    sea_level_offset:              f32,
    erosion_thickness_coupling:    f32,
    deposition_thickness_coupling: f32,
    ocean_water_inventory:         f32,
    ocean_water_inventory_baseline: f32,
    ice_inventory:                 f32,
    marine_sediment_mass:          f32,
    global_sediment_export:        f32,
    solid_earth_mass_proxy:        f32,
    solid_earth_mass_proxy_baseline: f32,
}
```

`sea_level_offset` は projection/derived state ではなく、次 tick の計算に効く control 値として
`WorldControlState` に置く。
v1 の `sea_level_offset` は `ocean_water_inventory` と `ice_inventory` を使う
`capacity closure` で決める海面変数として扱い、
`ocean basin capacity` は現地形から毎 tick 近似再計算する。
海面式に直接入る water inventory は `Ocean + Ice` のみとし、
湖・河川・土壌水・地下水は diagnostics に留める。
`Crust` 期は海面固定で進め、`Crust`→`Environment` 境界で
`height` と `sea_level_offset` から `ocean_water_inventory` を再基準化してから
`capacity closure` を有効化する。

## Diagnostics と reservoir proxy

mass-based reservoir では次の扱いを採る。

- `solid_earth_mass` は `WorldState` のセル正本には置かず、`height`・密度 proxy・セル面積から導く全球 diagnostic proxy とする
- `marine_sediment_mass` は export の受け皿として global / `sink` diagnostics に置き、非減少の一方向 sink として扱う
- fluvial sediment accounting の正本集計キーは `sink_id` とし、各 `sink` の inflow / temporary storage / export / marine transfer を記録する
- `drainage_basin_id` や `depression_hierarchy_node_id` は公開 API に含めない
- glacial sediment は transport 状態を持たず、glacial erosion source と export / marine accounting の診断量として扱う

これらの diagnostics は benchmark と長期 drift 監視のために公開してよいが、
次 tick の更新正本とは区別して記述する。

## FeedbackQueue

同一tick内で循環依存を作らないための遅延反映キュー。
`World` の正本には含めず、実行管理層が保持する。
各 module 実行直前に、その module inbox に向いた entry だけを適用する。

`ModuleId` は実行単位の `System` ID ではない。
責務境界、feedback 帰属、実行 DAG のノードを表す `Module` 識別子として扱う。

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
    Region(RegionId),
    Global,
}

// u8で8種の作物をビット管理
// bit0: Wheat, bit1: Rice, bit2: Maize, bit3: Millet
// bit4: Potato, bit5: Cassava, bit6: Sorghum, bit7: Yam
type CropBitmap = u8;

// u8で5種の家畜をビット管理
// bit0: Cattle, bit1: Horse, bit2: Sheep, bit3: Pig, bit4: Camel
type LivestockBitmap = u8;

// 生業構成。各フィールドの合計が1.0になるよう正規化して使う
struct SubsistenceMix {
    gathering:   f32,  // 採集
    hunting:     f32,  // 狩猟
    fishing:     f32,  // 漁撈
    cultivation: f32,  // 農耕
    herding:     f32,  // 牧畜
}

enum CellFieldId {
    CropAdoption(CropId),
    LivestockAdoption(LivestockId),
    DomesticatesRoutedCropFeedback(CropId),
    DomesticatesRoutedLivestockFeedback(LivestockId),
    DomesticatesIntensificationBonus,
}

enum FeedbackPayload {
    // セルのf32フィールドに加算する（競合時は単純加算）
    // 例: DeltaF32 { field: CellFieldId::CropAdoption(CropId(0)), cell, delta }
    DeltaF32     { field: CellFieldId, cell: CellId, delta: f32 },
    // セルのフィールドを直接上書きする
    SetValue     { field: CellFieldId, cell: CellId, value: FieldValue },
    // EntityState 側のエンティティ操作
    SpawnEntity  { bundle: EntityBundle },
    DestroyEntity{ entity: EntityRef },
    MutateEntity { entity: EntityRef, patch: ComponentPatch },
    // 型互換のため保持するが、固定 tick 遷移モードでは exec pipeline で無効化する
    TriggerEpochTransition { to: Epoch },
}
```

`docs/reference/modules/` で `DomesticatesSpread` / `DomesticatesPopulationPressure`
のようなドメイン語を使う場合でも、transport 層の正本は上記 `FeedbackPayload` である。
実装時は次の写像で統一する。

- `DomesticatesSpread`
  `DeltaF32 { field: DomesticatesRoutedCropFeedback(_)/DomesticatesRoutedLivestockFeedback(_), ... }`
  の組で表現する
- `DomesticatesPopulationPressure`
  `DeltaF32 { field: DomesticatesIntensificationBonus, ... }` で表現する

`FeedbackEntry.source` と `FeedbackEntry.target_module` は、どの `Module` 境界から出た影響か、
どの `Module` 境界へ渡す影響かを示す。
同一 `Module` 内でどの `System` を実行したかは exec pipeline の実行計画で管理し、
`FeedbackQueue` の型には直接持たせない。

複数エントリが同一フィールド・同一セルに `DeltaF32` を積んだ場合、単純加算で解決する。
適用タイミングと更新順序は `docs/concepts/phase_control.md` を参照。

---

## WorldMesh

メッシュ構造情報。正二十面体分割による球面メッシュを保持する。

```rust
struct WorldMesh {
    positions:    Vec<[f32; 3]>,   // 頂点位置（3 次元座標）
    nbr_offsets:  Vec<u32>,        // 隣接リストのオフセット（CSR 形式）
    nbrs:         Vec<u32>,        // 隣接頂点インデックス（CSR 形式）
}
```

`WorldMesh` が隣接トポロジの正本であり、terrain/cell state 側に neighbor 配列を重複保持しない。

## ExecScratchState

exec 中だけ必要な scratch を保持する。serialize/replay の正本には含めない。
現在は geology 実行補助の scratch slot を持つ。

```rust
struct ExecScratchState {
    geology_dynamics: Option<GeologyDynamicsState>,
}
```

`TransitionState` は `ClockState` 側に属し、hydrology runtime state は `World` ではなく
exec 管理層から明示引数で渡す。

## Module Declarations

exec pipeline の正本は hand-written な if/match 列ではなく module declaration とする。
各 module は reads / writes / feedback / profiling / display group / execution kind を宣言する。

```rust
struct ModuleDeclaration {
    phase:          ExecWorldPhase,
    module_id:      ModuleId,
    reads:          &'static [WorldResource],
    writes:         &'static [WorldResource],
    feedback:       &'static [ModuleId],
    feedback_mode:  FeedbackMode,
    profile_category: ProfileCategory,
    display_group:  DisplayGroup,
    execution_kind: ExecutionKind,
    completes_tick: bool,
    step:           fn(&mut World, &mut ModuleExecContext<'_>),
}
```

依存辺は declaration から自動生成する。
基本ルールは次の通り。

- `writes -> reads/writes` の資源競合から順序を張る
- `feedback -> target module` から inbox 依存を張る
- topo sort 後も declaration 定義順は安定化する

この declaration から、次の情報を docs / graph / web UI にそのまま出力できる。

- module 一覧
- 依存 edge 一覧
- inbox 種別
- profiling group
- display group
- tick boundary
- execution kind
