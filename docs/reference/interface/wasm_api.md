# WASM API仕様

JSから利用する現行WASM公開APIを定義する。

## 1. Core API

- `generate_mesh(level: number) -> { positions, indices, cell_overlay_positions, cell_overlay_cell_ids, cell_overlay_lift }`
- `generate_geology(seed: string, params?: GeologyParams) -> GeologyOutput`
- `build_render_positions(input) -> number[]`

補足:

- `cell_overlay_positions` はセル上面 overlay 用の非 index 三角形頂点列。
- `cell_overlay_cell_ids` は `cell_overlay_positions` の各頂点が属するセル ID。
- `cell_overlay_lift` は `cell_overlay_positions` の各頂点に対する押し出し係数（0.0 または 1.0）。
    - 1.0: 指標による上方向オフセットを適用（上面/側面上端）
    - 0.0: 指標オフセットを適用しない（側面下端）
- `generate_geology`の`params`未指定時は`GeologyParams::default()`を使用する。
- `GeologyParams`には従来項目に加えて、プレート運動・境界再分類・沈み込み開始閾値・マントル熱場・プルーム関連の項目が含まれる。

## 2. WorldSimController API

`WorldSimController`は世界インスタンスの初期化、timeline 操作、観測を提供する。

### 2.1 初期化と実行

- `init_world(seed: string, mesh_level: number, config?: InitWorldConfig) -> { world_id, tick, head_tick, era, cell_count }`
- `advance_timeline(world_id: string, tick_count: number) -> { world_id, tick, head_tick, advanced_ticks }`
- `exec_world(world_id: string, tick_count: number) -> void`
- `exec_world_slice(world_id: string, work_budget: number) -> { world_id, processed_ticks, busy, phase, tick }`
- `exec_world_profiled(world_id: string, tick_count: number) -> StepWorldProfiledResponse`
- `exec_world_profiled_detail(world_id: string, tick_count: number) -> StepWorldProfiledDetailResponse`
- `set_simulation_rate(world_id: string, rate: number) -> void`

`InitWorldConfig`:

- `geology_params?: GeologyParams`
- `simulation_rate?: number`（内部で`0.1..=32.0`にclamp）
- `timeline?: { checkpoint_interval?: number, checkpoint_limit?: number, undo_log_limit?: number }`
    - `max_estimated_bytes?: number`

実行仕様:

- `exec_world`は`tick_count`に`simulation_rate`を掛けた回数だけ内部更新する。
- `advance_timeline` は単一 timeline の cursor を進める正本 API である。
- `advance_timeline` は `head_tick` を超えるぶんだけ新規計算し、`head_tick` 以内なら既存 timeline 上を移動する。
- 地形更新はWorldの1Tickごとに1回実行される。
- `exec_world_slice` は通常再生向けの再開可能APIで、1回の呼び出しで `work_budget` 個の内部phaseだけ進める。
- `processed_ticks` はtick完了後の post-step まで終わった回数のみ返し、tick途中の状態は外部観測に出さない。
- `phase` は次に実行される phase を示し、`busy=true` の間は同一の論理tickが継続中である。
- timeline 操作の公開整合点は常に `tick 完了境界` であり、partial tick は返さない。

### 2.2 観測

- `get_field(world_id: string, field_kind: string, lod: number) -> FieldResponse`
    - `field_kind`: `height` / `river_flux` / `plate_id` / `river_next` / `river_transport_cost` / `surface_water_access` / `food_energy_mean` / `food_energy_variance` / `buffer_capacity` / `mobility_capacity` / `land_use_intensity` / `subsistence_gathering` / `subsistence_hunting` / `subsistence_fishing` / `subsistence_cultivation` / `subsistence_herding` / `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` / `sink_id` / `sink_spill_to` / `sink_capacity_remaining` / `sink_fill_ratio` / `biome` / `mantle_heat` / `temperature` / `precipitation` / `runoff` / `ocean_temperature` / `wind_u` / `wind_v` / `moisture_flux_u` / `moisture_flux_v`
