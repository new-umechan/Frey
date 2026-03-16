export const STEP_BREAKDOWN_SAMPLE_INTERVAL = 4;

const STEP_BREAKDOWN_METRIC_NAMES = [
    "step_feedback",
    "step_geology_terrain",
    "step_climate",
    "step_geology_river",
    "step_ecology",
    "step_civilization",
    "step_transition",
    "step_sync_erosion",
    "step_observe_world_change",
    "step_history_snapshot",
];

export function pushStepBreakdownSamples(perfRecorder, profiledResult, options = {}) {
    if (!perfRecorder || !profiledResult) {
        return;
    }
    const stepCountKey = options.stepCountKey ?? "steps";
    const steps = Math.max(1, Math.floor(profiledResult[stepCountKey] ?? 1));
    for (const metricName of STEP_BREAKDOWN_METRIC_NAMES) {
        const rawValue = profiledResult[`${metricName}_ms`];
        if (!Number.isFinite(rawValue)) {
            continue;
        }
        perfRecorder.pushSample(metricName, rawValue / steps);
    }
}
