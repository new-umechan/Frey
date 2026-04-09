import { createRuntimeControllers } from "./controller-factories";
import { DEFAULT_ERA_SCALE } from "../../../shared/constants";
import { runInitialWorldAndUiSync } from "../post-init-sync";
import {
    type AppElements,
    type StatFields,
    type EraScaleWeightFields,
    type PerfStatFields,
    type PlaybackControlsElements,
    type PerfControlsElements,
} from "../../../components/dom";
import {
    describeExecModuleGraph,
    getDefaultExecDisplayPhase,
} from "../../runtime/state";
import { createEngineWorkerClient } from "../../engine/engine-worker-client";
import { type EngineClient } from "../../engine/engine-client";
import { type RuntimeStore } from "../runtime-store";
import { type SceneRuntime } from "../scene-runtime";
import { type PlaybackController } from "../../playback/playback-controller";

export interface ControllerDeps {
    isPerfEnabled: boolean;
    setStatus: (msg: string) => void;
    store: RuntimeStore;
    scene: SceneRuntime;
    elements: AppElements;
    renderInitializationFrames: (renderFrame: () => void) => Promise<void>;
}

export interface RuntimeDomRefs {
    seedForm: HTMLFormElement;
    seedInput: HTMLInputElement;
    debugToggleInput: HTMLInputElement;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    viewModeInputs: HTMLInputElement[];
    statFields: StatFields;
    statusEraLabel: HTMLElement;
    playbackControls: PlaybackControlsElements;
    eventLogList: HTMLUListElement;
    perfControls: PerfControlsElements | null;
    perfStatFields: PerfStatFields | null;
    viewportPanel: HTMLElement;
}

export interface RuntimeContext {
    isPerfEnabled: boolean;
    setStatus: (msg: string) => void;
    store: RuntimeStore;
    scene: SceneRuntime;
    dom: RuntimeDomRefs;
    renderInitializationFrames: (renderFrame: () => void) => Promise<void>;
    engineClient: EngineClient;
}

function createRuntimeContext(options: ControllerDeps): RuntimeContext {
    const {
        isPerfEnabled,
        setStatus,
        store,
        scene,
        elements,
        renderInitializationFrames,
    } = options;

    const dom: RuntimeDomRefs = {
        seedForm: elements.seedForm,
        seedInput: elements.seedInput,
        debugToggleInput: elements.debugToggleInput,
        eraScaleSelect: elements.eraScaleSelect,
        eraScaleTickLabel: elements.eraScaleTickLabel,
        eraScaleWeightFields: elements.eraScaleWeightFields,
        viewModeInputs: elements.viewModeInputs,
        statFields: elements.statFields,
        statusEraLabel: elements.statusEraLabel,
        playbackControls: elements.playbackControls,
        eventLogList: elements.eventLogList,
        perfControls: elements.perfControls,
        perfStatFields: elements.perfStatFields,
        viewportPanel: elements.viewportPanel,
    };

    return {
        isPerfEnabled,
        setStatus,
        store,
        scene,
        dom,
        renderInitializationFrames,
        engineClient: null as unknown as EngineClient,
    };
}

interface RuntimeControllerHooks {
    updateTerrain: (seed: string) => Promise<void>;
    setEraScale: (era: string) => void;
    playbackController: PlaybackController;
}

async function runInitialSync(context: RuntimeContext, runtimeControllers: RuntimeControllerHooks) {
    await runInitialWorldAndUiSync({
        updateTerrain: runtimeControllers.updateTerrain,
        defaultTerrainSeed: context.store.getState().currentSeed,
        eraScaleSelect: context.dom.eraScaleSelect,
        eraScaleTickLabel: context.dom.eraScaleTickLabel,
        eraScaleWeightFields: context.dom.eraScaleWeightFields,
        currentEraScale: DEFAULT_ERA_SCALE,
        currentEraMetrics: context.store.getCurrentEraMetrics(),
        setEraScale: runtimeControllers.setEraScale,
        syncClimateUi: context.scene.syncClimateUi,
        playbackController: runtimeControllers.playbackController,
        viewportPanel: context.dom.viewportPanel,
        onResize: context.scene.onResize,
        plateHover: context.scene.plateHover,
    });
}

function shouldAdvanceWorld(context: RuntimeContext) {
    const state = context.store.getState();
    return context.store.worldState.playback.isPlaying && Boolean(state.currentTerrainData) && Boolean(state.activeWorldId);
}

export async function createControllerRuntime(options: ControllerDeps) {
    const context = createRuntimeContext(options);
    context.engineClient = createEngineWorkerClient();
    context.store.worldState.execModules = [];
    context.store.worldState.execModuleGraph = null;
    context.store.worldState.slicePhase = getDefaultExecDisplayPhase(context.store.worldState);
    context.store.worldState.execModules = await context.engineClient.get_exec_modules();
    context.store.worldState.execModuleGraph = await context.engineClient.get_exec_module_graph();
    context.store.worldState.slicePhase = getDefaultExecDisplayPhase(context.store.worldState);

    const runtimeControllers = createRuntimeControllers(context);
    runtimeControllers.playbackController.appendPlaybackEvent(
        "exec-modules-loaded",
        "実行DAG",
        describeExecModuleGraph(context.store.worldState),
        context.store.world.tick,
    );

    return {
        ...runtimeControllers,
        runInitialSync: () => runInitialSync(context, runtimeControllers),
        shouldAdvanceWorld: () => shouldAdvanceWorld(context),
    };
}
