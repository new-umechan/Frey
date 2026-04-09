import { buildCoreBuffers } from "./core-builders";
import { applyWorldDeltaToCore } from "./delta-sync";
import { refreshWorldStatsFromController } from "./stats-sync";
import { type SyncOptions, type SyncDeltaOptions, type SyncVisibleOptions } from "./types";

export async function syncWorldFromController(options: SyncOptions) {
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

    const core = await buildCoreBuffers(worldSimController, worldId);
    setCurrentTerrainData(core);

    const metrics = await worldSimController.get_metrics(worldId);
    if (!metrics) {
        return null;
    }

    const era = String(metrics.era_scale ?? world.era);
    const eraMetrics = buildEraMetricsFromRuntime(era, metrics);
    setEraScale(era);

    await refreshWorldStatsFromController({
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

export async function syncWorldDeltaFromController(options: SyncDeltaOptions) {
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
        const start = performance.now();
        worldDelta = await worldSimController.get_world_delta(worldId, { include_fields: deltaFieldKinds });
        perfRecorder.pushSample("get_world_delta", performance.now() - start);
    } else {
        worldDelta = await worldSimController.get_world_delta(worldId, { include_fields: deltaFieldKinds });
    }

    const changes = applyWorldDeltaToCore(core, worldDelta);

    let eraMetrics = null;
    let statsRefreshed = false;
    if (refreshStats) {
        statsRefreshed = await refreshWorldStats();
        const metrics = await worldSimController.get_metrics(worldId);
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

export async function syncVisibleCoreFieldsFromController(options: SyncVisibleOptions) {
    const { worldSimController, worldId, core, fieldKinds } = options;
    const worldDelta = await worldSimController.get_world_delta(worldId, {
        include_fields: fieldKinds,
    });
    return applyWorldDeltaToCore(core, worldDelta);
}
