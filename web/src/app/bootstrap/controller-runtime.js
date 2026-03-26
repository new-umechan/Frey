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
import { createPerfBenchmarkController } from "../perf-benchmark-controller.js";
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

const PERF_BENCH_WORKER_URL = new URL("../../workers/perf-benchmark-worker.js", import.meta.url);

export function createControllerRuntime(options = {}) {
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

    const playbackState = worldState.playback;
    const worldSimController = new WorldSimController();
    let playbackController = null;

    const worldUiController = createWorldUiController({
        cameraController,
        terrainRenderer,
        wireframe,
        plateHover,
        debugToggleInput,
        statusEraLabel,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        getEraScalePreset,
        createEraMetrics,
        renderEraScaleControls,
        worldState,
        defaultEraScale: DEFAULT_ERA_SCALE,
        getState,
        setState,
        setStatus,
        appendPlaybackEvent: (...args) => {
            playbackController?.appendPlaybackEvent(...args);
        },
    });
    const { setSurfaceMode, setDebugModeEnabled, setEraScale } = worldUiController;
    const setSurfaceModeWithPinchReset = (nextMode) => {
        globePinchFocusController.reset();
        setSurfaceMode(nextMode);
    };

    const worldSessionController = createWorldSessionController({
        worldSimController,
        world,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldFromController,
        refreshWorldStatsFromController,
        setCurrentTerrainData: (core) => {
            setState({ currentTerrainData: core });
        },
        syncClimateUi,
        hidePlateHover: () => {
            plateHover.hidePopup();
        },
        syncAfterWorldSync: () => {
            playbackController.syncAfterWorldSync();
        },
        getCurrentSeed: () => getState().currentSeed,
        getCurrentSurfaceMode: () => getState().currentSurfaceMode,
        getActiveWorldId: () => getState().activeWorldId,
        statFields,
        level: LEVEL,
    });
    const { syncWorldFromActiveController, refreshActiveWorldStats } = worldSessionController;

    const worldStepper = createWorldStepper({
        worldSimController,
        world,
        worldState,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldDeltaFromController,
        syncVisibleCoreFieldsFromController,
        getDeltaFieldKindsForView,
        refreshWorldStats: refreshActiveWorldStats,
        syncClimateUi,
        syncAfterWorldStep: () => {
            playbackController.syncAfterWorldStep();
        },
        setStatus,
        getCurrentState: getState,
        pushStepBreakdownSamples,
        getEraScalePreset,
    });
    const { syncVisibleFieldsForCurrentView, stepWorldTick } = worldStepper;

    const perfUiEnabled = isPerfEnabled && Boolean(perfControls);
    const perfBenchmarkController = createPerfBenchmarkController({
        enabled: perfUiEnabled,
        controls: perfControls,
        perfStatFields,
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
            const state = getState();
            return Boolean(state.activeWorldId && state.currentTerrainData);
        },
        setPlaybackRunning: (nextPlaying) => {
            const wasPlaying = playbackState.isPlaying;
            playbackController.setPlaybackRunning(nextPlaying);
            return wasPlaying;
        },
        syncPlaybackUi: () => {
            playbackController.syncAfterWorldSync();
        },
    });
    perfBenchmarkController.initialize();

    const viewModeController = createViewModeController({
        viewModeInputs,
        normalizeCellMetric,
        terrainRenderer,
        plateHover,
        syncClimateUi,
        syncVisibleFieldsForCurrentView,
        getCurrentViewMode: () => getState().currentViewMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        getDebugEnabled: () => getState().debugEnabled,
        setCurrentViewMode: (nextMode) => {
            setState({ currentViewMode: nextMode });
        },
        setCurrentCellMetric: (nextMetric) => {
            setState({ currentCellMetric: nextMetric });
        },
    });
    const { setViewMode, setCellMetric } = viewModeController;

    const terrainGenerationController = createTerrainGenerationController({
        seedForm,
        seedInput,
        worldSimController,
        level: LEVEL,
        terrainParams: GEOLOGY_PARAMS,
        world,
        worldState,
        createEmptyLayers,
        createInitialBudgets,
        createEraMetrics,
        resetWorldProgress,
        getEraScalePreset,
        setStatus,
        syncWorldFromActiveController,
        getCurrentEraScale: () => getState().currentEraScale,
        getCurrentSeed: () => getState().currentSeed,
        setCurrentState: setState,
        setPlaybackRunning: (isPlaying) => {
            playbackController.setPlaybackRunning(isPlaying);
        },
        appendPlaybackEvent: (...args) => {
            playbackController.appendPlaybackEvent(...args);
        },
        onInitWorldStart: async () => {
            loadingOverlayController.setWorldInitializing(true);
            loadingOverlayController.render();
            await renderInitializationFrames(renderFrame);
        },
        onInitWorldEnd: () => {
            loadingOverlayController.setWorldInitializing(false);
            loadingOverlayController.clear();
        },
    });
    const { updateTerrain } = terrainGenerationController;

    playbackController = createPlaybackController({
        playbackControls,
        eventLogList,
        playbackState,
        worldState,
        worldSimController,
        getActiveWorldId: () => getState().activeWorldId,
        getCurrentTerrainData: () => getState().currentTerrainData,
        getWorldTick: () => world.tick,
        syncWorldFromActiveController,
        stepWorldTick,
        setStatus,
    });

    async function runInitialSync() {
        await runInitialWorldAndUiSync({
            updateTerrain,
            defaultTerrainSeed: getState().currentSeed,
            eraScaleSelect,
            eraScaleTickLabel,
            eraScaleWeightFields,
            currentEraScale: DEFAULT_ERA_SCALE,
            currentEraMetrics: getCurrentEraMetrics(),
            setEraScale,
            syncClimateUi,
            playbackController,
            viewportPanel,
            onResize: () => {
                cameraController.onResize();
                loadingOverlayController.render();
            },
            plateHover,
        });
    }

    function shouldAdvanceWorld() {
        const state = getState();
        return playbackState.isPlaying && Boolean(state.currentTerrainData) && Boolean(state.activeWorldId);
    }

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
        runPerfBenchmark: perfBenchmarkController.runBenchmark,
        copyPerfBenchmarkResult: perfBenchmarkController.copyResult,
        runInitialSync,
        shouldAdvanceWorld,
        getLastPerfBenchmarkResult: () => perfBenchmarkController.getLastResult(),
    };
}
