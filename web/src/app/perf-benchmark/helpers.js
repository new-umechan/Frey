import {
    DELTA_FIELD_KIND_BY_VIEW,
    FIELD_KIND_BY_CELL_METRIC,
    RIVER_BREAKDOWN_METRIC_NAMES,
    STEP_BREAKDOWN_METRIC_NAMES,
} from "./constants.js";

export function defaultNowMs() {
    if (globalThis.performance && typeof globalThis.performance.now === "function") {
        return globalThis.performance.now();
    }
    return Date.now();
}

export function roundMs(value) {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000) / 1000;
}

export function roundRatio(value) {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000000) / 1000000;
}

export function formatError(error) {
    if (error instanceof Error) {
        return `${error.name}: ${error.message}`;
    }
    return String(error);
}

export function getDeltaFieldKindsForProfile(profile) {
    if (profile?.viewMode === "metric") {
        const metricField = FIELD_KIND_BY_CELL_METRIC[profile?.cellMetric] ?? "height";
        return ["height", "river_flux", "river_next", metricField];
    }
    return DELTA_FIELD_KIND_BY_VIEW[profile?.viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal;
}

export function pushStepBreakdownSamples(recorder, profiledResult) {
    if (!profiledResult) {
        return;
    }
    const steps = Math.max(1, Math.floor(profiledResult.steps ?? 1));
    for (const metricName of STEP_BREAKDOWN_METRIC_NAMES) {
        const rawValue = profiledResult[`${metricName}_ms`];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        recorder.pushSample(metricName, rawValue / steps);
    }
}

export function pushRiverBreakdownSamples(recorder, profiledResult) {
    if (!profiledResult) {
        return;
    }
    const steps = Math.max(1, Math.floor(profiledResult.steps ?? 1));
    for (const metricName of RIVER_BREAKDOWN_METRIC_NAMES) {
        const rawValue = profiledResult[`${metricName}_ms`];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        recorder.pushSample(metricName, rawValue / steps);
    }
}
