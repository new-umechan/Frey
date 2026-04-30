import { TERRAIN_PARAMS } from "../../../interface/params/terrain";
import { DEFAULT_ERA_SCALE, LEVEL } from "../../../shared/constants";
import {
    createEraMetrics,
    buildEraMetricsFromRuntime,
    getEraScalePreset,
    renderEraScaleControls,
} from "../../state/era-presets";
import {
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
    type TickPerfRecorder,
} from "../../perf/recorder";
import { createViewModeController } from "../../controllers/view-mode-controller";
import { normalizeCellMetric } from "../../visualizers/cell-metric";
import { createTerrainGenerationController } from "../../controllers/terrain-generation-controller";
import {
    createPlaybackController,
    type PlaybackController,
} from "../../playback/playback-controller";
import { pushStepBreakdownSamples } from "../../perf/perf-step-breakdown";
import { resetWorldProgress } from "../../sim/world-loop";
import { createPerfRuntime } from "../perf-runtime";
import { type RuntimeContext } from "./create-controller-runtime";
import { type CoreBuffers, type SyncWorldResult } from "../../sim/sync/types";
import { type MetricsResult } from "../../engine/engine-client";

const PERF_BENCH_WORKER_URL = new URL("../../../workers/perf-worker.js", import.meta.url);

type PlaybackRef = {
    current: PlaybackController | null;
};

