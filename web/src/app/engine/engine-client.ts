export type ExecModuleDocRecord = Record<string, unknown>;
export type ExecModuleGraphRecord = {
  modules: unknown[];
  edges: unknown[];
};

export interface MeshGenerationResult {
  positions: number[] | Float32Array;
  indices: number[] | Uint32Array;
  cell_overlay_positions: number[] | Float32Array;
  cell_overlay_cell_ids: number[] | Uint32Array;
  cell_overlay_lift: number[] | Float32Array;
}

export interface InitWorldResult {
  world_id: string;
  tick?: number;
  head_tick?: number;
  dev_snapshot_restore_status?: "used" | "fallback";
  dev_snapshot_stage?: string;
  dev_snapshot_reason?: string;
}

export interface ExecWorldSliceResult {
  busy?: boolean;
  processed_ticks?: number;
  phase?: string;
  head_tick?: number;
  tick_boundary?: string;
}

export interface ExecWorldSliceAndDeltaResult {
  slice: ExecWorldSliceResult;
  delta: WorldDeltaResult | null;
}

export interface HistoryTicksResult {
  ticks?: unknown[];
  interval?: number;
}

export interface RuntimeBudgetsResult {
  geology: number;
  climate: number;
  ecology: number;
  civilization: number;
}

export interface MetricsResult {
  world_id: string;
  tick: number;
  era: string;
  simulation_rate: number;
  real_years_per_tick: number;
  runtime_tick_ms: number;
  budgets: RuntimeBudgetsResult;
  cell_count: number;
  land_cells: number;
  land_ratio: number;
  sea_level_offset: number;
  mean_height: number;
  height_std_dev: number;
  mean_river_flux: number;
  max_height: number;
  min_height: number;
  max_river_flux: number;
  top10_river_flux_sum: number;
  river_active_cells: number;
  river_fragmentation_ratio: number;
  river_ocean_reach_ratio: number;
  river_mainstem_persistence: number;
  river_flux_concentration: number;
  continent_count: number;
  largest_continent_cells: number;
  plate_count?: number;
}

export type ProfiledExecResult = Record<string, unknown>;

export interface DeltaRangeResult {
  start: number;
  end: number;
}

export interface FieldDeltaResult {
  field_kind: string;
  mode: "full" | "delta" | "bitmap";
  ranges: DeltaRangeResult[];
  dirty_bitmap?: number[] | Uint32Array | null;
  f32_data?: number[] | Float32Array | null;
  u32_data?: number[] | Uint32Array | null;
  i32_data?: number[] | Int32Array | null;
}

export interface ViewDeltaResult {
  world_id: string;
  tick: number;
  head_tick?: number;
  era: string;
  real_years_per_tick: number;
  runtime_tick_ms: number;
  budgets: RuntimeBudgetsResult;
  deltas: FieldDeltaResult[];
}

export type WorldDeltaResult = ViewDeltaResult;
export type FieldResult = Record<string, unknown>;
export type TimelineAdvanceResult = {
  world_id: string;
  tick: number;
  head_tick: number;
  advanced_ticks: number;
};
export type TimelineStateResult = {
  world_id: string;
  current_tick: number;
  head_tick: number;
  checkpoint_interval: number;
  checkpoint_limit?: number;
  checkpoint_count: number;
  undo_log_limit?: number;
  undo_log_count: number;
  checkpoint_estimated_bytes?: number;
  undo_log_estimated_bytes?: number;
  total_estimated_bytes?: number;
  max_estimated_bytes?: number | null;
  tick_boundary?: string;
};

export interface EngineClient {
  generate_mesh: (level: number) => Promise<MeshGenerationResult>;
  init_world: (
    seed: string,
    meshLevel: number,
    config: unknown,
    options?: { devSnapshotStage?: string },
  ) => Promise<InitWorldResult>;
  advance_timeline: (
    worldId: string,
    tickCount: number,
  ) => Promise<TimelineAdvanceResult>;
  exec_world: (worldId: string, tickCount: number) => Promise<void>;
  advance_timeline_slice: (
    worldId: string,
    workBudget: number,
  ) => Promise<ExecWorldSliceResult>;
  advance_timeline_slice_and_delta: (
    worldId: string,
    workBudget: number,
    options?: unknown,
  ) => Promise<ExecWorldSliceAndDeltaResult>;
  exec_world_slice: (
    worldId: string,
    workBudget: number,
  ) => Promise<ExecWorldSliceResult>;
  exec_world_slice_and_delta: (
    worldId: string,
    workBudget: number,
    options?: unknown,
  ) => Promise<ExecWorldSliceAndDeltaResult>;
  exec_world_profiled: (
    worldId: string,
    tickCount: number,
  ) => Promise<ProfiledExecResult>;
  get_view_delta: (
    worldId: string,
    options?: unknown,
  ) => Promise<ViewDeltaResult>;
  get_world_delta: (
    worldId: string,
    options?: unknown,
  ) => Promise<WorldDeltaResult>;
  get_timeline_state: (worldId: string) => Promise<TimelineStateResult>;
  get_metrics: (worldId: string) => Promise<MetricsResult | null>;
  get_field: (
    worldId: string,
    fieldKind: string,
    window: number,
  ) => Promise<FieldResult>;
  list_checkpoint_ticks: (worldId: string) => Promise<HistoryTicksResult>;
  list_history_ticks: (worldId: string) => Promise<HistoryTicksResult>;
  seek_world_to_tick: (worldId: string, tick: number) => Promise<void>;
  rewind_world_by_ticks: (worldId: string, tickCount: number) => Promise<void>;
  restore_world_to_tick: (worldId: string, tick: number) => Promise<void>;
  set_simulation_rate: (worldId: string, rate: number) => Promise<void>;
  get_exec_modules: () => Promise<ExecModuleDocRecord[]>;
  get_exec_module_graph: () => Promise<ExecModuleGraphRecord>;
}
