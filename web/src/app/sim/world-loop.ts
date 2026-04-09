import { DEFAULT_ERA_SCALE, type WorldSubsystemKey } from "../../shared/constants";
import { type WorldState } from "../state/app-state";
import { getDefaultExecDisplayPhase, type RuntimeState, type WorldLayers } from "../runtime/state";
import { type EraMetrics } from "../state/era-presets";

export function resetWorldProgress(
    world: WorldState,
    worldState: RuntimeState,
    createEmptyLayers: () => WorldLayers,
    createInitialBudgets: () => Record<WorldSubsystemKey, number>,
    createEraMetrics: (era: string) => EraMetrics
): EraMetrics {
    world.tick = 0;
    world.era = DEFAULT_ERA_SCALE;
    world.layers = createEmptyLayers();
    world.budgets = createInitialBudgets();
    worldState.accumulatorMs = 0;
    worldState.lastFrameTimeMs = null;
    worldState.sliceBusy = false;
    worldState.slicePhase = getDefaultExecDisplayPhase(worldState);
    worldState.pendingRiverSteps = 0;
    worldState.terrainErosionDirty = false;
    worldState.terrainCoreDirty = false;
    worldState.latestActivity.geology = 0;
    worldState.latestActivity.climate = 0;
    worldState.latestActivity.ecology = 0;
    worldState.latestActivity.civilization = 0;
    for (const key of Object.keys(worldState.carry) as WorldSubsystemKey[]) {
        worldState.carry[key] = 0;
    }
    for (const key of Object.keys(worldState.executedSteps) as WorldSubsystemKey[]) {
        worldState.executedSteps[key] = 0;
    }
    if (worldState.playback) {
        worldState.playback.historyInterval = 32;
        worldState.playback.selectedTick = null;
        worldState.playback.availableTicks = [];
        worldState.playback.eventLog = [];
        worldState.playback.nextLogId = 1;
    }
    worldState.isRunning = true;
    return createEraMetrics(DEFAULT_ERA_SCALE);
}

export function advanceWorldLoop(
    nowMs: number,
    worldState: RuntimeState,
    canRunTick: () => boolean,
    stepWorldPlayback: () => { processedTicks: number } | undefined
): void {
    if (!Number.isFinite(nowMs)) {
        return;
    }
    if (worldState.lastFrameTimeMs === null) {
        worldState.lastFrameTimeMs = nowMs;
        return;
    }

    const frameDeltaMs = Math.min(nowMs - worldState.lastFrameTimeMs, 250);
    worldState.lastFrameTimeMs = nowMs;

    const playbackActive = canRunTick();
    if (!playbackActive && !worldState.sliceBusy) {
        return;
    }

    if (playbackActive) {
        worldState.accumulatorMs += frameDeltaMs;
    }

    if (!worldState.sliceBusy && worldState.accumulatorMs < worldState.runtimeTickMs) {
        return;
    }

    const result = stepWorldPlayback();
    if (result && result.processedTicks > 0) {
        worldState.accumulatorMs = Math.max(
            0,
            worldState.accumulatorMs - (worldState.runtimeTickMs * result.processedTicks),
        );
    }
    if (playbackActive || worldState.sliceBusy) {
        worldState.accumulatorMs = Math.min(worldState.accumulatorMs, worldState.runtimeTickMs);
    }
}
