# WASM API仕様

本書は、JSから利用する現行WASM公開APIを定義する。

## 1. Core API

- `generate_mesh(level: number) -> { positions, indices }`
- `generate_terrain(seed: string, params?: TerrainParams) -> TerrainOutput`
- `build_render_positions(input) -> number[]`

## 2. WorldSimController API

`WorldSimController`は世界インスタンスの初期化、逐次更新、観測、介入、分岐、チェックポイント保存/復元を提供する。

### 2.1 初期化と実行

- `init_world(seed: string, mesh_level: number, config?: object) -> { world_id, tick, era, cell_count }`
- `step_world(world_id: string, tick_count: number) -> void`
- `set_simulation_rate(world_id: string, rate: number) -> void`

### 2.2 観測

- `get_field(world_id: string, field_kind: string, lod: number) -> FieldResponse`
  - `field_kind`: `height` / `river_flux` / `plate_id` / `river_next`
- `get_metrics(world_id: string) -> MetricsResponse`
- `get_plate_stats(world_id: string) -> PlateStatsResponse`

### 2.3 介入と分岐

- `apply_intervention(world_id: string, op_batch: InterventionOp[]) -> { applied, rejected }`
- `fork_world(world_id: string, tick: number) -> { source_world_id, world_id, tick }`

### 2.4 保存と復元

- `save_checkpoint(world_id: string) -> { snapshot_id, world_id, tick }`
- `load_checkpoint(snapshot_id: string) -> { source_snapshot_id, world_id, tick }`
