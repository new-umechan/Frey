import { TERRAIN_PARAMS } from "../../../interface/params/terrain";
import { DEFAULT_ERA_SCALE, LEVEL } from "../../../shared/constants";
import {
    createEraMetrics,
    buildEraMetricsFromRuntime,
    getEraScalePreset,
    renderEraScaleControls,
} from "../../state/era-presets";
import {
    createEmptyLayers,
    createInitialBudgets,
} from "../../runtime/state";
import { getDeltaFieldKindsForView } from "../../sim/sync/view-mode";
import { refreshWorldStatsFromController } from "../../sim/sync/stats-sync";
import {
    syncVisibleCoreFieldsFromController,
    syncWorldDeltaFromController,
    syncWorldFromController,
} from "../../sim/sync/world-state-sync";
import { createWorldUiController } from "../../controllers/world-ui-controller";
import { createWorldSessionController } from "../../sim/world-session-controller";
import { createWorldStepper } from "../../sim/world-stepper";
import {
    createPerfConsoleTable,
    createPerfProfile,
    formatPerfSummaryLine,
} from "../../perf/recorder";
import { createViewModeController } from "../../controllers/view-mode-controller";
import { normalizeCellMetric } from "../../visualizers/cell-metric";
import { createTerrainGenerationController } from "../../controllers/terrain-generation-controller";
import { createPlaybackController } from "../../playback/playback-controller";
import { pushStepBreakdownSamples } from "../../perf/perf-step-breakdown";
import { resetWorldProgress } from "../../sim/world-loop";
import { createPerfRuntime } from "../perf-runtime";
import { type RuntimeContext } from "./create-controller-runtime";
import { type CoreBuffers } from "../../sim/sync/types";

const PERF_BENCH_WORKER_URL = new URL("../../../workers/perf-worker.js", import.meta.url);

function createWorldUiRuntime(context: RuntimeContext, playbackControllerRef: { current: any }) {
    const worldUiController = createWorldUiController({
        cameraController: context.cameraController,
        terrainRenderer: context.terrainRenderer,
        wireframe: context.wireframe,
        plateHover: context.plateHover,
        debugToggleInput: context.debugToggleInput,
        statusEraLabel: context.statusEraLabel,
        eraScaleSelect: context.eraScaleSelect,
        eraScaleTickLabel: context.eraScaleTickLabel,
        eraScaleWeightFields: context.eraScaleWeightFields,
        getEraScalePreset,
        createEraMetrics,
        renderEraScaleControls,
        worldState: context.worldState,
        defaultEraScale: DEFAULT_ERA_SCALE,
        getState: context.getState,
        setState: context.setState,
        setStatus: context.setStatus,
        appendPlaybackEvent: (...args: any[]) => {
            playbackControllerRef.current?.appendPlaybackEvent(...args);
        },
    });

    const {
        setSurfaceMode,
        setDebugModeEnabled,
        setEraScale,
    } = worldUiController;

    return {
        setDebugModeEnabled,
        setEraScale,
        setSurfaceModeWithPinchReset: (nextMode: string) => {
            context.globePinchFocusController.reset();
            setSurfaceMode(nextMode);
        },
    };
}

function createWorldSessionRuntime(context: RuntimeContext, playbackControllerRef: { current: any }, setEraScale: (era: string) => void) {
    return createWorldSessionController({
        worldSimController: context.worldSimController,
        world: context.world,
        terrainRenderer: context.terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldFromController,
        refreshWorldStatsFromController,
        setCurrentTerrainData: (core: CoreBuffers) => {
            context.setState({ currentTerrainData: core });
        },
        syncClimateUi: context.syncClimateUi,
        hidePlateHover: () => {
            context.plateHover.hidePopup();
        },
        syncAfterWorldSync: () => {
            playbackControllerRef.current?.syncAfterWorldSync();
        },
        getCurrentSeed: () => context.getState().currentSeed,
        getCurrentSurfaceMode: () => context.getState().currentSurfaceMode,
        getActiveWorldId: () => context.getState().activeWorldId,
        statFields: context.statFields,
        level: LEVEL,
    });
}

function createWorldStepperRuntime(context: RuntimeContext, playbackControllerRef: { current: any }, setEraScale: (era: string) => void, refreshWorldStats: () => boolean) {
    return createWorldStepper({
        worldSimController: context.worldSimController,
        world: context.world,
        worldState: context.worldState,
        terrainRenderer: context.terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldDeltaFromController,
        syncVisibleCoreFieldsFromController,
        getDeltaFieldKindsForView,
        refreshWorldStats,
        syncClimateUi: context.syncClimateUi,
        syncAfterWorldStep: () => {
            playbackControllerRef.current?.syncAfterWorldStep();
        },
        setStatus: context.setStatus,
        getCurrentState: context.getState,
        pushStepBreakdownSamples,
        getEraScalePreset,
    });
}

function createViewModeRuntime(context: RuntimeContext, syncVisibleFieldsForCurrentView: () => void) {
    return createViewModeController({
        viewModeInputs: context.viewModeInputs,
        normalizeCellMetric,
        terrainRenderer: context.terrainRenderer,
        plateHover: context.plateHover,
        syncClimateUi: context.syncClimateUi,
        syncVisibleFieldsForCurrentView,
        getCurrentViewMode: () => context.getState().currentViewMode,
        getCurrentCellMetric: () => context.getState().currentCellMetric,
        getDebugEnabled: () => context.getState().debugEnabled,
        setCurrentViewMode: (nextMode: string) => {
            context.setState({ currentViewMode: nextMode });
        },
        setCurrentCellMetric: (nextMetric: string) => {
            context.setState({ currentCellMetric: nextMetric });
        },
    });
}

