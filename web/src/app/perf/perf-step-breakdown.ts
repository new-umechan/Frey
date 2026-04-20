export const STEP_BREAKDOWN_SAMPLE_INTERVAL = 4;

const STEP_BREAKDOWN_METRIC_NAMES = [
    "step_feedback",
    "step_geology_terrain",
    "step_climate",
    "step_glaciology",
    "step_geology_river",
    "step_ecology",
    "step_civilization",
    "step_transition",
    "step_sync_erosion",
    "step_observe_world_change",
    "step_history_snapshot",
];

interface PerfRecorder {
    pushSample: (metricName: string, value: number) => void;
}

interface ProfiledResult {
    [key: string]: unknown;
    steps?: number;
}

interface StepBreakdownOptions {
    stepCountKey?: string;
}

export function pushStepBreakdownSamples(perfRecorder: PerfRecorder | null, profiledResult: ProfiledResult, options: StepBreakdownOptions = {}) {
    if (!perfRecorder || !profiledResult) {
        return;
    }
    const stepCountKey = options.stepCountKey ?? "steps";
    const steps = Math.max(1, Math.floor((profiledResult[stepCountKey] as number) ?? 1));
    const rawMetricByName: Record<string, string> = {
        step_feedback: "exec_feedback_ms",
        step_geology_terrain: "exec_geology_terrain_ms",
        step_climate: "exec_climate_ms",
        step_glaciology: "exec_glaciology_ms",
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
        const rawValue = profiledResult[rawFieldName] as number | undefined;
        if (rawValue === undefined || !Number.isFinite(rawValue)) {
            continue;
        }
        perfRecorder.pushSample(metricName, rawValue / steps);
    }
}
