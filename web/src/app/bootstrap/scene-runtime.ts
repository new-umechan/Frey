import { resizeViewport, createGlobeScene, type GlobeScene } from "../../gfx/scene";
import { buildRiverMaskTexture } from "../../gfx/materials/river-mask";
import { createCameraController } from "../../gfx/views/camera-controller";
import { createGlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller";
import { buildRenderPositions } from "../../gfx/views/terrain-visuals";
import { createLoadingOverlayController } from "./loading-overlay";
import { setupTerrainGeometryAttributes } from "./terrain-geometry-setup";
import { createClimateUiController } from "../ui/climate-ui-controller";
import { createPlateHover } from "../input/plate-hover";
import { createTerrainRenderer } from "../rendering/terrain-renderer";
import { DEFAULT_VIEW_MODE } from "../../core/constants";
import { type AppElements } from "../../ui/dom";
import { type AppState } from "../core/app-state";

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
