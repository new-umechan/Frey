import { WORLD_CHANGESET } from "./constants.js";
import { buildCoreFromController, buildPlateInfoFromStats } from "./core-builders.js";
import { applyWorldDeltaToCore } from "./delta-sync.js";
import { fetchCoreFields } from "./field-io.js";
import { fetchWorldStats, updateUiStatsFromWorldStats } from "./stats-sync.js";

function applyWorldMetrics({
    world,
    metrics,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
}) {
    world.tick = Math.max(0, Math.floor(metrics.tick ?? 0));
    world.era = typeof metrics.era === "string" ? metrics.era : createEraMetrics().key;
    const nextEraMetrics = buildEraMetricsFromRuntime(world.era, metrics);
    world.budgets = { ...nextEraMetrics.budgets };
    setEraScale(world.era, nextEraMetrics);
}

function syncWorldState({
    world,
    metrics,
    core,
    currentSurfaceMode,
    terrainRenderer,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
    initializeTerrain = false,
    changes = WORLD_CHANGESET,
    perfRecorder = null,
}) {
    world.core = core;
    applyWorldMetrics({
        world,
        metrics,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
    });
    if (initializeTerrain) {
        terrainRenderer.initializeTerrain(core, currentSurfaceMode);
    } else {
        terrainRenderer.applyCoreChanges(
            core,
            changes,
            currentSurfaceMode,
            world.tick,
            perfRecorder,
        );
    }
    return {
        changes,
        metrics,
    };
}

function maybeRefreshStats({ refreshStats, refreshWorldStats }) {
    if (!refreshStats || typeof refreshWorldStats !== "function") {
        return { statsRefreshed: false, stats: null };
    }
    return {
        statsRefreshed: true,
        stats: refreshWorldStats(),
    };
}

export function syncWorldFromController({
    worldSimController,
    worldId,
    world,
    currentSeed,
    currentSurfaceMode,
    terrainRenderer,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
    setCurrentTerrainData,
    statFields,
    level,
}) {
    const stats = fetchWorldStats(worldSimController, worldId);
    const core = buildCoreFromController({
        ...fetchCoreFields(worldSimController, worldId),
        plateInfo: buildPlateInfoFromStats(stats.plateStats),
        targetLandRatio: stats.metrics.land_ratio,
    });
    setCurrentTerrainData(core);
    const result = syncWorldState({
        world,
        metrics: stats.metrics,
        core,
        currentSurfaceMode,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        initializeTerrain: true,
    });
    updateUiStatsFromWorldStats({ stats, currentSeed, statFields, level });
    return {
        ...result,
        statsRefreshed: true,
        stats,
    };
}

export function syncWorldDeltaFromController({
    worldSimController,
    worldId,
    world,
    currentSurfaceMode,
    terrainRenderer,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
    refreshStats,
    refreshWorldStats,
    deltaFieldKinds,
    perfRecorder = null,
}) {
    if (!world.core) {
        return {
            changes: null,
            statsRefreshed: false,
            metrics: null,
            stats: null,
        };
    }

    const deltaTask = () => {
        const worldDelta = worldSimController.get_world_delta(
            worldId,
            Array.isArray(deltaFieldKinds) && deltaFieldKinds.length > 0
                ? { include_fields: deltaFieldKinds }
                : undefined,
        );
        const changes = applyWorldDeltaToCore(world.core, worldDelta);
        return { worldDelta, changes };
    };
    const {
        worldDelta,
        changes,
    } = perfRecorder ? perfRecorder.measure("delta_sync", deltaTask) : deltaTask();
    const result = syncWorldState({
        world,
        metrics: worldDelta,
        core: world.core,
        currentSurfaceMode,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        changes,
        perfRecorder,
    });
    const refreshed = maybeRefreshStats({ refreshStats, refreshWorldStats });
    return {
        ...result,
        ...refreshed,
    };
}
