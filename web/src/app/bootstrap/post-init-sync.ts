import { renderEraScaleControls } from "../state/era-presets";
import { type EraScaleWeightFields } from "../../components/dom";
import { type EraMetrics } from "../state/era-presets";

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
}: {
    updateTerrain: (seed: string) => Promise<void>;
    defaultTerrainSeed: string;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    currentEraScale: string;
    currentEraMetrics: EraMetrics;
    setEraScale: (scale: string, metrics: EraMetrics) => void;
    syncClimateUi: () => void;
    playbackController: {
        refreshHistoryTicks: () => Promise<void>;
        syncPlaybackUi: () => void;
        notePlaybackOverlayActivity: () => void;
        bindOverlayActivityEvents: (element: HTMLElement) => void;
    };
    viewportPanel: HTMLElement;
    onResize: () => void;
    plateHover: { hidePopup: () => void };
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

    await playbackController.refreshHistoryTicks();
    playbackController.syncPlaybackUi();
    playbackController.notePlaybackOverlayActivity();
    playbackController.bindOverlayActivityEvents(viewportPanel);

    onResize();
    plateHover.hidePopup();
}
