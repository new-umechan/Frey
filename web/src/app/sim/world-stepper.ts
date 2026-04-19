import { type WorldState, type AppState } from "../state/app-state";
import { getDefaultExecDisplayPhase, type RuntimeState } from "../runtime/state";
import { type EraMetrics, type EraScaleConfig } from "../state/era-presets";
import { type TickPerfRecorder } from "../perf/recorder";
import { type EngineClient, type MetricsResult } from "../engine/engine-client";
import { type TerrainRenderer } from "../visualizers/terrain-renderer";
import {
    type SyncDeltaOptions,
    type SyncVisibleOptions,
    type CoreBuffers,
    type SyncDeltaResult,
    type CoreDeltaApplyResult,
} from "../sim/sync/types";
import { type FieldKind } from "../sim/sync/constants";

export interface WorldStepperOptions {
    engineClient: EngineClient;
    world: WorldState;
    worldState: RuntimeState;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: MetricsResult) => EraMetrics;
    setEraScale: (era: string) => void;
    syncWorldDeltaFromController: (options: SyncDeltaOptions) => Promise<SyncDeltaResult>;
    syncVisibleCoreFieldsFromController: (options: SyncVisibleOptions) => Promise<CoreDeltaApplyResult>;
    getDeltaFieldKindsForView: (options: { viewMode: string; cellMetric: string }) => FieldKind[];
    refreshWorldStats: () => Promise<boolean>;
    syncClimateUi: () => void;
    syncAfterWorldStep: (options: { previousTick: number; nextTick: number; ticksAdvanced: number; batched: boolean }) => void;
    setStatus: (msg: string) => void;
    getCurrentState: () => AppState;
    getCurrentEraMetrics: () => EraMetrics;
    getActiveWorldId: () => string | null;
    getCurrentTerrainData: () => CoreBuffers | null;
    pushStepBreakdownSamples: (recorder: TickPerfRecorder | null, profiled: Record<string, unknown>) => void;
    getEraScalePreset: (era: string) => EraScaleConfig & { key: string };
}

