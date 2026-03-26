import { WorldSimController } from "../../interface/wasm.js";
import { GEOLOGY_PARAMS } from "../../interface/params/geology.js";
import { DEFAULT_ERA_SCALE, LEVEL } from "../../core/constants.js";
import {
    createEraMetrics,
    buildEraMetricsFromRuntime,
    getEraScalePreset,
    renderEraScaleControls,
} from "../era-presets.js";
import {
    createEmptyLayers,
    createInitialBudgets,
} from "../runtime/state.js";
import {
    getDeltaFieldKindsForView,
} from "../world-sync/view-mode.js";
import { refreshWorldStatsFromController } from "../world-sync/stats-sync.js";
import { syncVisibleCoreFieldsFromController } from "../world-sync/field-io.js";
import {
    syncWorldDeltaFromController,
    syncWorldFromController,
} from "../world-sync/world-state-sync.js";
import { createWorldUiController } from "../world-ui-controller.js";
import { createWorldSessionController } from "../world-session-controller.js";
import { createWorldStepper } from "../world-stepper.js";
import {
    createBenchmarkConsoleTable,
    createBenchmarkProfile,
    formatBenchmarkSummaryLine,
} from "../perf-benchmark.js";
import { createViewModeController } from "../view-mode-controller.js";
import { normalizeCellMetric } from "../cell-metric.js";
import { createTerrainGenerationController } from "../terrain-generation-controller.js";
import { createPlaybackController } from "../playback-controller.js";
import { pushStepBreakdownSamples } from "../perf-step-breakdown.js";
import { resetWorldProgress } from "../world-loop.js";
import { runInitialWorldAndUiSync } from "./post-init-sync.js";
import { createPerfRuntime } from "./perf-runtime.js";

const PERF_BENCH_WORKER_URL = new URL("../../workers/perf-benchmark-worker.js", import.meta.url);

function createRuntimeContext(options = {}) {
    const {
        elements,
        isPerfEnabled,
        setStatus,
        world,
        worldState,
        getState,
        setState,
        getCurrentEraMetrics,
        cameraController,
        terrainRenderer,
        wireframe,
        plateHover,
        globePinchFocusController,
        loadingOverlayController,
        syncClimateUi,
        renderFrame,
        renderInitializationFrames,
    } = options;

    const {
        seedForm,
        seedInput,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        viewModeInputs,
        statFields,
        statusEraLabel,
        playbackControls,
        eventLogList,
        perfControls,
        perfStatFields,
        viewportPanel,
    } = elements;

    return {
        elements,
        isPerfEnabled,
        setStatus,
        world,
        worldState,
        getState,
        setState,
        getCurrentEraMetrics,
        cameraController,
        terrainRenderer,
        wireframe,
        plateHover,
        globePinchFocusController,
        loadingOverlayController,
        syncClimateUi,
        renderFrame,
        renderInitializationFrames,
        seedForm,
        seedInput,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        viewModeInputs,
        statFields,
        statusEraLabel,
        playbackControls,
        eventLogList,
        perfControls,
        perfStatFields,
        viewportPanel,
    };
}

function createWorldUiRuntime(context, playbackControllerRef) {
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
        appendPlaybackEvent: (...args) => {
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
        setSurfaceModeWithPinchReset: (nextMode) => {
            context.globePinchFocusController.reset();
            setSurfaceMode(nextMode);
        },
    };
}

function createWorldSessionRuntime(context, playbackControllerRef, setEraScale) {
    return createWorldSessionController({
        worldSimController: context.worldSimController,
        world: context.world,
        terrainRenderer: context.terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldFromController,
        refreshWorldStatsFromController,
        setCurrentTerrainData: (core) => {
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

function createWorldStepperRuntime(context, playbackControllerRef, setEraScale, refreshWorldStats) {
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

function createViewModeRuntime(context, syncVisibleFieldsForCurrentView) {
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
        setCurrentViewMode: (nextMode) => {
            context.setState({ currentViewMode: nextMode });
        },
        setCurrentCellMetric: (nextMetric) => {
            context.setState({ currentCellMetric: nextMetric });
        },
    });
}

function createTerrainGenerationRuntime(context, playbackControllerRef, syncWorldFromActiveController) {
    return createTerrainGenerationController({
        seedForm: context.seedForm,
        seedInput: context.seedInput,
        worldSimController: context.worldSimController,
        level: LEVEL,
        terrainParams: GEOLOGY_PARAMS,
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
        setPlaybackRunning: (isPlaying) => {
            playbackControllerRef.current?.setPlaybackRunning(isPlaying);
        },
        appendPlaybackEvent: (...args) => {
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

function createPlaybackRuntime(context, syncWorldFromActiveController, stepWorldTick) {
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

function createPerfControllers(context, playbackControllerRef) {
    const { perfUiEnabled, perfBenchmarkController } = createPerfRuntime({
        isPerfEnabled: context.isPerfEnabled,
        perfControls: context.perfControls,
        perfStatFields: context.perfStatFields,
        workerUrl: PERF_BENCH_WORKER_URL,
        terrainParams: GEOLOGY_PARAMS,
        level: LEVEL,
        createBenchmarkProfile,
        createBenchmarkConsoleTable,
        formatBenchmarkSummaryLine,
        getRuntimeMeta: () => ({
            user_agent: navigator.userAgent,
            timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        }),
        canRunBenchmark: () => {
            const state = context.getState();
            return Boolean(state.activeWorldId && state.currentTerrainData);
        },
        setPlaybackRunning: (nextPlaying) => {
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
        runPerfBenchmark: perfBenchmarkController.runBenchmark,
        copyPerfBenchmarkResult: perfBenchmarkController.copyResult,
        getLastPerfBenchmarkResult: () => perfBenchmarkController.getLastResult(),
    };
}

function createRuntimeActions(context, runtimeControllers) {
    const {
        updateTerrain,
        setEraScale,
        playbackController,
    } = runtimeControllers;

    async function runInitialSync() {
        await runInitialWorldAndUiSync({
            updateTerrain,
            defaultTerrainSeed: context.getState().currentSeed,
            eraScaleSelect: context.eraScaleSelect,
            eraScaleTickLabel: context.eraScaleTickLabel,
            eraScaleWeightFields: context.eraScaleWeightFields,
            currentEraScale: DEFAULT_ERA_SCALE,
            currentEraMetrics: context.getCurrentEraMetrics(),
            setEraScale,
            syncClimateUi: context.syncClimateUi,
            playbackController,
            viewportPanel: context.viewportPanel,
            onResize: () => {
                context.cameraController.onResize();
                context.loadingOverlayController.render();
            },
            plateHover: context.plateHover,
        });
    }

    function shouldAdvanceWorld() {
        const state = context.getState();
        return context.worldState.playback.isPlaying && Boolean(state.currentTerrainData) && Boolean(state.activeWorldId);
    }

    return {
        runInitialSync,
        shouldAdvanceWorld,
    };
}

export function createControllerRuntime(options = {}) {
    const context = createRuntimeContext(options);
    context.worldSimController = new WorldSimController();

    const playbackControllerRef = { current: null };
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
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        getLastPerfBenchmarkResult,
    } = createPerfControllers(context, playbackControllerRef);

    const { runInitialSync, shouldAdvanceWorld } = createRuntimeActions(context, {
        updateTerrain,
        setEraScale,
        playbackController,
    });

    return {
        perfUiEnabled,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setCellMetric,
        setSurfaceModeWithPinchReset,
        stepWorldTick,
        updateTerrain,
        playbackController,
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        runInitialSync,
        shouldAdvanceWorld,
        getLastPerfBenchmarkResult,
    };
}
