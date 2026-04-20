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
    mesh:             WorldMesh,
    state:            WorldState,            // 次 tick の計算に必要な SoA 正本
    projections:      WorldProjectionState,  // terrain などの派生 view
    entities:         EntityState,           // 疎な Entity の正本
    clock:            ClockState,
    control:          WorldControlState,     // simulation control / tunables
    exec_scratch:     ExecScratchState,      // exec 中だけ使う scratch
    polity_relations: HashMap<(PolityId, PolityId), PolityRelation>,
    polity_groups:    Vec<PolityGroup>,
    plate_relations:  HashMap<(PlateId, PlateId), PlateRelation>,
}
```

履歴用の snapshot と replay 状態は `World` の正本には含めず、
管理層（現状は WASM 側の `ManagedWorld`）で保持する。

## Managed 層（WASM transport）

`ManagedWorld` は `World` 正本の外側で、次を管理する。

- `hydrology_dynamics` / `geology_dynamics` などの実行補助状態
- 履歴スナップショットと replay 制御
- transport 用の `WorldTransportCache`（delta 返却用 shadow）

`WorldTransportCache` の観測更新は、毎tickで一時配列を生成しないことを原則とする。
派生値（例: `plate_id` / `biome` / domesticates 系列）は shadow への直接比較更新で扱い、
不要な `Vec` 生成を避ける。

## ID型定義

すべてのIDはnewtypeパターンで定義する。異なるID型の混在はコンパイルエラーとなる。
セル数は現在約4万だがu32を採用する（将来的な解像度向上に備える）。

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

### GeologyState

全セルのComponentをSoA（Structure of Arrays）で保持する。
セルは常に全数存在し、全Componentを保持する。
インデックスがそのままCellIdになる。

```rust
struct GeologyState {
    // --- Geology ---
    height:               Vec<f32>,
    plate_id:             Vec<PlateId>,
    volcanism:            Vec<f32>,
    vertex_buoyancy:      Vec<f32>,
    lake_depth:           Vec<f32>,       // 湖の深さ。窪地を湖として扱う

    geology_internal:     Vec<GeologyInternal>,

    // --- Geology (debug/intermediate) ---
    // 以下のフィールドはデバッグ用途または内部中間状態。公開 API (GeologyOutput) からのみ参照可能。
    plate_is_ocean:       Vec<u8>,        // プレートが海洋か大陸か (0: 大陸，1: 海洋)
    plate_base_height:    Vec<f32>,       // プレート基準高さ (デバッグ用)
    plate_base_weight:    Vec<f32>,       // プレート基準重み (デバッグ用)
    vertex_age_norm:      Vec<f32>,       // 頂点年齢正規化 (デバッグ用)
    vertex_weight:        Vec<f32>,       // 頂点重み (デバッグ用)
    debug_trench_strength: Vec<f32>,      // 海溝強度 (デバッグ用)
    debug_arc_strength:    Vec<f32>,      // アーク強度 (デバッグ用)
    debug_backarc_strength: Vec<f32>,     // バックアーク強度 (デバッグ用)
    debug_ocean_ocean_arc_strength: Vec<f32>, // 海洋 - 海洋アーク強度 (デバッグ用)

    // --- Climate ---
    temperature:          Vec<f32>,
    precipitation:        Vec<f32>,
    evapotranspiration:   Vec<f32>,
    runoff:               Vec<f32>,
    aridity:              Vec<f32>,
    ocean_temperature:    Vec<f32>,
    wind_u:               Vec<f32>,
    wind_v:               Vec<f32>,
    moisture_flux_u:      Vec<f32>,
    moisture_flux_v:      Vec<f32>,

    // --- Glaciology ---
    ice_thickness:         Vec<f32>,
    ice_load:              Vec<f32>,           // 氷荷重。Geology が地盤応答計算に使用
    accumulation:          Vec<f32>,
    ablation:              Vec<f32>,
    isostatic_adjustment:  Vec<f32>,           // 地盤応答目標量。Geology が height に反映
    glacial_erosion_rate:  Vec<f32>,
    glacial_melt_runoff:   Vec<f32>,           // 氷河融解流出量。Hydrology の流出入力へ加算

