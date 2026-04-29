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

`WorldSimController`は世界インスタンスの初期化、逐次更新、観測、履歴tick復元を提供する。

### 2.1 初期化と実行

- `init_world(seed: string, mesh_level: number, config?: InitWorldConfig) -> { world_id, tick, era, cell_count }`
- `exec_world(world_id: string, tick_count: number) -> void`
- `exec_world_slice(world_id: string, work_budget: number) -> { world_id, processed_ticks, busy, phase, tick }`
- `exec_world_profiled(world_id: string, tick_count: number) -> StepWorldProfiledResponse`
- `exec_world_profiled_detail(world_id: string, tick_count: number) -> StepWorldProfiledDetailResponse`
- `set_simulation_rate(world_id: string, rate: number) -> void`

`InitWorldConfig`:

- `geology_params?: GeologyParams`
- `simulation_rate?: number`（内部で`0.1..=32.0`にclamp）

実行仕様:

- `exec_world`は`tick_count`に`simulation_rate`を掛けた回数だけ内部更新する。
- 地形更新はWorldの1Tickごとに1回実行される。
- `exec_world_slice` は通常再生向けの再開可能APIで、1回の呼び出しで `work_budget` 個の内部phaseだけ進める。
- `processed_ticks` はtick完了後の post-step まで終わった回数のみ返し、tick途中の状態は外部観測に出さない。
- `phase` は次に実行される phase を示し、`busy=true` の間は同一の論理tickが継続中である。

### 2.2 観測

- `get_field(world_id: string, field_kind: string, lod: number) -> FieldResponse`
    - `field_kind`: `height` / `river_flux` / `plate_id` / `river_next` / `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` / `sink_id` / `sink_spill_to` / `sink_capacity_remaining` / `sink_fill_ratio` / `biome` / `mantle_heat` / `temperature` / `precipitation` / `runoff` / `ocean_temperature` / `wind_u` / `wind_v` / `moisture_flux_u` / `moisture_flux_v`
- `get_world_delta(world_id: string, options?: { include_fields?: string[] }) -> WorldDeltaResponse`
- `get_causal_exploration_demo(world_id: string) -> CausalExplorationDemoResponse`
- `get_metrics(world_id: string) -> MetricsResponse`
- `get_plate_stats(world_id: string) -> PlateStatsResponse`
- `list_history_ticks(world_id: string) -> { world_id, interval, ticks }`

補足:

- 内部河川表現はMFD（複数流下先+重み）を使用する。
- `WorldState.hydrology.river_downstream` は `[(cell, weight)]` の配列として保持し、`get_field` の
  `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` は互換のためCSR形式へ変換して返す。
- `river_downstream_offset` / `river_downstream_cell` / `river_downstream_weight` はCSR形式で1セルあたり最大3流下先を表す。
- `river_next` は互換用のprimary流下先（最大重みの流下先）を返す。
- `get_world_delta` は差分のみを返す。`include_fields` 未指定時は全対象フィールドを返す。

`get_world_delta` の内部同期（`WorldSyncState`）:

- `WorldSimController` は各 `world_id` ごとに `World` 本体とは別に `WorldSyncState` を保持し、差分返却専用のシャドウ状態として利用する。
- 追跡対象フィールドは `height` / `river_flux` / `river_next` / `mantle_heat` / `temperature` / `precipitation`。
- 差分は `exec_world*` 実行後の観測で更新される。`restore_world_to_tick` では対象Worldの現在値から再初期化される。
- `get_world_delta` は pending 差分を返す one-shot API で、同じ差分は次回以降に再返却されない。
- `include_fields` 指定時、対象外フィールドの pending 差分は保持せず破棄される。
- 変更セル率が閾値以上（現行実装では40%以上）の場合、当該フィールドは範囲差分ではなく `mode: "full"` で返す。
- 疎な更新では `mode: "bitmap"` を返す。`dirty_bitmap` の立っているセル順に値配列が並ぶ。

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

`CausalExplorationDemoResponse`:

- `demo_id: string`
- `features: CausalFeatureDescriptor[]`
- `trace_segments: CausalTraceSegment[]`
- `metrics: CausalMetricValue[]`
- `display_mapping: CausalDisplayMapping`
- `evidence: CausalEvidenceEntry[]`

`CausalFeatureDescriptor`:

- `feature_id: string`
- `feature_type: "border_segment" | "ridge_or_mountain_band" | "tectonic_compression_or_plate_boundary"`
- `label: string`
- `short_label: string`
- `anchor: { x: number, y: number, z: number }`
- `metrics: CausalMetricValue[]`
- `uncertainty_stage: "low" | "medium" | "high"`

`CausalTraceSegment`:

- `trace_id: string`
- `label: string`
- `source_feature_id: string`
- `target_feature_id: string`
- `relation_type: "constraint_alignment" | "geomorphic_structure" | "tectonic_driver"`
- `path: { x: number, y: number, z: number }[]`
- `metrics: CausalMetricValue[]`
- `uncertainty_stage: "low" | "medium" | "high"`
- `evidence_ids: string[]`
- `display_key: string`

`CausalMetricValue`:

- `metric_id: string`
- `label: string`
- `value: number`
- `unit: string`
- `display_value: string`

`CausalDisplayMapping`:

- `feature_styles: { feature_id, color_hex, glow_intensity, pulse_hz, radius }[]`
- `trace_styles: { trace_id, color_hex, thickness, flow_speed, jitter_amplitude, label_short }[]`

`CausalEvidenceEntry`:

- `evidence_id: string`
- `trace_id: string`
- `evidence_type: "morphology" | "passability_proxy" | "tectonic_proxy"`
- `summary: string`
- `assumptions: string[]`
- `approximations: string[]`
- `uncertainty_reason: string`
- `reference_model: string`
- `reference_notes: string`

補足:

- 初回実装は `border_mountain_plate_demo` の固定データを返す。
- `world_id` は存在確認にだけ使い、不明な world の場合はエラーを返す。
- UI 層は `display_mapping` にない色・太さ・揺らぎを補完しない。
- この API は現時点では因果探索モードの恒久仕様ではなく、Demo Slice 実験用の公開面である。

### 2.3 履歴復元

- `restore_world_to_tick(world_id: string, tick: number) -> { world_id, tick }`

## 3. エラー方針

- `world_id`が不正な場合は`JsValue`エラーを返す。
- `mesh_level > 8`はエラー。
- `exec_world(world_id, 0)`はno-opで成功する。
- `set_simulation_rate` の `rate` が非有限値（NaN/Inf）の場合はエラー。
- `restore_world_to_tick` の `tick` が不正（負値・非整数・未保存tick）の場合はエラー。

## 4. 互換性方針

- 既存の公開APIシグネチャ（関数名・引数・戻り値の基本構造）は原則維持する。
- `MetricsResponse` の既存フィールドは後方互換のため削除しない。
- 新規フィールド追加は後方互換な拡張として扱い、既存クライアントの読み取りを破壊しない。
- 内部実装（`WorldState` 分割、実行パイプライン、Feedback構造）の再編は非公開詳細とし、互換レイヤで既存挙動を維持する。
- `get_world_delta` は公開APIとして維持する。
