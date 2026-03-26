import { DEFAULT_ERA_SCALE } from "../core/constants.js";

const ADAPTIVE_SYNC_BATCH_MAX_TICKS = 4;
const ADAPTIVE_SYNC_BATCH_TRIGGER_LAG_TICKS = 2;

export function resetWorldProgress(world, worldState, createEmptyLayers, createInitialBudgets, createEraMetrics) {
    world.tick = 0;
    world.era = DEFAULT_ERA_SCALE;
    world.layers = createEmptyLayers();
    world.budgets = createInitialBudgets();
    worldState.accumulatorMs = 0;
    worldState.lastFrameTimeMs = null;
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

export function advanceWorldLoop(nowMs, worldState, canRunTick, stepWorldTick) {
    if (!Number.isFinite(nowMs)) {
        return;
    }
    if (worldState.lastFrameTimeMs === null) {
        worldState.lastFrameTimeMs = nowMs;
        return;
    }

    const frameDeltaMs = Math.min(nowMs - worldState.lastFrameTimeMs, 250);
    worldState.lastFrameTimeMs = nowMs;

    if (!canRunTick()) {
        return;
    }

    worldState.accumulatorMs += frameDeltaMs;
    let ticksProcessed = 0;
    while (
        worldState.accumulatorMs >= worldState.runtimeTickMs &&
        ticksProcessed < worldState.maxTicksPerFrame
    ) {
        const remainingBudget = worldState.maxTicksPerFrame - ticksProcessed;
        const lagTicks = Math.floor(worldState.accumulatorMs / worldState.runtimeTickMs);
        const shouldBatch = lagTicks >= ADAPTIVE_SYNC_BATCH_TRIGGER_LAG_TICKS;
        const batchCount = shouldBatch
            ? Math.max(1, Math.min(ADAPTIVE_SYNC_BATCH_MAX_TICKS, lagTicks, remainingBudget))
            : 1;
        stepWorldTick(null, {
            batchCount,
            batched: batchCount > 1,
        });
        worldState.accumulatorMs -= worldState.runtimeTickMs * batchCount;
        ticksProcessed += batchCount;
    }

    if (ticksProcessed >= worldState.maxTicksPerFrame) {
        worldState.accumulatorMs = Math.min(worldState.accumulatorMs, worldState.runtimeTickMs);
    }
}