- `get_view_delta(world_id: string, options?: { include_fields?: string[] }) -> ViewDeltaResponse`
- `get_timeline_state(world_id: string) -> TimelineStateResponse`
- `get_metrics(world_id: string) -> MetricsResponse`
- `get_plate_stats(world_id: string) -> PlateStatsResponse`
- `list_checkpoint_ticks(world_id: string) -> { world_id, interval, ticks }`

補足:

- 内部河川表現はMFD（複数流下先+重み）を使用する。
- `WorldState.hydrology.river_downstream` は `[(cell, weight)]` の配列として保持し、`get_field` の
  `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` は互換のためCSR形式へ変換して返す。
- `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` はCSR形式で1セルあたり最大4流下先を表す。
- `river_next` は互換用のprimary流下先（最大重みの流下先）を返す。
- `get_view_delta` は差分のみを返す。`include_fields` 未指定時は全対象フィールドを返す。
- `get_timeline_state` は `current_tick`、`head_tick`、checkpoint/undo log window、`tick_boundary` を返す。
- `get_timeline_state` は加えて `checkpoint_estimated_bytes`、`undo_log_estimated_bytes`、`total_estimated_bytes`、`max_estimated_bytes` を返す。

`get_view_delta` の内部同期（`TimelineViewCache`）:

- `WorldSimController` は各 `world_id` ごとに `World` 本体とは別に `TimelineViewCache` を保持し、差分返却専用のシャドウ状態として利用する。
- 追跡対象フィールドは `height` / `river_flux` / `river_next` / `mantle_heat` / `temperature` / `precipitation`。
- 差分は `exec_world*` 実行後の観測で更新される。`seek_world_to_tick` では対象Worldの現在値から再初期化される。
- `get_view_delta` は pending 差分を返す one-shot API で、同じ差分は次回以降に再返却されない。
- `include_fields` 指定時、対象外フィールドの pending 差分は保持せず破棄される。
- 変更セル率が閾値以上（現行実装では40%以上）の場合、当該フィールドは範囲差分ではなく `mode: "full"` で返す。
- 疎な更新では `mode: "bitmap"` を返す。`dirty_bitmap` の立っているセル順に値配列が並ぶ。

互換性のため、旧 API 名 `get_world_delta` は alias として残す。

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

`CausalMetricValue`:

- `metric_id: string`
- `label: string`
- `value: number`
- `unit: string`
- `display_value: string`

### 2.3 checkpoint seek

- `seek_world_to_tick(world_id: string, tick: number) -> { world_id, tick, head_tick }`
- `rewind_world_by_ticks(world_id: string, tick_count: number) -> { world_id, tick, head_tick, rewound_ticks }`

互換性のため、旧 API 名 `restore_world_to_tick` は alias として残す。

## 3. エラー方針

- `world_id`が不正な場合は`JsValue`エラーを返す。
- `mesh_level > 8`はエラー。
- `exec_world(world_id, 0)`はno-opで成功する。
- `set_simulation_rate` の `rate` が非有限値（NaN/Inf）の場合はエラー。
- `seek_world_to_tick` の `tick` が不正（負値・非整数・未保存tick）の場合はエラー。
- `rewind_world_by_ticks` は可能なら tick 単位 undo log を使って巻き戻し、必要時は checkpoint+seek にフォールバックする。
- `seek_world_to_tick` と `rewind_world_by_ticks` は単一 timeline 上の cursor 移動であり、未来側 checkpoint / undo log は破棄しない。
- retention は `checkpoint_limit` / `undo_log_limit` に加えて `max_estimated_bytes` でも prune される。バイト値は近似であり、厳密な allocator 使用量ではない。
- budget 超過時でも、最低限の seek / replay 足場として初期 checkpoint と最新 checkpoint は優先保持する。

## 4. 互換性方針

- 既存の公開APIシグネチャ（関数名・引数・戻り値の基本構造）は原則維持する。
- `MetricsResponse` の既存フィールドは後方互換のため削除しない。
- 新規フィールド追加は後方互換な拡張として扱い、既存クライアントの読み取りを破壊しない。
- 内部実装（`WorldState` 分割、実行パイプライン、Feedback構造）の再編は非公開詳細とし、互換レイヤで既存挙動を維持する。
- `get_world_delta`、`list_history_ticks`、`restore_world_to_tick` は互換 alias として維持する。
