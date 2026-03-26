import { bindAppUiControls } from "./ui-bindings.js";
import { createEraMetrics } from "./era-presets.js";
import { createRuntimeStore } from "./bootstrap/runtime-store.js";
import { createSceneRuntime } from "./bootstrap/scene-runtime.js";
import { createControllerRuntime } from "./bootstrap/controller-runtime.js";
import { renderInitializationFrames } from "./bootstrap/initialization-frames.js";

export function bootstrapAppRuntime(options = {}) {
    const {
        elements,
        isPerfEnabled,
        setStatus,
        basePositions,
        indices,
    } = options;
    const {
        debugToggleInput,
        eraScaleSelect,
        viewModeInputs,
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfControls,
        seedForm,
        seedInput,
        canvas,
        viewportPanel,
        sidebarToggle,
    } = elements;

    const runtimeStore = createRuntimeStore({
        basePositions,
        indices,
        createEraMetrics,
        debugEnabled: debugToggleInput.checked,
    });
    const {
        world,
        worldState,
        getState,
        setState,
        getCurrentEraMetrics,
    } = runtimeStore;

    const sceneRuntime = createSceneRuntime({
        elements,
        indices,
        basePositions,
        getState,
    });
    const {
        cameraController,
        terrainRenderer,
        wireframe,
        plateHover,
        globePinchFocusController,
        loadingOverlayController,
        syncClimateUi,
        renderFrame,
        onResize,
    } = sceneRuntime;

    const controllerRuntime = createControllerRuntime({
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
    });
    const {
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
    } = controllerRuntime;

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
        setSidebarOpen: elements.setSidebarOpen,
        plateHover,
        globePinchFocusController,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setCellMetric,
        setSurfaceMode: setSurfaceModeWithPinchReset,
        playbackController,
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        getDebugEnabled: () => getState().debugEnabled,
        getCurrentSurfaceMode: () => getState().currentSurfaceMode,
        getCurrentViewMode: () => getState().currentViewMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        updateTerrain,
        setStatus,
    });

    return {
        renderFrame,
        worldState,
        stepWorldTick,
        runInitialSync,
        shouldAdvanceWorld,
        getLastPerfBenchmarkResult,
    };
}
