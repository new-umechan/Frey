import initWasm, {
    WorldSimController,
    generate_mesh,
} from "../interface/wasm.js";
import { collectAppElements } from "../ui/dom.js";
import { createPlateHover } from "./plate-hover.js";
import { normalizeClimateMetric } from "./climate-metric.js";
import { createTerrainRenderer } from "./terrain-renderer.js";
import {
    createStatusController,
    isPerfFeatureEnabled,
    setPerfPanelVisibility,
} from "./bootstrap/status-ui.js";
import { setupTerrainGeometryAttributes } from "./bootstrap/terrain-geometry-setup.js";
import { runInitialWorldAndUiSync } from "./bootstrap/post-init-sync.js";
import { createGlobeScene, resizeViewport } from "../gfx/scene.js";
import { createCameraController } from "../gfx/views/camera-controller.js";
import { GEOLOGY_PARAMS } from "../interface/params/geology.js";
import { buildRenderPositions } from "../gfx/views/terrain-visuals.js";
import { buildRiverMaskTexture } from "../gfx/materials/river-mask.js";
import {
    buildEraMetricsFromRuntime,
    createEraMetrics,
    getEraScalePreset,
    renderEraScaleControls,
} from "./era-presets.js";
import {
    DEBUG_SNAPSHOT_TICKS,
    DEFAULT_CLIMATE_METRIC,
    DEFAULT_ERA_SCALE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
    LEVEL,
} from "../core/constants.js";
import {
    createEmptyCore,
    createEmptyLayers,
    createInitialBudgets,
    createInitialRuntimeState,
} from "./runtime/state.js";
import { saveDebugSnapshotIfNeeded } from "./debug/snapshot.js";
import {
    getDeltaFieldKindsForView,
    refreshWorldStatsFromController,
    syncVisibleCoreFieldsFromController,
    syncWorldDeltaFromController,
    syncWorldFromController,
} from "./world-sync.js";
import { advanceWorldLoop, resetWorldProgress } from "./world-loop.js";
import { createPlaybackController } from "./playback-controller.js";
import {
    createBenchmarkConsoleTable,
    createBenchmarkProfile,
    formatBenchmarkSummaryLine,
} from "./perf-benchmark.js";
import { createPerfBenchmarkController } from "./perf-benchmark-controller.js";
import { createClimateUiController } from "./climate-ui-controller.js";
import { pushStepBreakdownSamples } from "./perf-step-breakdown.js";
import { createWorldStepper } from "./world-stepper.js";
import { createViewModeController } from "./view-mode-controller.js";
import { createWorldUiController } from "./world-ui-controller.js";
import { createTerrainGenerationController } from "./terrain-generation-controller.js";
import { createWorldSessionController } from "./world-session-controller.js";
import { bindAppUiControls } from "./ui-bindings.js";

