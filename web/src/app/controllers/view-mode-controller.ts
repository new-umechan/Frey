import { type PlateHoverController } from "../input/plate-hover";
import { type TerrainRenderer } from "../visualizers/terrain-renderer";

export interface ViewModeControllerOptions {
    viewModeInputs: HTMLInputElement[];
    normalizeCellMetric: (metric: string) => string;
    terrainRenderer: TerrainRenderer;
    plateHover: PlateHoverController;
    syncClimateUi: () => void;
    syncVisibleFieldsForCurrentView: () => void;
    getCurrentViewMode: () => string;
    getCurrentCellMetric: () => string;
    getDebugEnabled: () => boolean;
    setCurrentViewMode: (nextMode: string) => void;
    setCurrentCellMetric: (nextMetric: string) => void;
}

export interface ViewModeController {
    setViewMode: (nextMode: string) => void;
    setCellMetric: (nextMetric: string) => void;
}

export function createViewModeController(options: ViewModeControllerOptions): ViewModeController {
    const {
        viewModeInputs,
        normalizeCellMetric,
        terrainRenderer,
        plateHover,
        syncClimateUi,
        syncVisibleFieldsForCurrentView,
        getCurrentViewMode,
        getCurrentCellMetric,
        getDebugEnabled,
        setCurrentViewMode,
        setCurrentCellMetric,
    } = options;

    const setViewMode = (nextMode: string) => {
        const normalizedMode = nextMode === "metric" ? "metric" : "normal";
        setCurrentViewMode(normalizedMode);
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        syncVisibleFieldsForCurrentView();
        terrainRenderer.applyTerrainMaterialState(
            normalizedMode,
            getDebugEnabled(),
            getCurrentCellMetric(),
        );
        syncClimateUi();
        if (normalizedMode !== "metric") {
            plateHover.hidePopup();
        }
    };

    const setCellMetric = (nextMetric: string) => {
        const normalizedMetric = normalizeCellMetric(nextMetric);
        setCurrentCellMetric(normalizedMetric);
        if (getCurrentViewMode() !== "metric") {
            setViewMode("metric");
            return;
        }
        terrainRenderer.applyTerrainMaterialState(
            getCurrentViewMode(),
            getDebugEnabled(),
            normalizedMetric,
        );
        syncVisibleFieldsForCurrentView();
        syncClimateUi();
        plateHover.hidePopup();
    };

    return {
        setViewMode,
        setCellMetric,
    };
}
