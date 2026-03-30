export function createViewModeController(options: any = {}) {
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
