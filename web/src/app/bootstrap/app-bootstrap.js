import { bindAppUiControls } from "../ui/ui-bindings.js";
import { createEraMetrics } from "../core/era-presets.js";
import { createRuntimeStore } from "./runtime-store.js";
import { createSceneRuntime } from "./scene-runtime.js";
import { createControllerRuntime } from "./controller-runtime/create-controller-runtime.js";
import { renderInitializationFrames } from "./initialization-frames.js";

function createControllerDeps(options) {
    const {
        elements,
        isPerfEnabled,
        setStatus,
        runtimeStore,
        sceneRuntime,
    } = options;

    return {
        elements,
        isPerfEnabled,
        setStatus,
        world: runtimeStore.world,
        worldState: runtimeStore.worldState,
        getState: runtimeStore.getState,
        setState: runtimeStore.setState,
        getCurrentEraMetrics: runtimeStore.getCurrentEraMetrics,
        cameraController: sceneRuntime.cameraController,
        terrainRenderer: sceneRuntime.terrainRenderer,
        wireframe: sceneRuntime.wireframe,
        plateHover: sceneRuntime.plateHover,
        globePinchFocusController: sceneRuntime.globePinchFocusController,
        loadingOverlayController: sceneRuntime.loadingOverlayController,
        syncClimateUi: sceneRuntime.syncClimateUi,
        renderFrame: sceneRuntime.renderFrame,
        renderInitializationFrames,
    };
}

function bindRuntimeUi(options) {
    const {
        elements,
        sceneRuntime,
        controllerRuntime,
        getState,
        setStatus,
    } = options;

    bindAppUiControls({
        canvas: elements.canvas,
        viewportPanel: elements.viewportPanel,
        sidebarToggle: elements.sidebarToggle,
        debugToggleInput: elements.debugToggleInput,
        eraScaleSelect: elements.eraScaleSelect,
        viewModeInputs: elements.viewModeInputs,
        controlHelpModal: elements.controlHelpModal,
        controlHelpCloseButton: elements.controlHelpCloseButton,
        playbackControls: elements.playbackControls,
        eventLogList: elements.eventLogList,
        perfEnabled: controllerRuntime.perfUiEnabled,
        perfControls: elements.perfControls,
        seedForm: elements.seedForm,
        seedInput: elements.seedInput,
        onResize: sceneRuntime.onResize,
        setSidebarOpen: elements.setSidebarOpen,
        plateHover: sceneRuntime.plateHover,
        globePinchFocusController: sceneRuntime.globePinchFocusController,
        setDebugModeEnabled: controllerRuntime.setDebugModeEnabled,
        setEraScale: controllerRuntime.setEraScale,
        setViewMode: controllerRuntime.setViewMode,
        setCellMetric: controllerRuntime.setCellMetric,
        setSurfaceMode: controllerRuntime.setSurfaceModeWithPinchReset,
        playbackController: controllerRuntime.playbackController,
        runPerfBenchmark: controllerRuntime.runPerfBenchmark,
        copyPerfBenchmarkResult: controllerRuntime.copyPerfBenchmarkResult,
        getDebugEnabled: () => getState().debugEnabled,
        getCurrentSurfaceMode: () => getState().currentSurfaceMode,
        getCurrentViewMode: () => getState().currentViewMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        updateTerrain: controllerRuntime.updateTerrain,
        setStatus,
    });
}

export function bootstrapAppRuntime(options = {}) {
    const {
        elements,
        isPerfEnabled,
        setStatus,
        basePositions,
        indices,
    } = options;

    const runtimeStore = createRuntimeStore({
        basePositions,
        indices,
        createEraMetrics,
        debugEnabled: elements.debugToggleInput.checked,
    });

    const sceneRuntime = createSceneRuntime({
        elements,
        indices,
        basePositions,
        getState: runtimeStore.getState,
    });

    const controllerRuntime = createControllerRuntime(createControllerDeps({
        elements,
        isPerfEnabled,
        setStatus,
        runtimeStore,
        sceneRuntime,
    }));

    bindRuntimeUi({
        elements,
        sceneRuntime,
        controllerRuntime,
        getState: runtimeStore.getState,
        setStatus,
    });

    return {
        renderFrame: sceneRuntime.renderFrame,
        worldState: runtimeStore.worldState,
        stepWorldTick: controllerRuntime.stepWorldTick,
        stepWorldPlayback: controllerRuntime.stepWorldPlayback,
        runInitialSync: controllerRuntime.runInitialSync,
        shouldAdvanceWorld: controllerRuntime.shouldAdvanceWorld,
        getLastPerfBenchmarkResult: controllerRuntime.getLastPerfBenchmarkResult,
    };
}
