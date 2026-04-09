import {
    DEFAULT_ERA_SCALE,
    ERA_SCALE_PRESETS,
    formatRealYearsPerTick,
    type EraScaleConfig,
    type WorldSubsystemKey,
} from "../../shared/constants";

export type { EraScaleConfig, WorldSubsystemKey };

export interface EraMetrics {
    key: string;
    tickLabel: string;
    runtimeTickMs: number;
    budgets: Record<WorldSubsystemKey, number>;
}

export function getEraScalePreset(key: string): EraScaleConfig & { key: string } {
    if (Object.hasOwn(ERA_SCALE_PRESETS, key)) {
        return {
            key,
            ...ERA_SCALE_PRESETS[key],
        };
    }
    return {
        key: DEFAULT_ERA_SCALE,
        ...ERA_SCALE_PRESETS[DEFAULT_ERA_SCALE],
    };
}

export function createEraMetrics(key = DEFAULT_ERA_SCALE): EraMetrics {
    const preset = getEraScalePreset(key);
    return {
        key,
        tickLabel: preset.tickLabel,
        runtimeTickMs: Number.isFinite(preset.runtimeTickMs) ? preset.runtimeTickMs : 120,
        budgets: {
            geology: Number(preset.weights.geology ?? 0),
            climate: Number(preset.weights.climate ?? 0),
            ecology: Number(preset.weights.ecology ?? 0),
            civilization: Number(preset.weights.civilization ?? 0),
        },
    };
}

export function buildEraMetricsFromRuntime(era: string, metrics: unknown): EraMetrics {
    const runtimeMetrics = metrics as {
        real_years_per_tick?: number;
        runtime_tick_ms?: number;
        budgets?: {
            geology?: number;
            climate?: number;
            ecology?: number;
            civilization?: number;
        };
    } | null;
    const fallback = createEraMetrics(era);
    return {
        key: Object.hasOwn(ERA_SCALE_PRESETS, era) ? era : DEFAULT_ERA_SCALE,
        tickLabel: formatRealYearsPerTick(Number(runtimeMetrics?.real_years_per_tick) || 0),
        runtimeTickMs: Number(runtimeMetrics?.runtime_tick_ms) || fallback.runtimeTickMs,
        budgets: {
            geology: Number(runtimeMetrics?.budgets?.geology) || 0,
            climate: Number(runtimeMetrics?.budgets?.climate) || 0,
            ecology: Number(runtimeMetrics?.budgets?.ecology) || 0,
            civilization: Number(runtimeMetrics?.budgets?.civilization) || 0,
        },
    };
}

export interface EraScaleWeightFields {
    geology: HTMLElement;
    climate: HTMLElement;
    ecology: HTMLElement;
    civilization: HTMLElement;
}

export function renderEraScaleControls(
    eraScaleSelect: HTMLSelectElement,
    eraScaleTickLabel: HTMLElement,
    eraScaleWeightFields: EraScaleWeightFields,
    currentEraScale: string,
    currentEraMetrics: EraMetrics
): void {
    eraScaleSelect.value = currentEraScale;
    eraScaleTickLabel.textContent = `1Tick: ${currentEraMetrics.tickLabel}`;
    eraScaleWeightFields.geology.textContent = currentEraMetrics.budgets.geology.toFixed(2);
    eraScaleWeightFields.climate.textContent = currentEraMetrics.budgets.climate.toFixed(2);
    eraScaleWeightFields.ecology.textContent = currentEraMetrics.budgets.ecology.toFixed(2);
    eraScaleWeightFields.civilization.textContent = currentEraMetrics.budgets.civilization.toFixed(2);
}
