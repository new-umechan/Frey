import { resizeViewport, createGlobeScene } from "../../gfx/scene.js";
import { buildRiverMaskTexture } from "../../gfx/materials/river-mask.js";
import { createCameraController } from "../../gfx/views/camera-controller.js";
import { createGlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller.js";
import { buildRenderPositions } from "../../gfx/views/terrain-visuals.js";
import { createLoadingOverlayController } from "./loading-overlay.js";
import { setupTerrainGeometryAttributes } from "./terrain-geometry-setup.js";
import { createClimateUiController } from "../climate-ui-controller.js";
import { createPlateHover } from "../plate-hover.js";
import { createTerrainRenderer } from "../terrain-renderer.js";
import { DEFAULT_VIEW_MODE } from "../../core/constants.js";

export function createSceneRuntime(options = {}) {
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
