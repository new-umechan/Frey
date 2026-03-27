import { TERRAIN_LEVEL } from "../interface/params/terrain";
import { RUNTIME_PARAMS } from "../interface/params/runtime";

export const LEVEL = TERRAIN_LEVEL;
export const DEFAULT_TERRAIN_SEED = "alpha";
export const DEFAULT_VIEW_MODE = "normal";
export const DEFAULT_CELL_METRIC = "height";
export const DEFAULT_CLIMATE_METRIC = "temperature";
export const DEFAULT_SURFACE_MODE = "globe";
export const DEFAULT_ERA_SCALE = "crust";
export const PLATE_HOVER_POPUP_DELAY_MS = 450;

export const WORLD_SUBSYSTEM_KEYS = [
    "geology",
    "climate",
    "ecology",
    "civilization",
] as const;

export type WorldSubsystemKey = (typeof WORLD_SUBSYSTEM_KEYS)[number];

export const LAYER_KIND = {
    CLIMATE: "climate",
    ECOLOGY: "ecology",
    CIVILIZATION: "civilization",
} as const;

export type LayerKind = (typeof LAYER_KIND)[keyof typeof LAYER_KIND];

export interface EraScaleConfig {
    label: string;
    tickLabel: string;
    runtimeTickMs: number;
    weights: Record<WorldSubsystemKey, number>;
}

export const ERA_SCALE_PRESETS: Record<string, EraScaleConfig> = Object.freeze({
    crust: {
        label: "地殻形成期",
        tickLabel: "500万年",
        runtimeTickMs: 70,
        weights: { geology: 4.0, climate: 0.0, ecology: 0.0, civilization: 0.0 },
    },
    environment: {
        label: "環境形成期",
        tickLabel: "100万年",
        runtimeTickMs: 150,
        weights: { geology: 3.0, climate: 3.0, ecology: 1.0, civilization: 0.0 },
    },
    life: {
        label: "先史期",
        tickLabel: "1000年",
        runtimeTickMs: 110,
        weights: { geology: 2.0, climate: 3.0, ecology: 4.0, civilization: 1.0 },
    },
    civilization: {
        label: "文明成立期",
        tickLabel: "100年",
        runtimeTickMs: 90,
        weights: { geology: 1.0, climate: 2.0, ecology: 2.0, civilization: 4.0 },
    },
    history: {
        label: "歴史展開期",
        tickLabel: "1年",
        runtimeTickMs: 70,
        weights: { geology: 1.0, climate: 1.0, ecology: 1.0, civilization: 4.0 },
    },
});

function formatCompactDecimal(value: number, digits: number): string {
    return value.toFixed(digits).replace(/\.0+$/, "");
}

export function formatRealYearsPerTick(years: number): string {
    if (!Number.isFinite(years) || years <= 0) {
        return "-";
    }
    if (years >= 100000000) {
        return `${formatCompactDecimal(years / 100000000, years >= 1000000000 ? 0 : 1)}億年`;
    }
    if (years >= 10000) {
        return `${formatCompactDecimal(years / 10000, years >= 100000 ? 0 : 1)}万年`;
    }
    return `${formatCompactDecimal(years, years >= 10 ? 0 : 1)}年`;
}

export const SUBSYSTEM_ACTIVITY_SIGNAL_GAIN = Object.freeze({
    geology: RUNTIME_PARAMS.activity_signal_gain_terrain + RUNTIME_PARAMS.activity_signal_gain_river,
    climate: RUNTIME_PARAMS.activity_signal_gain_climate,
    ecology: RUNTIME_PARAMS.activity_signal_gain_ecology,
    civilization: RUNTIME_PARAMS.activity_signal_gain_civilization,
});

export const SUBSYSTEM_ACTIVITY_STEP_BASELINE = Object.freeze({
    geology:
        RUNTIME_PARAMS.activity_step_baseline_terrain + RUNTIME_PARAMS.activity_step_baseline_river,
    climate: RUNTIME_PARAMS.activity_step_baseline_climate,
    ecology: RUNTIME_PARAMS.activity_step_baseline_ecology,
    civilization: RUNTIME_PARAMS.activity_step_baseline_civilization,
});

export const SUBSYSTEM_ACTIVITY_WEIGHT_MIX = RUNTIME_PARAMS.activity_weight_mix;
export const SUBSYSTEM_ACTIVITY_QUEUE_PRESSURE_GAIN = RUNTIME_PARAMS.activity_queue_pressure_gain;

export const PLATE_MOTION_SPEED_BY_ERA = Object.freeze({
    crust: 0.00045,
    environment: 0.00030,
    life: 0.00020,
    civilization: 0.00014,
    history: 0.00010,
});

export const PLATE_MOTION_REMAP_INTERVAL_BY_ERA = Object.freeze({
    crust: 4,
    environment: 7,
    life: 12,
    civilization: 18,
    history: 24,
});

export const PLATE_MOTION_ACTIVITY_GAIN = 10.0;

export const LAND_RATIO_RECOVERY_BY_ERA = Object.freeze({
    crust: 0.22,
    environment: 0.16,
    life: 0.11,
    civilization: 0.08,
    history: 0.06,
});

export const LAND_RATIO_FLOOR_BY_ERA = Object.freeze({
    crust: 0.94,
    environment: 0.90,
    life: 0.86,
    civilization: 0.82,
    history: 0.80,
});

export const RIVER_BUDGET_SCALE_BY_ERA = Object.freeze({
    crust: 0.08,
    environment: 0.22,
    life: 0.40,
    civilization: 0.55,
    history: 0.70,
});

export const TERRAIN_DYNAMICS_BY_ERA = Object.freeze({
    crust: { diffusion: 0.034, uplift: 0.025, subsidence: 0.011, fluvial: 0.0034, coastline: 0.016 },
    environment: { diffusion: 0.021, uplift: 0.013, subsidence: 0.0067, fluvial: 0.0039, coastline: 0.012 },
    life: { diffusion: 0.013, uplift: 0.007, subsidence: 0.0042, fluvial: 0.0035, coastline: 0.0085 },
    civilization: { diffusion: 0.009, uplift: 0.0040, subsidence: 0.0027, fluvial: 0.0029, coastline: 0.0065 },
    history: { diffusion: 0.0065, uplift: 0.0028, subsidence: 0.0018, fluvial: 0.0024, coastline: 0.0056 },
});

export const TERRAIN_HEIGHT_CLAMP = 1.2;
export const TERRAIN_UPLIFT_SATURATION_SOFT = 0.42;
export const TERRAIN_UPLIFT_SATURATION_HARD = 0.74;
export const TERRAIN_OCEAN_DIFFUSION_SCALE = 0.42;
export const TERRAIN_OCEAN_MAX_SUBSIDENCE = 0.0035;
export const TERRAIN_STRESS_MEMORY_DECAY = 0.86;
export const TERRAIN_STRESS_MEMORY_GAIN = 0.14;
export const TERRAIN_EARLY_OCEAN_GUARD_TICK = 96;
export const TERRAIN_OCEAN_MAX_DROP_EARLY = 0.00045;
export const TERRAIN_OCEAN_MAX_DROP_LATE = 0.0014;
export const PLATE_REMAP_MAX_FRACTION = 0.018;
export const PLATE_REMAP_DOMINANCE_CAP = 0.34;
export const PLATE_REMAP_SWITCH_MARGIN = 0.055;
