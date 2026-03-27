import { buildCoreBuffers } from "./core-builders";
import { applyWorldDeltaToCore } from "./delta-sync";
import { refreshWorldStatsFromController } from "./stats-sync";
import { type SyncOptions, type SyncDeltaOptions, type SyncVisibleOptions } from "./types";

export function syncWorldFromController(options: SyncOptions) {
    const {
        worldSimController,
        worldId,
        world,
        currentSurfaceMode,
        terrainRenderer,
        buildEraMetricsFromRuntime,
        setEraScale,
        setCurrentTerrainData,
        statFields,
        level,
    } = options;

    const core = buildCoreBuffers(worldSimController, worldId);
    setCurrentTerrainData(core);

    const metrics = worldSimController.get_metrics(worldId);
    if (!metrics) {
        return null;
    }

    const era = String(metrics.era_scale ?? world.era);
    const eraMetrics = buildEraMetricsFromRuntime(era, metrics);
    setEraScale(era);

    refreshWorldStatsFromController({
        worldSimController,
        worldId,
        world,
        currentSeed: options.currentSeed,
        statFields,
        level,
    });

    terrainRenderer.initializeTerrain(core, currentSurfaceMode);

    return {
        eraMetrics,
    };
}

export function syncWorldDeltaFromController(options: SyncDeltaOptions) {
    const {
        worldSimController,
        worldId,
        world,
        core,
        currentSurfaceMode,
        terrainRenderer,
        buildEraMetricsFromRuntime,
        setEraScale,
        refreshStats,
        refreshWorldStats,
        deltaFieldKinds,
        perfRecorder,
    } = options;

    let worldDelta = null;
    if (perfRecorder) {
        worldDelta = perfRecorder.measure("get_world_delta", () =>
            worldSimController.get_world_delta(worldId, { include_fields: deltaFieldKinds })
        );
    } else {
        worldDelta = worldSimController.get_world_delta(worldId, { include_fields: deltaFieldKinds });
    }

    const changes = applyWorldDeltaToCore(core, worldDelta);

    let eraMetrics = null;
    let statsRefreshed = false;
    if (refreshStats) {
        statsRefreshed = refreshWorldStats();
        const metrics = worldSimController.get_metrics(worldId);
        if (metrics) {
            const era = String(metrics.era_scale ?? world.era);
            eraMetrics = buildEraMetricsFromRuntime(era, metrics);
            setEraScale(era);
        }
    }

    terrainRenderer.applyCoreChanges(core, changes, currentSurfaceMode, world.tick, perfRecorder);

    return {
        changes,
        eraMetrics,
        statsRefreshed,
    };
}

export function syncVisibleCoreFieldsFromController(options: SyncVisibleOptions) {
    const { worldSimController, worldId, core, fieldKinds } = options;
    const worldDelta = worldSimController.get_world_delta(worldId, {
        include_fields: fieldKinds,
    });
    return applyWorldDeltaToCore(core, worldDelta);
}