const PERF_BENCH_WORKER_URL = new URL("../workers/perf-benchmark-worker.js", import.meta.url);
export async function createApp() {
    const isPerfEnabled = isPerfFeatureEnabled();
    const {
        appShell,
        canvas,
        viewportPanel,
        seedForm,
        seedInput,
        sidebarToggle,
        statusMessage,
        statusEraLabel,
        plateHoverPopup,
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
        perfPanel,
        perfControls,
        perfStatFields,
        statFields,
    } = collectAppElements({ perfEnabled: isPerfEnabled });
    const statusRows = [statusEraLabel, eraScaleTickLabel];
    const { setStatus } = createStatusController(statusMessage, statusRows);

    function setSidebarOpen(isOpen) {
        if (!sidebarToggle) {
            return;
        }
        appShell.classList.toggle("is-sidebar-collapsed", !isOpen);
        sidebarToggle.setAttribute("aria-expanded", String(isOpen));
    }

    setPerfPanelVisibility(perfPanel, isPerfEnabled);
    if (sidebarToggle) {
        setSidebarOpen(true);
    }
    seedInput.value = DEFAULT_TERRAIN_SEED;
    setStatus("Loading WASM...");
    await initWasm();
    setStatus("Preparing mesh...");
    const mesh = generate_mesh(LEVEL);
    const basePositions = new Float32Array(mesh.positions);
    const indices = new Uint32Array(mesh.indices);

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
    let debugEnabled = debugToggleInput.checked;
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
        isDebugEnabled: () => debugEnabled,
    });

    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentClimateMetric = DEFAULT_CLIMATE_METRIC;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentEraScale = DEFAULT_ERA_SCALE;
    let currentEraMetrics = createEraMetrics(DEFAULT_ERA_SCALE);
    const worldSimController = new WorldSimController();
    let activeWorldId = null;
    const world = {
        tick: 0,
        era: DEFAULT_ERA_SCALE,
        mesh: {
            positions: basePositions,
            indices,
            nbrOffsets: null,
            nbrs: null,
        },
        core: createEmptyCore(),
        layers: createEmptyLayers(),
        budgets: createInitialBudgets(),
        runtime: createInitialRuntimeState(currentEraMetrics.runtimeTickMs),
    };
    let currentTerrainData = world.core;
    const worldState = world.runtime;
    const playbackState = worldState.playback;
    const debugSnapshotTickSet = new Set(DEBUG_SNAPSHOT_TICKS);
    const debugSnapshotSavedTicks = new Set();

    setupTerrainGeometryAttributes({
        geometry,
        terrainMaterial,
        basePositions,
        currentViewMode,
        currentClimateMetric,
        debugEnabled,
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
        getCurrentViewMode: () => currentViewMode,
        getCurrentClimateMetric: () => currentClimateMetric,
        getCurrentTerrainData: () => currentTerrainData,
    });
    const { syncClimateUi, updateClimateHoverReadout } = climateUiController;
    const plateHover = createPlateHover({
        canvas,
        sphere,
        geometry,
        viewportPanel,
        plateHoverPopup,
        getState: () => ({
            currentTerrainData,
            currentViewMode,
            currentClimateMetric,
            currentSurfaceMode,
            camera: cameraController.getCamera(),
            debugEnabled,
        }),
        onClimateHover: updateClimateHoverReadout,
    });

    let playbackController = null;
    const getMutableState = () => {
        return {
            activeWorldId,
            currentSeed,
            currentTerrainData,
            currentSurfaceMode,
            currentViewMode,
            currentClimateMetric,
            currentEraScale,
            currentEraMetrics,
            debugEnabled,
            worldTick: world.tick,
        };
    };
    const stateSetters = {
        activeWorldId: (value) => { activeWorldId = value; },
        currentSeed: (value) => { currentSeed = value; },
        currentEraScale: (value) => { currentEraScale = value; },
        currentEraMetrics: (value) => { currentEraMetrics = value; },
        currentSurfaceMode: (value) => { currentSurfaceMode = value; },
        debugEnabled: (value) => { debugEnabled = value; },
    };
    const setMutableState = (patch = {}) => {
        for (const [key, value] of Object.entries(patch)) {
            stateSetters[key]?.(value);
        }
    };
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
        getState: getMutableState,
        setState: setMutableState,
        setStatus,
        appendPlaybackEvent: (...args) => {
            playbackController?.appendPlaybackEvent(...args);
        },
    });
    const { setSurfaceMode, setDebugModeEnabled, setEraScale } = worldUiController;

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
            currentTerrainData = core;
        },
        syncClimateUi,
        hidePlateHover: () => {
            plateHover.hidePopup();
        },
        syncAfterWorldSync: () => {
            playbackController.syncAfterWorldSync();
        },
        getCurrentSeed: () => currentSeed,
        getCurrentSurfaceMode: () => currentSurfaceMode,
        getActiveWorldId: () => activeWorldId,
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
        saveDebugSnapshotIfNeeded,
        isDev: import.meta.env.DEV,
        debugSnapshotTickSet,
        debugSnapshotSavedTicks,
        syncClimateUi,
        syncAfterWorldStep: () => {
            playbackController.syncAfterWorldStep();
        },
        setStatus,
        getCurrentState: getMutableState,
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
        canRunBenchmark: () => Boolean(activeWorldId && currentTerrainData),
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
        normalizeClimateMetric,
        terrainRenderer,
        plateHover,
        syncClimateUi,
        syncVisibleFieldsForCurrentView,
        getCurrentViewMode: () => currentViewMode,
        getCurrentClimateMetric: () => currentClimateMetric,
        getDebugEnabled: () => debugEnabled,
        setCurrentViewMode: (nextMode) => {
            currentViewMode = nextMode;
        },
        setCurrentClimateMetric: (nextMetric) => {
            currentClimateMetric = nextMetric;
        },
    });
    const { setViewMode, setClimateMetric } = viewModeController;

    function onResize() {
        cameraController.onResize();
    }

    const terrainGenerationController = createTerrainGenerationController({
        seedForm,
        seedInput,
        worldSimController,
        level: LEVEL,
        terrainParams: GEOLOGY_PARAMS,
        world,
        worldState,
        debugSnapshotSavedTicks,
        createEmptyLayers,
        createInitialBudgets,
        createEraMetrics,
        resetWorldProgress,
        getEraScalePreset,
        setStatus,
        syncWorldFromActiveController,
        getCurrentEraScale: () => currentEraScale,
        getCurrentSeed: () => DEFAULT_TERRAIN_SEED,
        setCurrentState: setMutableState,
        setPlaybackRunning: (isPlaying) => {
            playbackController.setPlaybackRunning(isPlaying);
        },
        appendPlaybackEvent: (...args) => {
            playbackController.appendPlaybackEvent(...args);
        },
    });
    const { updateTerrain } = terrainGenerationController;

    playbackController = createPlaybackController({
        playbackControls,
        eventLogList,
        playbackState,
        worldState,
        worldSimController,
        getActiveWorldId: () => activeWorldId,
        getCurrentTerrainData: () => currentTerrainData,
        getWorldTick: () => world.tick,
        syncWorldFromActiveController,
        stepWorldTick,
        setStatus,
    });

    bindAppUiControls({
        canvas,
        viewportPanel,
        sidebarToggle,
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
        setSidebarOpen,
        plateHover,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setClimateMetric,
        setSurfaceMode,
        playbackController,
        runPerfBenchmark: perfBenchmarkController.runBenchmark,
        copyPerfBenchmarkResult: perfBenchmarkController.copyResult,
        getDebugEnabled: () => debugEnabled,
        getCurrentSurfaceMode: () => currentSurfaceMode,
        getCurrentViewMode: () => currentViewMode,
        getCurrentClimateMetric: () => currentClimateMetric,
        updateTerrain,
        setStatus,
    });

    await runInitialWorldAndUiSync({
        updateTerrain,
        defaultTerrainSeed: DEFAULT_TERRAIN_SEED,
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

    return {
        tick(nowMs) {
            advanceWorldLoop(
                nowMs,
                worldState,
                () => playbackState.isPlaying && Boolean(currentTerrainData) && Boolean(activeWorldId),
                stepWorldTick,
            );
            cameraController.getActiveControls().update();
            renderer.render(scene, cameraController.getCamera());
        },
        getLastPerfBenchmarkResult,
    };
}
