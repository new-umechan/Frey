# World State / Graph State / Exec State

## 目的

この文書は、何を `World State` に置き、何を `Graph State` に置き、何を `Exec State` に置くかを定義する。

設計上の原則は次の通りである。

- 各モジュールが共有して読む**セル単位の現在値**は `World State` に置く
- セルに還元できない**グラフ構造の現在値**は `Graph State` に置く
- tick進行や履歴管理のための**進行管理状態**は `Exec State` に置く

## 目標構造

```python
World = {
    state: WorldState,
    graph: GraphState,
    exec:  ExecState,
}
```

---

## WorldState

各セルが持つ属性の現在値である。
モジュールはこれを読んで書く。

```python
WorldState = {

    geo: {
        # 固定地理量（tickごとに変化しないが全モジュールが読む）
        latitude_deg,
        distance_from_ocean_km,
        coast_side,
        is_coastal,
    },

    geology: {
        # 書き手: Geology
        height,
        plate_id,
        erosion_rate,
        deposition_rate,
    },

    climate: {
        # 書き手: Climate
        precipitation,
        temperature,
        evapotranspiration,
        runoff,
        aridity,
        ocean_temperature,
    },

    hydrology: {
        # 書き手: Hydrology
        river_path,
        river_flow,
        river_transport_cost,
    },

    ecology: {
        # 公開I/O
        biome,               # 派生, enum (詳細: docs/modules/ecology/ecology.md)

        tree_cover,          # 0..1
        ground_cover,        # 0..1  草本層（tree_coverと独立、重複あり）
        disturbance,         # 0..1（減衰あり）
        soil_fertility,        # 0..1（遅い）

        # tier2で追加予定。
        # shrub_cover         # 0..1
        # soil_moisture     # 1年程度の短いスパンでは必要となるが、それ以上ではほとんど無視できるため
    }

    domesticates: {
        # 書き手: Domesticates
        crop_available,         # 栽培可能種ビットマップ
        crop_adopted,           # 栽培実績ビットマップ
        livestock_available,    # 利用可能種ビットマップ
        livestock_adopted,      # 利用実績ビットマップ
    },

    subsistence: {
        # 書き手: Subsistence
        subsistence_mix,        # 生業構成（採集・狩猟・漁撈・農耕・牧畜・混合の比率）
        productivity,           # 生産性
        food_production,        # 食料生産量
        habitability,           # biome + productivity + river_flow + height → 立地適性
        land_use,               # 土地利用（Ecologyへのフィードバック元）
    },

    population: {
        # 書き手: Population
        population,
        population_density,
        migration_pressure,     # 人口移動圧（Settlementが読む）
    },

    settlement: {
        # 書き手: Settlement
        settlement_size,        # 集落規模
        urbanization,           # 都市化度
        centrality,             # 中心地階層
    },

    polity: {
        # 書き手: Polity
        polity_id,
        territory_status,       # settled / occupied / neutral
        language_group,         # 言語・文化圏ID
        polity_stability,       # 国家安定度
    },

    conflict: {
        # 書き手: Conflict
        war_state,              # このセルが戦闘地帯かどうか
        occupier_id,            # 占領中の国家ID（中立なら null）
    },

}
```

---

## GraphState

セルに還元できないグラフ構造の現在値である。
国家間関係のように「セルAとセルB」ではなく「国家Aと国家B」の関係として自然に表現されるものを置く。

```python
GraphState = {

    polity_relations,   # 重み付きグラフ: (polity_id, polity_id) → relation_weight
                        # 同盟(+1.0) ～ 戦争中(-1.0)

    # Tier 2追加時の拡張予定
    # trade_network     # 交易ネットワーク: (polity_id, polity_id) → trade_volume
    # diffusion_graph   # 技術・作物伝播グラフ
}
```

---

## ExecState

世界を進めるための進行管理状態である。
各モジュールの対象世界そのものではない。

```python
ExecState = {
    tick,
    epoch,
    budgets,            # SubsystemBudgets
    feedback_queue,     # FeedbackQueue
    history,
    snapshots,
}
```

---

## 更新器との関係

更新器はステートレスに保つ。
`World State`・`Graph State`・`Exec State` を引数として受け取り、次の状態を書き戻すだけにする。

```python
def update_geology(world_state, graph_state, exec_state): ...
def update_climate(world_state, graph_state, exec_state): ...
def update_hydrology(world_state, graph_state, exec_state): ...
def update_ecology(world_state, graph_state, exec_state): ...
def update_domesticates(world_state, graph_state, exec_state): ...
def update_subsistence(world_state, graph_state, exec_state): ...
def update_population(world_state, graph_state, exec_state): ...
def update_settlement(world_state, graph_state, exec_state): ...
def update_polity(world_state, graph_state, exec_state): ...
def update_conflict(world_state, graph_state, exec_state): ...
```

`FeedbackQueue` は `Exec State` に置く。
tick N で各モジュールが書き込み、tick N+1 の開始時に `Exec` が `World State` および `Graph State` へ適用する。

---

## Tier 2 追加時の拡張予定

Tier 2モジュールが有効化された際に追加される名前空間。

```python
# Disease
disease: {
    infection_rate,
    mortality_modifier,
}

# Resources
resources: {
    energy_deposit,       # エネルギー資源埋蔵量
    mineral_deposit,      # 鉱産資源埋蔵量
    extraction_rate,      # 採掘量
}

# Trade
trade: {
    trade_flow,           # 交易流量
    market_access,        # 市場アクセス度
}

# Technology
technology: {
    ag_tools,           # 農具・灌漑
    metallurgy,         # 金属器
    navigation,         # 航海術
    military_tech,      # 軍事技術
    recording,          # 記録技術
    transport,          # 輸送技術
}

# Infrastructure
infrastructure: {
    road_cost_modifier,   # 地上移動コスト修正
    irrigation,           # 灌漑
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
