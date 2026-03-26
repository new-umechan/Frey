import { WorldSimController } from "../interface/wasm.js";
import { GEOLOGY_PARAMS } from "../interface/params/geology.js";
import { createGlobeScene, resizeViewport } from "../gfx/scene.js";
import { createCameraController } from "../gfx/views/camera-controller.js";
import { createGlobePinchFocusController } from "../gfx/views/globe-pinch-focus-controller.js";
import { buildRenderPositions } from "../gfx/views/terrain-visuals.js";
import { buildRiverMaskTexture } from "../gfx/materials/river-mask.js";
import {
    DEFAULT_ERA_SCALE,
    DEFAULT_VIEW_MODE,
    LEVEL,
} from "../core/constants.js";
import {
    buildEraMetricsFromRuntime,
    createEraMetrics,
    getEraScalePreset,
    renderEraScaleControls,
} from "./era-presets.js";
import {
    createEmptyLayers,
    createInitialBudgets,
} from "./runtime/state.js";
import {
    getDeltaFieldKindsForView,
    refreshWorldStatsFromController,
    syncVisibleCoreFieldsFromController,
    syncWorldDeltaFromController,
    syncWorldFromController,
} from "./world-sync.js";
import { createTerrainRenderer } from "./terrain-renderer.js";
import { createClimateUiController } from "./climate-ui-controller.js";
import { createPlateHover } from "./plate-hover.js";
import { createLoadingOverlayController } from "./bootstrap/loading-overlay.js";
import { setupTerrainGeometryAttributes } from "./bootstrap/terrain-geometry-setup.js";
import { runInitialWorldAndUiSync } from "./bootstrap/post-init-sync.js";
import { createWorldUiController } from "./world-ui-controller.js";
import { createWorldSessionController } from "./world-session-controller.js";
import { createWorldStepper } from "./world-stepper.js";
import { createPerfBenchmarkController } from "./perf-benchmark-controller.js";
import {
    createBenchmarkConsoleTable,
    createBenchmarkProfile,
    formatBenchmarkSummaryLine,
} from "./perf-benchmark.js";
import { createViewModeController } from "./view-mode-controller.js";
import { normalizeCellMetric } from "./cell-metric.js";
import { createTerrainGenerationController } from "./terrain-generation-controller.js";
import { createPlaybackController } from "./playback-controller.js";
import { pushStepBreakdownSamples } from "./perf-step-breakdown.js";
import { bindAppUiControls } from "./ui-bindings.js";
import { createWorldState, createMutableStateStore } from "./app-state.js";
import { resetWorldProgress } from "./world-loop.js";

const PERF_BENCH_WORKER_URL = new URL("../workers/perf-benchmark-worker.js", import.meta.url);

function renderOnNextAnimationFrame(renderFrame) {
    return new Promise((resolve) => {
        window.requestAnimationFrame(() => {
            renderFrame();
            resolve();
        });
    });
}

async function renderInitializationFrames(renderFrame, frameCount = 2) {
    for (let i = 0; i < frameCount; i += 1) {
        await renderOnNextAnimationFrame(renderFrame);
    }
}

