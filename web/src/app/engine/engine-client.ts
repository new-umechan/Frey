import type { ExecModuleDocRecord, ExecModuleGraphRecord } from "../../interface/wasm";

export interface MeshGenerationResult {
    positions: number[] | Float32Array;
    indices: number[] | Uint32Array;
}

export interface InitWorldResult {
    world_id: string;
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
    geology?: number;
    climate?: number;
    ecology?: number;
    civilization?: number;
}

export interface MetricsResult {
    tick?: number;
    era_scale?: string;
    real_years_per_tick?: number;
    runtime_tick_ms?: number;
    budgets?: RuntimeBudgetsResult;
    budget_geology?: number;
    budget_climate?: number;
    budget_ecology?: number;
    budget_civilization?: number;
    plate_count?: number;
    land_ratio?: number;
}

export type ProfiledExecResult = Record<string, unknown>;
export type WorldDeltaResult = unknown;
export type FieldResult = unknown;

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
    get_exec_modules: () => Promise<ExecModuleDocRecord[]>;
    get_exec_module_graph: () => Promise<ExecModuleGraphRecord>;
}