function createWorldUiRuntime(context: RuntimeContext, playbackControllerRef: PlaybackRef) {
    const worldUiController = createWorldUiController({
        cameraController: context.scene.cameraController,
        terrainRenderer: context.scene.terrainRenderer,
        wireframe: context.scene.wireframe,
        plateHover: context.scene.plateHover,
        debugToggleInput: context.dom.debugToggleInput,
        statusEraLabel: context.dom.statusEraLabel,
        eraScaleSelect: context.dom.eraScaleSelect,
        eraScaleTickLabel: context.dom.eraScaleTickLabel,
        eraScaleWeightFields: context.dom.eraScaleWeightFields,
        getEraScalePreset,
        createEraMetrics,
        renderEraScaleControls,
        worldState: context.store.worldState,
        defaultEraScale: DEFAULT_ERA_SCALE,
        getState: context.store.getState,
        getCurrentTerrainData: context.store.getCurrentTerrainData,
        getActiveWorldId: context.store.getActiveWorldId,
        setState: context.store.setState,
        setCurrentEraMetrics: context.store.setCurrentEraMetrics,
        getWorldTick: () => context.store.world.tick,
        setStatus: context.setStatus,
        appendPlaybackEvent: (type: string, label: string, detail?: string) => {
            playbackControllerRef.current?.appendPlaybackEvent(type, label, detail);
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
            context.scene.globePinchFocusController.reset();
            setSurfaceMode(nextMode);
        },
    };
}

function createWorldSessionRuntime(context: RuntimeContext, playbackControllerRef: PlaybackRef, setEraScale: (era: string) => void) {
    return createWorldSessionController({
        engineClient: context.engineClient,
        world: context.store.world,
        terrainRenderer: context.scene.terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldFromController,
        refreshWorldStatsFromController,
        setCurrentTerrainData: (core: CoreBuffers) => {
            context.store.setCurrentTerrainData(core);
        },
        syncClimateUi: context.scene.syncClimateUi,
        hidePlateHover: () => {
            context.scene.plateHover.hidePopup();
        },
        syncAfterWorldSync: () => {
            playbackControllerRef.current?.syncAfterWorldSync();
        },
        getCurrentSeed: () => context.store.getState().currentSeed,
        getCurrentSurfaceMode: () => context.store.getState().currentSurfaceMode,
        getActiveWorldId: context.store.getActiveWorldId,
        statFields: context.dom.statFields,
        level: LEVEL,
    });
}

function createWorldStepperRuntime(
    context: RuntimeContext,
    playbackControllerRef: PlaybackRef,
    setEraScale: (era: string) => void,
    refreshWorldStats: () => Promise<MetricsResult | null>,
) {
    return createWorldStepper({
        engineClient: context.engineClient,
        world: context.store.world,
        worldState: context.store.worldState,
        terrainRenderer: context.scene.terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldDeltaFromController,
        syncVisibleCoreFieldsFromController,
        getDeltaFieldKindsForView,
        refreshWorldStats,
        syncClimateUi: context.scene.syncClimateUi,
        syncAfterWorldStep: (options) => {
            playbackControllerRef.current?.syncAfterWorldStep(options);
        },
        setStatus: context.setStatus,
        getCurrentState: context.store.getState,
        getCurrentEraMetrics: context.store.getCurrentEraMetrics,
        getActiveWorldId: context.store.getActiveWorldId,
        getCurrentTerrainData: context.store.getCurrentTerrainData,
        pushStepBreakdownSamples,
        getEraScalePreset,
    });
}

function createViewModeRuntime(context: RuntimeContext, syncVisibleFieldsForCurrentView: () => void) {
    return createViewModeController({
        viewModeInputs: context.dom.viewModeInputs,
        normalizeCellMetric,
        terrainRenderer: context.scene.terrainRenderer,
        plateHover: context.scene.plateHover,
        syncClimateUi: context.scene.syncClimateUi,
        syncVisibleFieldsForCurrentView,
        getCurrentViewMode: () => context.store.getState().currentViewMode,
        getCurrentCellMetric: () => context.store.getState().currentCellMetric,
        getCurrentTerrainData: context.store.getCurrentTerrainData,
        getCurrentSurfaceMode: () => context.store.getState().currentSurfaceMode,
        getWorldTick: () => context.store.world.tick,
        getDebugEnabled: () => context.store.getState().debugEnabled,
        setCurrentViewMode: (nextMode: string) => {
            context.store.setState({ currentViewMode: nextMode });
        },
        setCurrentCellMetric: (nextMetric: string) => {
            context.store.setState({ currentCellMetric: nextMetric });
        },
    });
}

function createTerrainGenerationRuntime(
    context: RuntimeContext,
    playbackControllerRef: PlaybackRef,
    syncWorldFromActiveController: () => Promise<SyncWorldResult | null>,
) {
    return createTerrainGenerationController({
        seedForm: context.dom.seedForm,
        seedInput: context.dom.seedInput,
        engineClient: context.engineClient,
        level: LEVEL,
        terrainParams: TERRAIN_PARAMS,
        world: context.store.world,
        worldState: context.store.worldState,
        createInitialBudgets,
        createEraMetrics,
        resetWorldProgress,
        getEraScalePreset,
        setStatus: context.setStatus,
        syncWorldFromActiveController,
        getCurrentEraScale: () => context.store.getState().currentEraScale,
        getCurrentSeed: () => context.store.getState().currentSeed,
        setActiveWorldId: context.store.setActiveWorldId,
        setCurrentState: context.store.setState,
        setCurrentEraMetrics: context.store.setCurrentEraMetrics,
        setPlaybackRunning: (isPlaying: boolean) => {
            playbackControllerRef.current?.setPlaybackRunning(isPlaying);
        },
        appendPlaybackEvent: (type: string, label: string, detail?: string) => {
            playbackControllerRef.current?.appendPlaybackEvent(type, label, detail);
        },
        onInitWorldStart: async () => {
            context.scene.loadingOverlayController.setWorldInitializing(true);
            context.scene.loadingOverlayController.render();
            await context.renderInitializationFrames(context.scene.renderFrame);
        },
        onInitWorldEnd: () => {
            context.scene.loadingOverlayController.setWorldInitializing(false);
            context.scene.loadingOverlayController.clear();
        },
    });
}

function createPlaybackRuntime(
    context: RuntimeContext,
    syncWorldFromActiveController: () => Promise<SyncWorldResult | null>,
    stepWorldTick: (perfRecorder?: TickPerfRecorder | null) => Promise<boolean>,
) {
    return createPlaybackController({
        playbackControls: context.dom.playbackControls,
        eventLogList: context.dom.eventLogList,
        playbackState: context.store.worldState.playback,
        worldState: context.store.worldState,
        engineClient: context.engineClient,
        getActiveWorldId: context.store.getActiveWorldId,
        getCurrentTerrainData: context.store.getCurrentTerrainData,
        getWorldTick: () => context.store.world.tick,
        syncWorldFromActiveController,
        stepWorldTick,
        setStatus: context.setStatus,
    });
}

function createPerfControllers(context: RuntimeContext, playbackControllerRef: PlaybackRef) {
    const { perfUiEnabled, perfBenchmarkController } = createPerfRuntime({
        isPerfEnabled: context.isPerfEnabled,
        perfControls: context.dom.perfControls,
        perfStatFields: context.dom.perfStatFields,
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
            return Boolean(context.store.getActiveWorldId() && context.store.getCurrentTerrainData());
        },
        setPlaybackRunning: (nextPlaying: boolean) => {
            const wasPlaying = context.store.worldState.playback.isPlaying;
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
    const playbackControllerRef: PlaybackRef = { current: null };
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
