export function createWorldUiController(options: any = {}) {
    const {
        cameraController,
        terrainRenderer,
        wireframe,
        plateHover,
        debugToggleInput,
        statusEraLabel,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        getEraScalePreset,
        createEraMetrics,
        renderEraScaleControls,
        worldState,
        defaultEraScale,
        getState,
        setState,
        setStatus,
        appendPlaybackEvent,
    } = options;

    const setSurfaceMode = (nextMode) => {
        const state = getState();
        const normalizedMode = nextMode === "map" ? "map" : "globe";
        if (state.currentSurfaceMode === normalizedMode && state.currentTerrainData) {
            return;
        }
        setState({ currentSurfaceMode: normalizedMode });
        terrainRenderer.updateGeometryPositions(state.currentTerrainData, normalizedMode, {
            force: true,
            heightChanged: true,
            tick: state.worldTick,
        });
        cameraController.setSurfaceMode(normalizedMode);
        plateHover.hidePopup();
    };

    const setDebugModeEnabled = (nextEnabled) => {
        const state = getState();
        const debugEnabled = Boolean(nextEnabled);
        setState({ debugEnabled });
        debugToggleInput.checked = debugEnabled;
        wireframe.visible = debugEnabled && cameraController.getSurfaceMode() === "globe";
        terrainRenderer.applyTerrainMaterialState(
            state.currentViewMode,
            debugEnabled,
            state.currentCellMetric,
        );
        plateHover.syncDebugMode();
    };

    const setEraScale = (nextEraScale, metrics = null) => {
        const state = getState();
        const previousEra = state.currentEraScale;
        const currentEraScale = getEraScalePreset(nextEraScale).key ?? defaultEraScale;
        const currentEraMetrics = metrics ?? createEraMetrics(currentEraScale);
        setState({
            currentEraScale,
            currentEraMetrics,
        });
        worldState.runtimeTickMs = currentEraMetrics.runtimeTickMs;
        renderEraScaleControls(
            eraScaleSelect,
            eraScaleTickLabel,
            eraScaleWeightFields,
            currentEraScale,
            currentEraMetrics,
        );
        const preset = getEraScalePreset(currentEraScale);
        statusEraLabel.textContent = `時代: ${preset.label}`;
        setStatus(`Ready (${state.currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel}`);
        if (state.activeWorldId && previousEra !== currentEraScale) {
            const previousLabel = getEraScalePreset(previousEra).label;
            appendPlaybackEvent(
                "era-changed",
                "時代遷移",
                `${previousLabel} -> ${preset.label}`,
            );
        }
    };

    return {
        setSurfaceMode,
        setDebugModeEnabled,
        setEraScale,
    };
}
