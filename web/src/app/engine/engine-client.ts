import type { ExecModuleDocRecord, ExecModuleGraphRecord } from "../../interface/wasm";

export interface MeshGenerationResult {
    positions: number[] | Float32Array;
    indices: number[] | Uint32Array;
}

export interface InitWorldResult {
    world_id: string;
}

export interface ForkWorldResult {
    source_world_id: string;
    world_id: string;
    tick: number;
}

export interface ExecWorldSliceResult {
    busy?: boolean;
    processed_ticks?: number;
    phase?: string;
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
    dirty_bitmap?: number[] | null;
    f32_data?: number[] | null;
    u32_data?: number[] | null;
    i32_data?: number[] | null;
}

export interface ViewDeltaResult {
    world_id: string;
    tick: number;
    era: string;
    real_years_per_tick: number;
    runtime_tick_ms: number;
    budgets: RuntimeBudgetsResult;
    deltas: FieldDeltaResult[];
}

export type WorldDeltaResult = ViewDeltaResult;
export type FieldResult = Record<string, unknown>;

export interface EngineClient {
    generate_mesh: (level: number) => Promise<MeshGenerationResult>;
    init_world: (seed: string, meshLevel: number, config: unknown) => Promise<InitWorldResult>;
    exec_world: (worldId: string, tickCount: number) => Promise<void>;
    exec_world_slice: (worldId: string, workBudget: number) => Promise<ExecWorldSliceResult>;
    exec_world_profiled: (worldId: string, tickCount: number) => Promise<ProfiledExecResult>;
    get_world_delta: (worldId: string, options?: unknown) => Promise<WorldDeltaResult>;
    get_metrics: (worldId: string) => Promise<MetricsResult | null>;
    get_field: (worldId: string, fieldKind: string, window: number) => Promise<FieldResult>;
    list_history_ticks: (worldId: string) => Promise<HistoryTicksResult>;
    restore_world_to_tick: (worldId: string, tick: number) => Promise<void>;
    set_simulation_rate: (worldId: string, rate: number) => Promise<void>;
    set_target_sea_ratio: (worldId: string, targetSeaRatio: number) => Promise<void>;
    fork_world: (worldId: string, tick: number) => Promise<ForkWorldResult>;
    get_exec_modules: () => Promise<ExecModuleDocRecord[]>;
    get_exec_module_graph: () => Promise<ExecModuleGraphRecord>;
}
