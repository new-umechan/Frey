# WASM API仕様

JSから利用する現行WASM公開APIを定義する。

## 1. Core API

- `generate_mesh(level: number) -> { positions, indices }`
- `generate_geology(seed: string, params?: GeologyParams) -> GeologyOutput`
- `build_render_positions(input) -> number[]`

補足:
- `generate_geology`の`params`未指定時は`GeologyParams::default()`を使用する。
- `GeologyParams`には従来項目に加えて、プレート運動・境界再分類・沈み込み開始閾値・マントル熱場・プルーム関連の項目が含まれる。

## 2. WorldSimController API

`WorldSimController`は世界インスタンスの初期化、逐次更新、観測、介入、分岐、チェックポイント保存/復元を提供する。

### 2.1 初期化と実行

- `init_world(seed: string, mesh_level: number, config?: InitWorldConfig) -> { world_id, tick, era, cell_count }`
- `exec_world(world_id: string, tick_count: number) -> void`
- `set_simulation_rate(world_id: string, rate: number) -> void`

`InitWorldConfig`:
- `geology_params?: GeologyParams`
- `target_sea_ratio?: number`（内部で`0.02..=0.98`にclamp）
- `simulation_rate?: number`（内部で`0.1..=32.0`にclamp）

実行仕様:
- `exec_world`は`tick_count`に`simulation_rate`を掛けた回数だけ内部更新する。
- 地形更新はWorldの1Tickごとに1回実行される。

### 2.2 観測

- `get_field(world_id: string, field_kind: string, lod: number) -> FieldResponse`
  - `field_kind`: `height` / `river_flux` / `plate_id` / `river_next` / `sink_id` / `sink_spill_to` / `sink_capacity_remaining` / `sink_fill_ratio` / `mantle_heat` / `temperature` / `precipitation` / `runoff` / `ocean_temperature`
- `get_metrics(world_id: string) -> MetricsResponse`
- `get_plate_stats(world_id: string) -> PlateStatsResponse`

`MetricsResponse`:
- `world_id: string`
- `tick: number`
- `era: string`
- `simulation_rate: number`
- `real_years_per_tick: number`
- `runtime_tick_ms: number`
- `budgets: { geology: number, climate: number, ecology: number, civilization: number }`
- `cell_count: number`
- `land_cells: number`
- `land_ratio: number`
- `mean_height: number`
- `height_std_dev: number`
- `mean_river_flux: number`
- `max_height: number`
- `min_height: number`
- `max_river_flux: number`
- `top10_river_flux_sum: number`
- `continent_count: number`
- `largest_continent_cells: number`

### 2.3 介入と分岐

- `apply_intervention(world_id: string, op_batch: InterventionOp[]) -> { applied, rejected }`
- `fork_world(world_id: string, tick: number) -> { source_world_id, world_id, tick }`

`InterventionOp`:
- `cell_id: number`
- `field: "height" | "river_flux" | "river_next" | "plate_id"`
- `value: number`

### 2.4 保存と復元

- `save_checkpoint(world_id: string) -> { snapshot_id, world_id, tick }`
- `load_checkpoint(snapshot_id: string) -> { source_snapshot_id, world_id, tick }`

## 3. エラー方針

- `world_id`や`snapshot_id`が不正な場合は`JsValue`エラーを返す。
- `mesh_level > 8`はエラー。
- `exec_world(world_id, 0)`はno-opで成功する。