function createTerrainGenerationRuntime(context: RuntimeContext, playbackControllerRef: { current: any }, syncWorldFromActiveController: () => any) {
    return createTerrainGenerationController({
        seedForm: context.seedForm,
        seedInput: context.seedInput,
        worldSimController: context.worldSimController,
        level: LEVEL,
        terrainParams: TERRAIN_PARAMS,
        world: context.world,
        worldState: context.worldState,
        createEmptyLayers,
        createInitialBudgets,
        createEraMetrics,
        resetWorldProgress,
        getEraScalePreset,
        setStatus: context.setStatus,
        syncWorldFromActiveController,
        getCurrentEraScale: () => context.getState().currentEraScale,
        getCurrentSeed: () => context.getState().currentSeed,
        setCurrentState: context.setState,
        setPlaybackRunning: (isPlaying: boolean) => {
            playbackControllerRef.current?.setPlaybackRunning(isPlaying);
        },
        appendPlaybackEvent: (...args: any[]) => {
            playbackControllerRef.current?.appendPlaybackEvent(...args);
        },
        onInitWorldStart: async () => {
            context.loadingOverlayController.setWorldInitializing(true);
            context.loadingOverlayController.render();
            await context.renderInitializationFrames(context.renderFrame);
        },
        onInitWorldEnd: () => {
            context.loadingOverlayController.setWorldInitializing(false);
            context.loadingOverlayController.clear();
        },
    });
}

function createPlaybackRuntime(context: RuntimeContext, syncWorldFromActiveController: () => any, stepWorldTick: (perfRecorder?: any) => any) {
    return createPlaybackController({
        playbackControls: context.playbackControls,
        eventLogList: context.eventLogList,
        playbackState: context.worldState.playback,
        worldState: context.worldState,
        worldSimController: context.worldSimController,
        getActiveWorldId: () => context.getState().activeWorldId,
        getCurrentTerrainData: () => context.getState().currentTerrainData,
        getWorldTick: () => context.world.tick,
        syncWorldFromActiveController,
        stepWorldTick,
        setStatus: context.setStatus,
    });
}

function createPerfControllers(context: RuntimeContext, playbackControllerRef: { current: any }) {
    const { perfUiEnabled, perfBenchmarkController } = createPerfRuntime({
        isPerfEnabled: context.isPerfEnabled,
        perfControls: context.perfControls,
        perfStatFields: context.perfStatFields,
        workerUrl: PERF_BENCH_WORKER_URL,
        terrainParams: TERRAIN_PARAMS,
        level: LEVEL,
        createPerfProfile,
        createPerfConsoleTable,
        formatPerfSummaryLine,
        getRuntimeMeta: () => ({
            user_agent: navigator.userAgent,
            timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        }),
        canRunBenchmark: () => {
            const state = context.getState();
            return Boolean(state.activeWorldId && state.currentTerrainData);
        },
        setPlaybackRunning: (nextPlaying: boolean) => {
            const wasPlaying = context.worldState.playback.isPlaying;
            playbackControllerRef.current?.setPlaybackRunning(nextPlaying);
            return wasPlaying;
        },
        syncPlaybackUi: () => {
            playbackControllerRef.current?.syncAfterWorldSync();
        },
    });

    return {
        perfUiEnabled,
        runPerf: perfBenchmarkController.runBenchmark,
        copyPerfResult: perfBenchmarkController.copyResult,
        getLastPerfResult: () => perfBenchmarkController.getLastResult(),
    };
}

export function createRuntimeControllers(context: RuntimeContext) {
    const playbackControllerRef: { current: any } = { current: null };
    const {
        setDebugModeEnabled,
        setEraScale,
        setSurfaceModeWithPinchReset,
    } = createWorldUiRuntime(context, playbackControllerRef);

    const {
        syncWorldFromActiveController,
        refreshActiveWorldStats,
    } = createWorldSessionRuntime(context, playbackControllerRef, setEraScale);

    const {
        syncVisibleFieldsForCurrentView,
        stepWorldTick,
        stepWorldPlayback,
    } = createWorldStepperRuntime(context, playbackControllerRef, setEraScale, refreshActiveWorldStats);

    const { setViewMode, setCellMetric } = createViewModeRuntime(context, syncVisibleFieldsForCurrentView);

    const { updateTerrain } = createTerrainGenerationRuntime(
        context,
        playbackControllerRef,
        syncWorldFromActiveController,
    );

    const playbackController = createPlaybackRuntime(context, syncWorldFromActiveController, stepWorldTick);
    playbackControllerRef.current = playbackController;

    const {
        perfUiEnabled,
        runPerf,
        copyPerfResult,
        getLastPerfResult,
    } = createPerfControllers(context, playbackControllerRef);

    return {
        perfUiEnabled,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setCellMetric,
        setSurfaceModeWithPinchReset,
        stepWorldTick,
        stepWorldPlayback,
        updateTerrain,
        playbackController,
        runPerf,
        copyPerfResult,
        getLastPerfResult,
    };
}
