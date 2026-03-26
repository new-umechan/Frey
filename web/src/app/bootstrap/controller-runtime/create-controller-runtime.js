import { WorldSimController } from "../../../interface/wasm.js";
import { createRuntimeControllers } from "./controller-factories.js";
import { DEFAULT_ERA_SCALE } from "../../../core/constants.js";
import { runInitialWorldAndUiSync } from "../post-init-sync.js";

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

async function runInitialSync(context, runtimeControllers) {
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

function shouldAdvanceWorld(context) {
    const state = context.getState();
    return context.worldState.playback.isPlaying && Boolean(state.currentTerrainData) && Boolean(state.activeWorldId);
}

export function createControllerRuntime(options = {}) {
    const context = createRuntimeContext(options);
    context.worldSimController = new WorldSimController();

    const runtimeControllers = createRuntimeControllers(context);

    return {
        ...runtimeControllers,
        runInitialSync: () => runInitialSync(context, runtimeControllers),
        shouldAdvanceWorld: () => shouldAdvanceWorld(context),
    };
}
