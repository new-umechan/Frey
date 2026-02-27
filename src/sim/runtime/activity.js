import {
    SUBSYSTEM_ACTIVITY_STEP_BASELINE,
    SUBSYSTEM_ACTIVITY_WEIGHT_MIX,
} from "../../core/constants.js";

export function clamp01(value) {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.min(1, Math.max(0, value));
}

export function smoothstep(edge0, edge1, x) {
    if (!Number.isFinite(x)) {
        return 0;
    }
    if (edge1 <= edge0) {
        return x >= edge1 ? 1 : 0;
    }
    const t = clamp01((x - edge0) / (edge1 - edge0));
    return t * t * (3 - 2 * t);
}

export function recordSubsystemActivity(worldState, subsystemKey, signal) {
    const normalized = clamp01(signal);
    const prev = clamp01(worldState.latestActivity[subsystemKey] ?? 0);
    worldState.latestActivity[subsystemKey] = clamp01(prev + normalized * (1 - prev));
}

export function buildObservedActivityForTick(worldState, worldBudgets, subsystemKey, preset) {
    const raw = clamp01(worldState.latestActivity[subsystemKey] ?? 0);
    const steps = Math.max(0, worldBudgets?.[subsystemKey] ?? 0);
    const stepBaseline = clamp01((SUBSYSTEM_ACTIVITY_STEP_BASELINE[subsystemKey] ?? 0) * steps);
    const weight = clamp01(preset?.weights?.[subsystemKey] ?? 0);
    const weightFactor = 1 - SUBSYSTEM_ACTIVITY_WEIGHT_MIX + weight * SUBSYSTEM_ACTIVITY_WEIGHT_MIX;
    return clamp01(Math.max(raw, stepBaseline) * weightFactor);
}
