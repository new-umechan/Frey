import {
    DEFAULT_ERA_SCALE,
    ERA_SCALE_PRESETS,
    formatRealYearsPerTick,
} from "../../core/constants.js";

export function getEraScalePreset(key) {
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

export function createEraMetrics(key = DEFAULT_ERA_SCALE) {
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

export function buildEraMetricsFromRuntime(era, metrics) {
    const fallback = createEraMetrics(era);
    return {
        key: Object.hasOwn(ERA_SCALE_PRESETS, era) ? era : DEFAULT_ERA_SCALE,
        tickLabel: formatRealYearsPerTick(Number(metrics?.real_years_per_tick) || 0),
        runtimeTickMs: Number(metrics?.runtime_tick_ms) || fallback.runtimeTickMs,
        budgets: {
            geology: Number(metrics?.budgets?.geology) || 0,
            climate: Number(metrics?.budgets?.climate) || 0,
            ecology: Number(metrics?.budgets?.ecology) || 0,
            civilization: Number(metrics?.budgets?.civilization) || 0,
        },
    };
}

export function renderEraScaleControls(eraScaleSelect, eraScaleTickLabel, eraScaleWeightFields, currentEraScale, currentEraMetrics) {
    eraScaleSelect.value = currentEraScale;
    eraScaleTickLabel.textContent = `1Tick: ${currentEraMetrics.tickLabel}`;
    eraScaleWeightFields.geology.textContent = currentEraMetrics.budgets.geology.toFixed(2);
    eraScaleWeightFields.climate.textContent = currentEraMetrics.budgets.climate.toFixed(2);
    eraScaleWeightFields.ecology.textContent = currentEraMetrics.budgets.ecology.toFixed(2);
    eraScaleWeightFields.civilization.textContent = currentEraMetrics.budgets.civilization.toFixed(2);
}
