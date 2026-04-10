import { buildCoreBuffers } from "./core-builders";
import { applyWorldDeltaToCore } from "./delta-sync";
import { refreshWorldStatsFromController } from "./stats-sync";
import {
    type SyncOptions,
    type SyncDeltaOptions,
    type SyncVisibleOptions,
    type SyncWorldResult,
    type SyncDeltaResult,
} from "./types";
import { type WorldChangeset } from "./constants";

export async function syncWorldFromController(options: SyncOptions): Promise<SyncWorldResult | null> {
    const {
        engineClient,
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

    const core = await buildCoreBuffers(engineClient, worldId);
    setCurrentTerrainData(core);

    const metrics = await engineClient.get_metrics(worldId);
    if (!metrics) {
        return null;
    }

    const era = String(metrics.era ?? world.era);
    const eraMetrics = buildEraMetricsFromRuntime(era, metrics);
    setEraScale(era);

    await refreshWorldStatsFromController({
        engineClient,
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

export async function syncWorldDeltaFromController(options: SyncDeltaOptions): Promise<SyncDeltaResult> {
    const {
        engineClient,
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

    let worldDelta: unknown = null;
    if (perfRecorder) {
        const start = performance.now();
        worldDelta = await engineClient.get_world_delta(worldId, { include_fields: deltaFieldKinds });
        perfRecorder.pushSample("get_world_delta", performance.now() - start);
    } else {
        worldDelta = await engineClient.get_world_delta(worldId, { include_fields: deltaFieldKinds });
    }

    const changes = applyWorldDeltaToCore(core, worldDelta);
    world.engineView.deltaRevision += 1;

    let eraMetrics = null;
    let statsRefreshed = false;
    if (refreshStats) {
        statsRefreshed = await refreshWorldStats();
        const metrics = await engineClient.get_metrics(worldId);
        if (metrics) {
            const era = String(metrics.era ?? world.era);
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

export async function syncVisibleCoreFieldsFromController(options: SyncVisibleOptions): Promise<WorldChangeset> {
    const { engineClient, worldId, core, fieldKinds } = options;
    const worldDelta = await engineClient.get_world_delta(worldId, {
        include_fields: fieldKinds,
    });
    return applyWorldDeltaToCore(core, worldDelta);
}
