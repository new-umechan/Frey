import { resizeViewport, createGlobeScene, type GlobeScene } from "../../gfx/scene.js";
import { buildRiverMaskTexture } from "../../gfx/materials/river-mask.js";
import { createCameraController } from "../../gfx/views/camera-controller.js";
import { createGlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller.js";
import { buildRenderPositions } from "../../gfx/views/terrain-visuals.js";
import { createLoadingOverlayController } from "./loading-overlay.js";
import { setupTerrainGeometryAttributes } from "./terrain-geometry-setup.js";
import { createClimateUiController } from "../ui/climate-ui-controller.js";
import { createPlateHover } from "../input/plate-hover.js";
import { createTerrainRenderer } from "../rendering/terrain-renderer.js";
import { DEFAULT_VIEW_MODE } from "../../core/constants.js";
import { type AppElements } from "../../ui/dom.js";
import { type AppState } from "../core/app-state.js";

export interface SceneRuntimeOptions {
    elements: AppElements;
    indices: Uint32Array;
    basePositions: Float32Array;
    getState: () => AppState;
}

export interface SceneRuntime {
    cameraController: any;
    terrainRenderer: any;
    wireframe: any;
    plateHover: any;
    globePinchFocusController: any;
    loadingOverlayController: any;
    syncClimateUi: () => void;
    renderFrame: () => void;
    onResize: () => void;
}

export function createSceneRuntime(options: SceneRuntimeOptions): SceneRuntime {
    const {
        elements,
        indices,
        basePositions,
        getState,
    } = options;
    const {
        canvas,
        loadingOverlayCanvas,
        viewportPanel,
        climateLegend,
        plateHoverPopup,
    } = elements;

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
    }: GlobeScene = createGlobeScene(canvas, indices);

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

    function onResize() {
        cameraController.onResize();
        loadingOverlayController.render();
    }

    return {
        cameraController,
        terrainRenderer,
        wireframe,
        plateHover,
        globePinchFocusController,
        loadingOverlayController,
        syncClimateUi,
        renderFrame,
        onResize,
    };
}