    // --- Hydrology ---
    river_downstream:     Vec<SmallVec<[(CellId, f32); 3]>>,
    river_flow:           Vec<f32>,
    river_transport_cost: Vec<f32>,      // 河川輸送コスト (0..1)。1.0 / (1.0 + river_flow.sqrt()) で計算。Trade/Route 計画で使用
    erosion_rate:         Vec<f32>,
    deposition_rate:      Vec<f32>,
    is_lake:              Vec<bool>,      // 窪地を湖として扱うフラグ。湖セルは流量を吸収し鞍部から溢れる

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
    // crop_available / livestock_available は環境適性判定結果。adoption更新は Domesticates が書くが、デバッグ・可視化で確認可能な公開値として扱う。Subsistence は読まない。
    crop_available:       Vec<CropBitmap>,
    crop_adoption:        Vec<[f32; N_CROPS]>,       // 0.0〜1.0の普及度。Subsistenceが読む
    livestock_available:  Vec<LivestockBitmap>,
    livestock_adoption:   Vec<[f32; N_LIVESTOCK]>,   // 0.0〜1.0の普及度。Subsistenceが読む

    // --- Subsistence ---
    subsistence_mix:      Vec<SubsistenceMix>,
    food_production:      Vec<f32>,
    freshwater_access:    Vec<f32>,  // river_flow・is_lakeから導出。Population・Settlementが読む

    // --- Population ---
    population:           Vec<f32>, // f32でも、数百万人のうち下位1桁しか変わらないため許容
    birth_rate:           Vec<f32>, // Subsistenceからの飢餓圧力を受ける
    death_rate:           Vec<f32>, // ConflictがFeedbackQueue経由で干渉する

    // --- Settlement ---
    urbanization:         Vec<f32>,

    // --- Polity ---
    polity_id:            Vec<Option<PolityId>>,

    // --- Conflict ---
    conflict_intensity:   Vec<f32>,            // 0..1、戦線からの距離減衰込みの戦闘強度。毎tick上書き
    occupier_id:          Vec<Option<PolityId>>,  // 実効支配国。主権(polity_id)とは独立して保持
}
```

`latitude` / `distance_from_ocean` / `coast_side` / `is_coastal` のような terrain 系の派生列は
`WorldState` には置かず、`WorldProjectionState` に分ける。
隣接トポロジは `WorldMesh` が正本であり、cell state 側に重複保持しない。

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
境界タイプ自体はedge単位では永続保持せず、必要時に再計算する。

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
居住地分布は `GeologyState` の `urbanization` などの公開列から導出して扱う。

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
    tick:    Tick,
    epoch:   Epoch,
    budgets: SubsystemBudgets,
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
    target_sea_ratio:              f32,
    sea_level_offset:              f32,
    erosion_thickness_coupling:    f32,
    deposition_thickness_coupling: f32,
}
```

`target_sea_ratio` や `sea_level_offset` は projection/derived state ではなく、
次 tick の計算に効く control 値として `WorldControlState` に置く。

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
    farming:     f32,  // 農耕
    pastoralism: f32,  // 牧畜
}

enum CellFieldId {
    // 基本フィールド
    // ...
    // Domesticates
    CropAdoption(CropId),
    LivestockAdoption(LivestockId),
    // Domesticates feedback staging（Population -> Domesticates）
    DomesticatesNeighborPopulationDensity,
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

`docs/reference/modules/` で `FeedbackKind::DomesticatesSpread` / `FeedbackKind::DomesticatesPopulationPressure`
のようなドメイン語を使う場合でも、transport 層の正本は上記 `FeedbackPayload` である。
実装時は次の写像で統一する。

- `DomesticatesSpread`
  `DeltaF32 { field: CropAdoption(_)/LivestockAdoption(_), ... }` の組で表現する
- `DomesticatesPopulationPressure`
  `SetValue { field: DomesticatesNeighborPopulationDensity, ... }` と
  `SetValue { field: DomesticatesIntensificationBonus, ... }` の組で表現する

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

exec 中だけ必要な scratch を保持する。serialize/fork/replay の正本には含めない。
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

## Tier 2 追加時の拡張予定

Tier2モジュールが有効化された際にCellStoreへ追加されるComponent列。

```rust
// Language（Tier1では削除。住民の言語・民族帰属を表現する）
language_group:       Vec<Option<LanguageGroupId>>,
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
