import { type AppState } from "../state/app-state";
import { type RuntimeState } from "../runtime/state";
import { type EraMetrics, type EraScaleConfig, type EraScaleWeightFields } from "../state/era-presets";
import { type PlateHoverController } from "../input/plate-hover";
import { type TerrainRenderer } from "../visualizers/terrain-renderer";
import { type CoreBuffers } from "../sim/sync/types";

interface CameraController {
    setSurfaceMode: (nextMode: string) => void;
    getSurfaceMode: () => string;
}

export interface WorldUiController {
    setSurfaceMode: (nextMode: string) => void;
    setDebugModeEnabled: (nextEnabled: boolean) => void;
    setEraScale: (nextEraScale: string, metrics?: EraMetrics | null) => void;
}

export interface WorldUiControllerOptions {
    cameraController: CameraController;
    terrainRenderer: TerrainRenderer;
    wireframe: { visible: boolean };
    plateHover: PlateHoverController;
    debugToggleInput: HTMLInputElement;
    statusEraLabel: HTMLElement;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    getEraScalePreset: (era: string) => EraScaleConfig & { key: string };
    createEraMetrics: (era: string) => EraMetrics;
    renderEraScaleControls: (
        select: HTMLSelectElement,
        label: HTMLElement,
        fields: EraScaleWeightFields,
        era: string,
        metrics: EraMetrics,
    ) => void;
    worldState: RuntimeState;
    defaultEraScale: string;
    getState: () => AppState;
    getCurrentTerrainData: () => CoreBuffers | null;
    getActiveWorldId: () => string | null;
    setState: (patch: Partial<AppState>) => void;
    getWorldTick: () => number;
    setStatus: (msg: string) => void;
    appendPlaybackEvent: (type: string, label: string, detail?: string) => void;
}

export function createWorldUiController(options: WorldUiControllerOptions): WorldUiController {
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
        getCurrentTerrainData,
        getActiveWorldId,
        setState,
        getWorldTick,
        setStatus,
        appendPlaybackEvent,
    } = options;

    const setSurfaceMode = (nextMode: string) => {
        const state = getState();
        const currentTerrainData = getCurrentTerrainData();
        const normalizedMode = nextMode === "map" ? "map" : "globe";
        if (state.currentSurfaceMode === normalizedMode && currentTerrainData) {
            return;
        }
        setState({ currentSurfaceMode: normalizedMode });
        if (currentTerrainData) {
            terrainRenderer.updateGeometryPositions(currentTerrainData, normalizedMode, {
                force: true,
                heightChanged: true,
                tick: getWorldTick(),
            });
        }
        cameraController.setSurfaceMode(normalizedMode);
        plateHover.hidePopup();
    };

    const setDebugModeEnabled = (nextEnabled: boolean) => {
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

    const setEraScale = (nextEraScale: string, metrics: EraMetrics | null = null) => {
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
        if (getActiveWorldId() && previousEra !== currentEraScale) {
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
