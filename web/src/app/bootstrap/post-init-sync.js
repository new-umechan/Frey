import { renderEraScaleControls } from "../era-presets.js";

export async function runInitialWorldAndUiSync({
    updateTerrain,
    defaultTerrainSeed,
    eraScaleSelect,
    eraScaleTickLabel,
    eraScaleWeightFields,
    currentEraScale,
    currentEraMetrics,
    setEraScale,
    syncClimateUi,
    playbackController,
    viewportPanel,
    onResize,
    plateHover,
}) {
    await updateTerrain(defaultTerrainSeed);

    eraScaleSelect.setAttribute("disabled", "disabled");
    eraScaleSelect.setAttribute("aria-disabled", "true");
    eraScaleSelect.title = "時代プリセットは進行状況に応じて自動切り替えされます。";

    renderEraScaleControls(
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        currentEraScale,
        currentEraMetrics,
    );
    setEraScale(currentEraScale, currentEraMetrics);
    syncClimateUi();

    playbackController.refreshHistoryTicks();
    playbackController.syncPlaybackUi();
    playbackController.notePlaybackOverlayActivity();
    playbackController.bindOverlayActivityEvents(viewportPanel);

    onResize();
    plateHover.hidePopup();
}
