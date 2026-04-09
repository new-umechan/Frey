import { type WorldState, type AppState } from "../state/app-state";
import { getDefaultExecDisplayPhase, type RuntimeState } from "../runtime/state";
import { type EraMetrics, type EraScaleConfig } from "../state/era-presets";
import { type TickPerfRecorder } from "../perf/recorder";
import { type EngineClient } from "../engine/engine-client";
import { type TerrainRenderer } from "../visualizers/terrain-renderer";
import { type SyncDeltaOptions, type SyncVisibleOptions, type CoreBuffers } from "../sim/sync/types";
import { type FieldKind } from "../sim/sync/constants";

export interface WorldStepperOptions {
    engineClient: EngineClient;
    world: WorldState;
    worldState: RuntimeState;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: any) => EraMetrics;
    setEraScale: (era: string) => void;
    syncWorldDeltaFromController: (options: SyncDeltaOptions) => Promise<{ changes: any; statsRefreshed: boolean }>;
    syncVisibleCoreFieldsFromController: (options: SyncVisibleOptions) => Promise<any>;
    getDeltaFieldKindsForView: (options: { viewMode: string; cellMetric: string }) => FieldKind[];
    refreshWorldStats: () => Promise<boolean>;
    syncClimateUi: () => void;
    syncAfterWorldStep: (options: { previousTick: number; nextTick: number; ticksAdvanced: number; batched: boolean }) => void;
    setStatus: (msg: string) => void;
    getCurrentState: () => AppState;
    pushStepBreakdownSamples: (recorder: TickPerfRecorder, profiled: any) => void;
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
        if (!state.activeWorldId || !state.currentTerrainData) {
            return;
        }
        const changes = await syncVisibleCoreFieldsFromController({
            engineClient,
            worldId: state.activeWorldId,
            core: state.currentTerrainData as CoreBuffers,
            fieldKinds: getCurrentDeltaFieldKinds(),
        });
        terrainRenderer.applyCoreChanges(state.currentTerrainData as CoreBuffers, changes, state.currentSurfaceMode, world.tick);
    };

    const syncCompletedWorldStep = async (tickOptions: { benchmarkMode?: boolean; batchCount?: number; previousTick?: number; batched?: boolean } = {}, perfRecorder: TickPerfRecorder | null = null) => {
        const liveState = getCurrentState();
        if (!liveState.activeWorldId || !liveState.currentTerrainData) {
            return false;
        }
        const benchmarkMode = tickOptions?.benchmarkMode === true;
        const batchCount = Math.max(1, Math.floor(tickOptions?.batchCount ?? 1));
        const previousTick = Math.max(0, Math.floor(tickOptions?.previousTick ?? world.tick));
        const nextTick = previousTick + batchCount;
        const shouldRefreshStats = benchmarkMode ? false : shouldRefreshStatsForAdvance(previousTick, nextTick);
        const { changes, statsRefreshed } = await syncWorldDeltaFromController({
            engineClient,
            worldId: liveState.activeWorldId,
            world,
            core: liveState.currentTerrainData as CoreBuffers,
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
            setStatus(
                `Running (${liveState.currentSeed}) | ${preset.label} / 1Tick=${liveState.currentEraMetrics.tickLabel} | tick=${world.tick}`,
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
        const state = getCurrentState();
        if (!state.activeWorldId || !state.currentTerrainData) {
            return false;
        }

        const runTick = async () => {
            const liveState = getCurrentState();
            const sampleStepBreakdown = tickOptions?.sampleStepBreakdown === true;
            const batchCount = Math.max(1, Math.floor(tickOptions?.batchCount ?? 1));
            const previousTick = world.tick;

            if (perfRecorder) {
                const start = performance.now();
                if (sampleStepBreakdown) {
                    const profiled = await engineClient.exec_world_profiled(liveState.activeWorldId!, batchCount);
                    pushStepBreakdownSamples(perfRecorder, profiled);
                } else {
                    await engineClient.exec_world(liveState.activeWorldId!, batchCount);
                }
                perfRecorder.pushSample("exec_world", performance.now() - start);
            } else {
                await engineClient.exec_world(liveState.activeWorldId!, batchCount);
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
        const state = getCurrentState();
        if (!state.activeWorldId || !state.currentTerrainData) {
            worldState.sliceBusy = false;
            worldState.slicePhase = getDefaultExecDisplayPhase(worldState);
            return {
                processedTicks: 0,
                busy: false,
                phase: worldState.slicePhase,
            };
        }

        const response = await engineClient.exec_world_slice(
            state.activeWorldId,
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
