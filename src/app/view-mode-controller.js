export function createViewModeController(options = {}) {
    const {
        viewModeInputs,
        normalizeClimateMetric,
        terrainRenderer,
        plateHover,
        syncClimateUi,
        syncVisibleFieldsForCurrentView,
        getCurrentViewMode,
        getCurrentClimateMetric,
        getDebugEnabled,
        setCurrentViewMode,
        setCurrentClimateMetric,
    } = options;

    const setViewMode = (nextMode) => {
        const normalizedMode = (
            nextMode === "plates"
            || nextMode === "mantle"
            || nextMode === "climate"
        )
            ? nextMode
            : "normal";
        setCurrentViewMode(normalizedMode);
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        syncVisibleFieldsForCurrentView();
        terrainRenderer.applyTerrainMaterialState(
            normalizedMode,
            getDebugEnabled(),
            getCurrentClimateMetric(),
        );
        syncClimateUi();
        if (normalizedMode !== "plates") {
            plateHover.hidePopup();
        }
    };

    const setClimateMetric = (nextMetric) => {
        const normalizedMetric = normalizeClimateMetric(nextMetric);
        setCurrentClimateMetric(normalizedMetric);
        if (getCurrentViewMode() === "climate") {
            syncVisibleFieldsForCurrentView();
        }
        terrainRenderer.applyTerrainMaterialState(
            getCurrentViewMode(),
            getDebugEnabled(),
            normalizedMetric,
        );
        syncClimateUi();
        plateHover.hidePopup();
    };

    return {
        setViewMode,
        setClimateMetric,
    };
}
