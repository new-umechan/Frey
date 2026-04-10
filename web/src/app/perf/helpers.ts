import {
    DELTA_FIELD_KIND_BY_VIEW,
    FIELD_KIND_BY_CELL_METRIC,
    RIVER_BREAKDOWN_METRIC_NAMES,
    STEP_BREAKDOWN_METRIC_NAMES,
} from "./constants";
import { type PerfProfile } from "./recorder";

interface PerfSampleRecorder {
    pushSample: (name: string, valueMs: number) => void;
}

type ProfiledResult = Record<string, unknown> & {
    steps?: number;
};

export function defaultNowMs(): number {
    if (globalThis.performance && typeof globalThis.performance.now === "function") {
        return globalThis.performance.now();
    }
    return Date.now();
}

export function roundMs(value: number): number {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000) / 1000;
}

export function roundRatio(value: number): number {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000000) / 1000000;
}

export function formatError(error: unknown): string {
    if (error instanceof Error) {
        return `${error.name}: ${error.message}`;
    }
    return String(error);
}

export function getDeltaFieldKindsForProfile(profile: PerfProfile): string[] {
    if (profile?.viewMode === "metric") {
        const metricKey = typeof profile?.cellMetric === "string" ? profile.cellMetric : "height";
        const metricField = FIELD_KIND_BY_CELL_METRIC[metricKey] ?? "height";
        return ["height", "river_flux", "river_next", metricField];
    }
    const viewMode = typeof profile?.viewMode === "string" ? profile.viewMode : "normal";
    return DELTA_FIELD_KIND_BY_VIEW[viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal;
}

export function pushStepBreakdownSamples(recorder: PerfSampleRecorder, profiledResult: ProfiledResult): void {
    if (!profiledResult) {
        return;
    }
    const steps = Math.max(1, Math.floor(profiledResult.steps ?? 1));
    const rawMetricByName: Record<string, string> = {
        step_feedback: "exec_feedback_ms",
        step_geology_terrain: "exec_geology_terrain_ms",
        step_climate: "exec_climate_ms",
        step_geology_river: "exec_hydrology_ms",
        step_ecology: "exec_ecology_ms",
        step_civilization: "exec_society_ms",
        step_transition: "exec_transition_ms",
        step_sync_erosion: "step_sync_erosion_ms",
        step_observe_world_change: "step_observe_world_change_ms",
        step_history_snapshot: "step_history_snapshot_ms",
    };
    for (const metricName of STEP_BREAKDOWN_METRIC_NAMES) {
        const rawFieldName = rawMetricByName[metricName] ?? `${metricName}_ms`;
        const rawValue = profiledResult[rawFieldName];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        recorder.pushSample(metricName, Number(rawValue) / steps);
    }
}

export function pushRiverBreakdownSamples(recorder: PerfSampleRecorder, profiledResult: ProfiledResult): void {
    if (!profiledResult) {
        return;
    }
    const steps = Math.max(1, Math.floor(profiledResult.steps ?? 1));
    for (const metricName of RIVER_BREAKDOWN_METRIC_NAMES) {
        const rawValue = profiledResult[`${metricName}_ms`];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        recorder.pushSample(metricName, Number(rawValue) / steps);
    }
}
