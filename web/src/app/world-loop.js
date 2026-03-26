import { DEFAULT_ERA_SCALE } from "../core/constants.js";

export function resetWorldProgress(world, worldState, createEmptyLayers, createInitialBudgets, createEraMetrics) {
    world.tick = 0;
    world.era = DEFAULT_ERA_SCALE;
    world.layers = createEmptyLayers();
    world.budgets = createInitialBudgets();
    worldState.accumulatorMs = 0;
    worldState.lastFrameTimeMs = null;
    worldState.sliceBusy = false;
    worldState.slicePhase = "feedback";
    worldState.pendingRiverSteps = 0;
    worldState.terrainErosionDirty = false;
    worldState.terrainCoreDirty = false;
    worldState.latestActivity.geology = 0;
    worldState.latestActivity.climate = 0;
    worldState.latestActivity.ecology = 0;
    worldState.latestActivity.civilization = 0;
    for (const key of Object.keys(worldState.carry)) {
        worldState.carry[key] = 0;
    }
    for (const key of Object.keys(worldState.executedSteps)) {
        worldState.executedSteps[key] = 0;
    }
    if (worldState.playback) {
        worldState.playback.isPlaying = true;
        worldState.playback.historyInterval = 32;
        worldState.playback.selectedTick = null;
        worldState.playback.availableTicks = [];
        worldState.playback.eventLog = [];
        worldState.playback.nextLogId = 1;
    }
    worldState.isRunning = true;
    return createEraMetrics(DEFAULT_ERA_SCALE);
}

export function advanceWorldLoop(nowMs, worldState, canRunTick, stepWorldPlayback) {
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
    if (result?.processedTicks > 0) {
        worldState.accumulatorMs = Math.max(
            0,
            worldState.accumulatorMs - (worldState.runtimeTickMs * result.processedTicks),
        );
    }
    if (playbackActive || worldState.sliceBusy) {
        worldState.accumulatorMs = Math.min(worldState.accumulatorMs, worldState.runtimeTickMs);
    }
}
