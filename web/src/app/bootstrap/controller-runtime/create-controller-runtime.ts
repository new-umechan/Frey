import {
    getExecModuleGraph,
    getExecModules,
    WorldSimController,
} from "../../../interface/wasm";
import { createRuntimeControllers } from "./controller-factories";
import { DEFAULT_ERA_SCALE } from "../../../shared/constants";
import { runInitialWorldAndUiSync } from "../post-init-sync";
import {
    type AppElements,
    type StatFields,
    type EraScaleWeightFields,
    type PerfStatFields,
} from "../../../components/dom";
import { type AppState, type WorldState } from "../../state/app-state";
import { type EraMetrics } from "../../state/era-presets";
import { type RuntimeState } from "../../runtime/state";
import {
    describeExecModuleGraph,
    getDefaultExecDisplayPhase,
} from "../../runtime/state";

export interface ControllerDeps {
    elements: AppElements;
    isPerfEnabled: boolean;
    setStatus: (msg: string) => void;
    world: WorldState;
    worldState: RuntimeState;
    getState: () => AppState;
    setState: (patch: Partial<AppState>) => void;
    getCurrentEraMetrics: () => EraMetrics;
    cameraController: any;
    terrainRenderer: any;
    wireframe: any;
    plateHover: any;
    globePinchFocusController: any;
    loadingOverlayController: any;
    syncClimateUi: () => void;
    renderFrame: () => void;
    renderInitializationFrames: any;
}

export interface RuntimeContext extends ControllerDeps {
    seedForm: HTMLFormElement;
    seedInput: HTMLInputElement;
    debugToggleInput: HTMLInputElement;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    viewModeInputs: HTMLInputElement[];
    statFields: StatFields;
    statusEraLabel: HTMLElement;
    playbackControls: any;
    eventLogList: HTMLUListElement;
    perfControls: any;
    perfStatFields: PerfStatFields | null;
    viewportPanel: HTMLElement;
    worldSimController: WorldSimController;
}

function createRuntimeContext(options: ControllerDeps): RuntimeContext {
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
        worldSimController: null as any, // Initialized later
    };
}

async function runInitialSync(context: RuntimeContext, runtimeControllers: any) {
    await runInitialWorldAndUiSync({
        updateTerrain: runtimeControllers.updateTerrain,
        defaultTerrainSeed: context.getState().currentSeed,
        eraScaleSelect: context.eraScaleSelect,
        eraScaleTickLabel: context.eraScaleTickLabel,
        eraScaleWeightFields: context.eraScaleWeightFields,
        currentEraScale: DEFAULT_ERA_SCALE,
        currentEraMetrics: context.getCurrentEraMetrics(),
        setEraScale: runtimeControllers.setEraScale,
        syncClimateUi: context.syncClimateUi,
        playbackController: runtimeControllers.playbackController,
        viewportPanel: context.viewportPanel,
        onResize: () => {
            context.cameraController.onResize();
            context.loadingOverlayController.render();
        },
        plateHover: context.plateHover,
    });
}

function shouldAdvanceWorld(context: RuntimeContext) {
    const state = context.getState();
    return context.worldState.playback.isPlaying && Boolean(state.currentTerrainData) && Boolean(state.activeWorldId);
}

export function createControllerRuntime(options: ControllerDeps) {
    const context = createRuntimeContext(options);
    context.worldSimController = new WorldSimController();
    context.worldState.execModules = getExecModules(context.worldSimController);
    context.worldState.execModuleGraph = getExecModuleGraph(context.worldSimController);
    context.worldState.slicePhase = getDefaultExecDisplayPhase(context.worldState);

    const runtimeControllers = createRuntimeControllers(context);
    runtimeControllers.playbackController.appendPlaybackEvent(
        "exec-modules-loaded",
        "実行DAG",
        describeExecModuleGraph(context.worldState),
        context.world.tick,
    );

    return {
        ...runtimeControllers,
        runInitialSync: () => runInitialSync(context, runtimeControllers),
        shouldAdvanceWorld: () => shouldAdvanceWorld(context),
    };
}