export function bootstrapAppRuntime(options = {}) {
    const {
        elements,
        isPerfEnabled,
        setStatus,
        basePositions,
        indices,
    } = options;
    const {
        canvas,
        loadingOverlayCanvas,
        viewportPanel,
        seedForm,
        seedInput,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        viewModeInputs,
        climateLegend,
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfControls,
        perfStatFields,
        statFields,
        statusEraLabel,
        plateHoverPopup,
    } = elements;

    let currentEraMetrics = createEraMetrics(DEFAULT_ERA_SCALE);
    const { world, worldState } = createWorldState({
        basePositions,
        indices,
        currentEraMetrics,
    });
    const mutableStateStore = createMutableStateStore({
        currentEraMetrics,
        debugEnabled: debugToggleInput.checked,
        worldTick: () => world.tick,
    });
    mutableStateStore.setState({ currentTerrainData: world.core });
    const { getState, setState } = mutableStateStore;
    const playbackState = worldState.playback;

    const {
        scene,
        globeCamera,
        mapCamera,
        renderer,
        globeControls,
        mapControls,
        geometry,
        sphere,
        wireframe,
        halo,
        terrainMaterial,
    } = createGlobeScene(canvas, indices);

    const cameraController = createCameraController({
        globeCamera,
        mapCamera,
        globeControls,
        mapControls,
        sphere,
        wireframe,
        halo,
        resizeViewport,
        viewportPanel,
        renderer,
        isDebugEnabled: () => getState().debugEnabled,
    });

    setupTerrainGeometryAttributes({
        geometry,
        terrainMaterial,
        basePositions,
        currentViewMode: DEFAULT_VIEW_MODE,
        currentCellMetric: getState().currentCellMetric,
        debugEnabled: getState().debugEnabled,
    });

    const terrainRenderer = createTerrainRenderer({
        geometry,
        terrainMaterial,
        basePositions,
        buildRenderPositions,
        buildRiverMaskTexture,
    });
    const climateUiController = createClimateUiController({
        climateLegend,
        getCurrentViewMode: () => getState().currentViewMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        getCurrentTerrainData: () => getState().currentTerrainData,
    });
    const { syncClimateUi, updateClimateHoverReadout } = climateUiController;

    const plateHover = createPlateHover({
        canvas,
        sphere,
        geometry,
        viewportPanel,
        plateHoverPopup,
        getState: () => ({
            ...getState(),
            camera: cameraController.getCamera(),
        }),
        onClimateHover: updateClimateHoverReadout,
    });
    const globePinchFocusController = createGlobePinchFocusController({
        canvas,
        sphere,
        globeCamera,
        globeControls,
        getCurrentSurfaceMode: () => getState().currentSurfaceMode,
    });
    const loadingOverlayController = createLoadingOverlayController({
        loadingOverlayCanvas,
        viewportPanel,
        sphere,
        getCamera: () => cameraController.getCamera(),
    });

    function renderFrame() {
        globePinchFocusController.update();
        cameraController.getActiveControls().update();
        renderer.render(scene, cameraController.getCamera());
        loadingOverlayController.render();
    }

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
        setState: (patch = {}) => {
            setState(patch);
            if (patch.currentEraMetrics) {
                currentEraMetrics = patch.currentEraMetrics;
            }
        },
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
    function getLastPerfBenchmarkResult() {
        return perfBenchmarkController.getLastResult();
    }

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

    function onResize() {
        cameraController.onResize();
        loadingOverlayController.render();
    }

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
        setCurrentState: (patch = {}) => {
            setState(patch);
            if (patch.currentEraMetrics) {
                currentEraMetrics = patch.currentEraMetrics;
            }
        },
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

    bindAppUiControls({
        canvas,
        viewportPanel,
        sidebarToggle: elements.sidebarToggle,
        debugToggleInput,
        eraScaleSelect,
        viewModeInputs,
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfEnabled: perfUiEnabled,
        perfControls,
        seedForm,
        seedInput,
        onResize,
        setSidebarOpen: elements.setSidebarOpen,
        plateHover,
        globePinchFocusController,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setCellMetric,
        setSurfaceMode: setSurfaceModeWithPinchReset,
        playbackController,
        runPerfBenchmark: perfBenchmarkController.runBenchmark,
        copyPerfBenchmarkResult: perfBenchmarkController.copyResult,
        getDebugEnabled: () => getState().debugEnabled,
        getCurrentSurfaceMode: () => getState().currentSurfaceMode,
        getCurrentViewMode: () => getState().currentViewMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        updateTerrain,
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
            currentEraMetrics,
            setEraScale,
            syncClimateUi,
            playbackController,
            viewportPanel,
            onResize,
            plateHover,
        });
    }

    function shouldAdvanceWorld() {
        const state = getState();
        return playbackState.isPlaying && Boolean(state.currentTerrainData) && Boolean(state.activeWorldId);
    }

    return {
        renderFrame,
        worldState,
        stepWorldTick,
        runInitialSync,
        shouldAdvanceWorld,
        getLastPerfBenchmarkResult,
    };
}
