import { buildCoreBuffers } from "./core-builders";
import { applyWorldDeltaToCore } from "./delta-sync";
import { refreshWorldStatsFromController } from "./stats-sync";
import {
    type SyncOptions,
    type SyncDeltaOptions,
    type SyncVisibleOptions,
    type SyncWorldResult,
    type SyncDeltaResult,
    type CoreDeltaApplyResult,
} from "./types";
import { type FieldDelta } from "../../perf/world-core";

function sanitizeTick(rawTick: unknown): number {
    const tick = Math.floor(Number(rawTick));
    if (!Number.isFinite(tick) || tick < 0) {
        return 0;
    }
    return tick;
}

function sanitizeBudget(rawBudget: unknown): number {
    const budget = Math.floor(Number(rawBudget));
    if (!Number.isFinite(budget) || budget < 0) {
        return 0;
    }
    return budget;
}

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
    terrainRenderer.setSeaLevelOffset(Number(metrics.sea_level_offset) || 0);

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
        metrics,
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
        preloadedDelta,
        perfRecorder,
    } = options;

    let worldDelta: unknown = null;
    if (preloadedDelta !== undefined) {
        worldDelta = preloadedDelta;
    } else if (perfRecorder) {
        const start = performance.now();
        worldDelta = await engineClient.get_view_delta(worldId, { include_fields: deltaFieldKinds });
        perfRecorder.pushSample("get_view_delta", performance.now() - start);
    } else {
        worldDelta = await engineClient.get_view_delta(worldId, { include_fields: deltaFieldKinds });
    }

    const deltaView = (worldDelta ?? {}) as {
        tick?: unknown;
        era?: unknown;
        budgets?: {
            geology?: unknown;
            climate?: unknown;
            ecology?: unknown;
            civilization?: unknown;
        };
        deltas?: FieldDelta[];
    };
    world.tick = sanitizeTick(deltaView.tick);
    world.engineView.tick = world.tick;
    if (typeof deltaView.era === "string" && deltaView.era.length > 0) {
        world.engineView.era = deltaView.era;
    }
    world.engineView.budgets = {
        geology: sanitizeBudget(deltaView.budgets?.geology),
        climate: sanitizeBudget(deltaView.budgets?.climate),
        ecology: sanitizeBudget(deltaView.budgets?.ecology),
        civilization: sanitizeBudget(deltaView.budgets?.civilization),
    };
    const deltaResult = applyWorldDeltaToCore(core, deltaView as { deltas?: FieldDelta[] });
    const { changes, dirtyCells } = deltaResult;
    world.engineView.deltaRevision += 1;

    let eraMetrics = null;
    let statsRefreshed = false;
    if (refreshStats) {
        const metrics = await refreshWorldStats();
        statsRefreshed = metrics !== null;
        if (metrics) {
            terrainRenderer.setSeaLevelOffset(Number(metrics.sea_level_offset) || 0);
            const era = String(metrics.era ?? world.era);
            eraMetrics = buildEraMetricsFromRuntime(era, metrics);
            setEraScale(era);
        }
    }

    terrainRenderer.applyCoreChanges(core, deltaResult, currentSurfaceMode, world.tick, perfRecorder);

    return {
        changes,
        dirtyCells,
        eraMetrics,
        statsRefreshed,
    };
}

export async function syncVisibleCoreFieldsFromController(options: SyncVisibleOptions): Promise<CoreDeltaApplyResult> {
    const { engineClient, worldId, core, fieldKinds } = options;
    const worldDelta = (await engineClient.get_view_delta(worldId, {
        include_fields: fieldKinds,
    })) as { deltas?: FieldDelta[] };
    return applyWorldDeltaToCore(core, worldDelta as { deltas?: FieldDelta[] });
}
