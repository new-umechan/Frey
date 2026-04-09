import { bindAppUiControls } from "../controllers/ui-bindings";
import { createEraMetrics } from "../state/era-presets";
import { createRuntimeStore, type RuntimeStore } from "./runtime-store";
import { createSceneRuntime, type SceneRuntime } from "./scene-runtime";
import { createControllerRuntime, type ControllerDeps } from "./controller-runtime/create-controller-runtime";
import { renderInitializationFrames } from "./initialization-frames";
import { type AppElements } from "../../components/dom";

function createControllerDeps(options: {
    elements: AppElements;
    isPerfEnabled: boolean;
    setStatus: (msg: string) => void;
    runtimeStore: RuntimeStore;
    sceneRuntime: SceneRuntime;
}): ControllerDeps {
    const {
        elements,
        isPerfEnabled,
        setStatus,
        runtimeStore,
        sceneRuntime,
    } = options;

    return {
        isPerfEnabled,
        setStatus,
        store: runtimeStore,
        scene: sceneRuntime,
        elements,
        renderInitializationFrames,
    };
}

interface BindRuntimeUiOptions {
    elements: AppElements;
    sceneRuntime: SceneRuntime;
    controllerRuntime: any;
    getState: () => any;
    setStatus: (msg: string) => void;
}

function bindRuntimeUi(options: BindRuntimeUiOptions) {
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
        eraScaleTickLabel: elements.eraScaleTickLabel,
        eraScaleWeightFields: elements.eraScaleWeightFields,
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
        runPerf: controllerRuntime.runPerf,
        copyPerfResult: controllerRuntime.copyPerfResult,
        getDebugEnabled: () => getState().debugEnabled,
        getCurrentSurfaceMode: () => getState().currentSurfaceMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        getCurrentEraScale: () => getState().currentEraScale,
        getCurrentEraMetrics: () => getState().currentEraMetrics,
        updateTerrain: controllerRuntime.updateTerrain,
        setStatus,
    });
}

interface BootstrapAppRuntimeOptions {
    elements: AppElements;
    isPerfEnabled: boolean;
    setStatus: (msg: string) => void;
    basePositions: Float32Array;
    indices: Uint32Array;
}

export async function bootstrapAppRuntime(options: BootstrapAppRuntimeOptions) {
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
        getCurrentTerrainData: runtimeStore.getCurrentTerrainData,
    });

    const controllerRuntime = await createControllerRuntime(createControllerDeps({
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
        getLastPerfResult: controllerRuntime.getLastPerfResult,
    };
}
