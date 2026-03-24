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
- `exec_world_profiled(world_id: string, tick_count: number) -> StepWorldProfiledResponse`
- `exec_world_profiled_detail(world_id: string, tick_count: number) -> StepWorldProfiledDetailResponse`
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
  - `field_kind`: `height` / `river_flux` / `plate_id` / `river_next` / `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` / `sink_id` / `sink_spill_to` / `sink_capacity_remaining` / `sink_fill_ratio` / `mantle_heat` / `temperature` / `precipitation` / `runoff` / `ocean_temperature`
- `get_world_delta(world_id: string, options?: { include_fields?: string[] }) -> WorldDeltaResponse`
- `get_metrics(world_id: string) -> MetricsResponse`
- `get_plate_stats(world_id: string) -> PlateStatsResponse`
- `list_history_ticks(world_id: string) -> { world_id, interval, ticks }`
- `list_checkpoints() -> { checkpoints: { snapshot_id, tick }[] }`

補足:
- 内部河川表現はMFD（複数流下先+重み）を使用する。
- `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` はCSR形式で1セルあたり最大3流下先を表す。
- `river_next` は互換用のprimary流下先（最大重みの流下先）を返す。
- `get_world_delta` は差分のみを返す。`include_fields` 未指定時は全対象フィールドを返す。

`get_world_delta` の内部同期（`WorldSyncState`）:
- `WorldSimController` は各 `world_id` ごとに `World` 本体とは別に `WorldSyncState` を保持し、差分返却専用のシャドウ状態として利用する。
- 追跡対象フィールドは `height` / `river_flux` / `river_next` / `mantle_heat` / `temperature` / `precipitation`。
- 差分は、`exec_world*` 実行後および `apply_intervention` 後の観測で更新される。`fork_world` / `restore_world_to_tick` / `load_checkpoint` では対象Worldの現在値から再初期化される。
- `get_world_delta` は pending 差分を返す one-shot API で、同じ差分は次回以降に再返却されない。
- `include_fields` 指定時、対象外フィールドの pending 差分は保持せず破棄される。
- 変更セル率が閾値以上（現行実装では40%以上）の場合、当該フィールドは範囲差分ではなく `mode: "full"` で返す。

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
- `river_active_cells: number`
- `river_fragmentation_ratio: number`
- `river_ocean_reach_ratio: number`
- `river_mainstem_persistence: number`
- `river_flux_concentration: number`
- `continent_count: number`
- `largest_continent_cells: number`

### 2.3 介入と分岐

- `apply_intervention(world_id: string, op_batch: InterventionOp[]) -> { applied, rejected }`
- `fork_world(world_id: string, tick: number) -> { source_world_id, world_id, tick }`
- `restore_world_to_tick(world_id: string, tick: number) -> { world_id, tick }`

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
- `set_simulation_rate` の `rate` が非有限値（NaN/Inf）の場合はエラー。
- `fork_world` / `restore_world_to_tick` の `tick` が不正（負値・非整数・未保存tick）の場合はエラー。

## 4. 互換性方針

- 既存の公開APIシグネチャ（関数名・引数・戻り値の基本構造）は維持する。
- `MetricsResponse` の既存フィールドは後方互換のため削除しない。
- 新規フィールド追加は後方互換な拡張として扱い、既存クライアントの読み取りを破壊しない。
- 内部実装（`WorldState` 分割、実行パイプライン、Feedback構造）の再編は非公開詳細とし、互換レイヤで既存挙動を維持する。
- `get_world_delta` は公開APIとして維持する。