export function createWorldStepper(options: WorldStepperOptions) {
    const {
        engineClient,
        world,
        worldState,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldDeltaFromController,
        syncVisibleCoreFieldsFromController,
        getDeltaFieldKindsForView,
        refreshWorldStats,
        syncClimateUi,
        syncAfterWorldStep,
        setStatus,
        getCurrentState,
        getCurrentEraMetrics,
        getActiveWorldId,
        getCurrentTerrainData,
        pushStepBreakdownSamples,
        getEraScalePreset,
    } = options;

    const shouldRefreshStatsForAdvance = (previousTick: number | undefined, nextTick: number | undefined): boolean => {
        const safePrev = Math.max(0, Math.floor(previousTick ?? 0));
        const safeNext = Math.max(safePrev, Math.floor(nextTick ?? safePrev));
        if (safeNext <= safePrev) {
            return false;
        }
        return Math.floor(safePrev / 8) < Math.floor(safeNext / 8);
    };

    const getCurrentDeltaFieldKinds = () => {
        const state = getCurrentState();
        return getDeltaFieldKindsForView({
            viewMode: state.currentViewMode,
            cellMetric: state.currentCellMetric,
        });
    };

    const syncVisibleFieldsForCurrentView = async () => {
        const state = getCurrentState();
        const activeWorldId = getActiveWorldId();
        const currentTerrainData = getCurrentTerrainData();
        if (!activeWorldId || !currentTerrainData) {
            return;
        }
        const deltaResult = await syncVisibleCoreFieldsFromController({
            engineClient,
            worldId: activeWorldId,
            core: currentTerrainData,
            fieldKinds: getCurrentDeltaFieldKinds(),
        });
        terrainRenderer.applyCoreChanges(currentTerrainData, deltaResult, state.currentSurfaceMode, world.tick);
    };

    const syncCompletedWorldStep = async (tickOptions: { benchmarkMode?: boolean; batchCount?: number; previousTick?: number; batched?: boolean } = {}, perfRecorder: TickPerfRecorder | null = null) => {
        const liveState = getCurrentState();
        const activeWorldId = getActiveWorldId();
        const currentTerrainData = getCurrentTerrainData();
        if (!activeWorldId || !currentTerrainData) {
            return false;
        }
        const benchmarkMode = tickOptions?.benchmarkMode === true;
        const batchCount = Math.max(1, Math.floor(tickOptions?.batchCount ?? 1));
        const previousTick = Math.max(0, Math.floor(tickOptions?.previousTick ?? world.tick));
        const nextTick = previousTick + batchCount;
        const shouldRefreshStats = benchmarkMode ? false : shouldRefreshStatsForAdvance(previousTick, nextTick);
        const { changes, statsRefreshed } = await syncWorldDeltaFromController({
            engineClient,
            worldId: activeWorldId,
            world,
            core: currentTerrainData,
            currentSurfaceMode: liveState.currentSurfaceMode,
            terrainRenderer,
            createEraMetrics,
            buildEraMetricsFromRuntime,
            setEraScale,
            refreshStats: shouldRefreshStats,
            refreshWorldStats,
            deltaFieldKinds: getCurrentDeltaFieldKinds(),
            perfRecorder,
        });
        if (!benchmarkMode && (changes?.metric || statsRefreshed)) {
            syncClimateUi();
        }

        if (!benchmarkMode && world.tick > 0 && shouldRefreshStats) {
            const preset = getEraScalePreset(liveState.currentEraScale);
            const currentEraMetrics = getCurrentEraMetrics();
            setStatus(
                `Running (${liveState.currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel} | tick=${world.tick}`,
            );
        }
        if (!benchmarkMode) {
            syncAfterWorldStep({
                previousTick,
                nextTick: world.tick,
                ticksAdvanced: batchCount,
                batched: tickOptions?.batched === true,
            });
        }
        return true;
    };

    const stepWorldTick = async (perfRecorder: TickPerfRecorder | null = null, tickOptions: { sampleStepBreakdown?: boolean; batchCount?: number; benchmarkMode?: boolean; batched?: boolean } = {}) => {
        const activeWorldId = getActiveWorldId();
        if (!activeWorldId || !getCurrentTerrainData()) {
            return false;
        }

        const runTick = async () => {
            const sampleStepBreakdown = tickOptions?.sampleStepBreakdown === true;
            const batchCount = Math.max(1, Math.floor(tickOptions?.batchCount ?? 1));
            const previousTick = world.tick;

            if (perfRecorder) {
                const start = performance.now();
                if (sampleStepBreakdown) {
                    const profiled = await engineClient.exec_world_profiled(activeWorldId, batchCount);
                    pushStepBreakdownSamples(perfRecorder, profiled);
                } else {
                    await engineClient.exec_world(activeWorldId, batchCount);
                }
                perfRecorder.pushSample("exec_world", performance.now() - start);
            } else {
                await engineClient.exec_world(activeWorldId, batchCount);
            }

            return await syncCompletedWorldStep({
                ...tickOptions,
                previousTick,
            }, perfRecorder);
        };

        if (perfRecorder) {
            const start = performance.now();
            const result = await runTick();
            perfRecorder.pushSample("tick_total", performance.now() - start);
            return result;
        }
        return await runTick();
    };

    const stepWorldPlayback = async () => {
        const activeWorldId = getActiveWorldId();
        if (!activeWorldId || !getCurrentTerrainData()) {
            worldState.sliceBusy = false;
            worldState.slicePhase = getDefaultExecDisplayPhase(worldState);
            return {
                processedTicks: 0,
                busy: false,
                phase: worldState.slicePhase,
            };
        }

        const response = await engineClient.exec_world_slice(
            activeWorldId,
            Math.max(1, Math.floor(worldState.sliceWorkBudget ?? 1)),
        );
        worldState.sliceBusy = response?.busy === true;
        worldState.slicePhase = typeof response?.phase === "string"
            ? response.phase
            : getDefaultExecDisplayPhase(worldState);
        const processedTicks = Math.max(0, Math.floor(response?.processed_ticks ?? 0));
        if (processedTicks > 0) {
            await syncCompletedWorldStep({
                previousTick: Math.max(0, world.tick - processedTicks),
                batchCount: processedTicks,
                batched: false,
            });
        }
        return {
            processedTicks,
            busy: worldState.sliceBusy,
            phase: worldState.slicePhase,
        };
    };

    return {
        syncVisibleFieldsForCurrentView,
        stepWorldTick,
        stepWorldPlayback,
    };
}
