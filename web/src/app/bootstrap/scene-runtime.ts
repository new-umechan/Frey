import { resizeViewport, createGlobeScene, type GlobeScene } from "../../gfx/scene";
import { buildRiverMaskTexture } from "../../gfx/materials/river-mask";
import { createCameraController } from "../../gfx/views/camera-controller";
import { createGlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller";
import { buildRenderPositions } from "../../gfx/views/terrain-visuals";
import { createLoadingOverlayController } from "./loading-overlay";
import { setupTerrainGeometryAttributes } from "./terrain-geometry-setup";
import { createClimateUiController } from "../controllers/climate-ui-controller";
import { createPlateHover, type PlateHoverController } from "../input/plate-hover";
import { createTerrainRenderer, type TerrainRenderer } from "../visualizers/terrain-renderer";
import { DEFAULT_VIEW_MODE } from "../../shared/constants";
import { type AppElements } from "../../components/dom";
import { type AppState } from "../state/app-state";
import { type GlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller";
import { type CoreBuffers } from "../sim/sync/types";
import { type Camera } from "three";

export interface CameraController {
    onResize: () => void;
    getCamera: () => Camera;
    getActiveControls: () => { update: () => void };
    setSurfaceMode: (nextMode: string) => void;
    getSurfaceMode: () => string;
}

export interface LoadingOverlayController {
    clear: () => void;
    render: () => void;
    setWorldInitializing: (value: boolean) => void;
}

export interface SceneRuntimeOptions {
    elements: AppElements;
    indices: Uint32Array;
    basePositions: Float32Array;
    getState: () => AppState;
    getCurrentTerrainData: () => CoreBuffers | null;
}

export interface SceneRuntime {
    cameraController: CameraController;
    terrainRenderer: TerrainRenderer;
    wireframe: { visible: boolean };
    plateHover: PlateHoverController;
    globePinchFocusController: GlobePinchFocusController;
    loadingOverlayController: LoadingOverlayController;
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
        getCurrentTerrainData,
    } = options;
    const {
        canvas,
        loadingOverlayCanvas,
        viewportPanel,
        climateLegend,
        domesticatesLegend,
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
        domesticatesLegend,
        getCurrentViewMode: () => getState().currentViewMode,
        getCurrentCellMetric: () => getState().currentCellMetric,
        getCurrentTerrainData,
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
            currentTerrainData: getCurrentTerrainData(),
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
