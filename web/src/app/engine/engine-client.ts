import type { ExecModuleDocRecord, ExecModuleGraphRecord } from "../../transport/wasm/frey-wasm-module";

export interface MeshGenerationResult {
    positions: number[] | Float32Array;
    indices: number[] | Uint32Array;
    cell_overlay_positions: number[] | Float32Array;
    cell_overlay_cell_ids: number[] | Uint32Array;
    cell_overlay_lift: number[] | Float32Array;
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

export type CausalFeatureType =
    | "border_segment"
    | "ridge_or_mountain_band"
    | "tectonic_compression_or_plate_boundary";

export type CausalRelationType =
    | "constraint_alignment"
    | "geomorphic_structure"
    | "tectonic_driver";

export type CausalEvidenceType =
    | "morphology"
    | "passability_proxy"
    | "tectonic_proxy";

export type UncertaintyStage = "low" | "medium" | "high";

export interface CausalLocationPoint {
    x: number;
    y: number;
    z: number;
}

export interface CausalMetricValue {
    metric_id: string;
    label: string;
    value: number;
    unit: string;
    display_value: string;
}

export interface CausalFeatureDescriptor {
    feature_id: string;
    feature_type: CausalFeatureType;
    label: string;
    short_label: string;
    anchor: CausalLocationPoint;
    metrics: CausalMetricValue[];
    uncertainty_stage: UncertaintyStage;
}

export interface CausalTraceSegment {
    trace_id: string;
    label: string;
    source_feature_id: string;
    target_feature_id: string;
    relation_type: CausalRelationType;
    path: CausalLocationPoint[];
    metrics: CausalMetricValue[];
    uncertainty_stage: UncertaintyStage;
    evidence_ids: string[];
    display_key: string;
}

export interface CausalDisplayFeatureStyle {
    feature_id: string;
    color_hex: string;
    glow_intensity: number;
    pulse_hz: number;
    radius: number;
}

export interface CausalDisplayTraceStyle {
    trace_id: string;
    color_hex: string;
    thickness: number;
    flow_speed: number;
    jitter_amplitude: number;
    label_short: string;
}

export interface CausalDisplayMapping {
    feature_styles: CausalDisplayFeatureStyle[];
    trace_styles: CausalDisplayTraceStyle[];
}

export interface CausalEvidenceEntry {
    evidence_id: string;
    trace_id: string;
    evidence_type: CausalEvidenceType;
    summary: string;
    assumptions: string[];
    approximations: string[];
    uncertainty_reason: string;
    reference_model: string;
    reference_notes: string;
}

export interface CausalExplorationDemoResult {
    demo_id: string;
    features: CausalFeatureDescriptor[];
    trace_segments: CausalTraceSegment[];
    metrics: CausalMetricValue[];
    display_mapping: CausalDisplayMapping;
    evidence: CausalEvidenceEntry[];
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
    exec_world_slice_and_delta: (
        worldId: string,
        workBudget: number,
        options?: unknown,
    ) => Promise<ExecWorldSliceAndDeltaResult>;
    exec_world_profiled: (worldId: string, tickCount: number) => Promise<ProfiledExecResult>;
    get_world_delta: (worldId: string, options?: unknown) => Promise<WorldDeltaResult>;
    get_causal_exploration_demo: (worldId: string) => Promise<CausalExplorationDemoResult>;
    get_metrics: (worldId: string) => Promise<MetricsResult | null>;
    get_field: (worldId: string, fieldKind: string, window: number) => Promise<FieldResult>;
    list_history_ticks: (worldId: string) => Promise<HistoryTicksResult>;
    restore_world_to_tick: (worldId: string, tick: number) => Promise<void>;
    set_simulation_rate: (worldId: string, rate: number) => Promise<void>;
    fork_world: (worldId: string, tick: number) => Promise<ForkWorldResult>;
    get_exec_modules: () => Promise<ExecModuleDocRecord[]>;
    get_exec_module_graph: () => Promise<ExecModuleGraphRecord>;
}
