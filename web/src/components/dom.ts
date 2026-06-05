function requireElement<T extends HTMLElement>(id: string, type: new () => T): T {
    const element = document.getElementById(id);
    if (!(element instanceof type)) {
        throw new Error(`required DOM element is missing: #${id}`);
    }
    return element as T;
}

function optionalElement<T extends HTMLElement>(id: string, type: new () => T): T | null {
    const element = document.getElementById(id);
    return element instanceof type ? (element as T) : null;
}

export interface ClimateLegendElements {
    panel: HTMLElement;
    title: HTMLElement;
    min: HTMLElement;
    mid: HTMLElement;
    max: HTMLElement;
    hover: HTMLElement;
}

function collectClimateLegend(): ClimateLegendElements | null {
    const panel = optionalElement("climate-legend-panel", HTMLElement);
    if (!panel) {
        return null;
    }
    return {
        panel,
        title: requireElement("climate-legend-title", HTMLElement),
        min: requireElement("climate-legend-min", HTMLElement),
        mid: requireElement("climate-legend-mid", HTMLElement),
        max: requireElement("climate-legend-max", HTMLElement),
        hover: requireElement("climate-legend-hover", HTMLElement),
    };
}

export interface DomesticatesLegendElements {
    panel: HTMLElement;
    title: HTMLElement;
    adoptionScale: HTMLElement;
    availableHint: HTMLElement;
}

function collectDomesticatesLegend(): DomesticatesLegendElements | null {
    const panel = optionalElement("domesticates-legend-panel", HTMLElement);
    if (!panel) {
        return null;
    }
    return {
        panel,
        title: requireElement("domesticates-legend-title", HTMLElement),
        adoptionScale: requireElement("domesticates-legend-adoption", HTMLElement),
        availableHint: requireElement("domesticates-legend-available", HTMLElement),
    };
}

export interface PlaybackControlsElements {
    overlay: HTMLElement;
    playToggleButton: HTMLButtonElement;
    currentTick: HTMLElement;
    maxTick: HTMLElement;
    historySeekSlider: HTMLInputElement;
    seekMinLabel: HTMLElement;
    seekMaxLabel: HTMLElement;
    seekBackwardButton: HTMLButtonElement;
    seekForwardButton: HTMLButtonElement;
}

function collectPlaybackControls(): PlaybackControlsElements {
    return {
        overlay: requireElement("playback-overlay", HTMLElement),
        playToggleButton: requireElement("play-toggle-button", HTMLButtonElement),
        currentTick: requireElement("playback-current-tick", HTMLElement),
        maxTick: requireElement("playback-max-tick", HTMLElement),
        historySeekSlider: requireElement("history-seek-slider", HTMLInputElement),
        seekMinLabel: requireElement("history-seek-min", HTMLElement),
        seekMaxLabel: requireElement("history-seek-max", HTMLElement),
        seekBackwardButton: requireElement("seek-backward-button", HTMLButtonElement),
        seekForwardButton: requireElement("seek-forward-button", HTMLButtonElement),
    };
}

export interface PerfControlsElements {
    runButton: HTMLButtonElement;
    copyButton: HTMLButtonElement;
    status: HTMLElement;
    progress: HTMLProgressElement;
}

export interface PerfStatFields {
    tickP50: HTMLElement;
    tickP95: HTMLElement;
    stepMean: HTMLElement;
    deltaMean: HTMLElement;
    geomMean: HTMLElement;
    riverMean: HTMLElement;
}

export interface PerfElements {
    perfPanel: HTMLElement | null;
    perfControls: PerfControlsElements | null;
    perfStatFields: PerfStatFields | null;
}

function collectPerfElements(perfEnabled: boolean): PerfElements {
    const perfPanel = optionalElement("perf-panel", HTMLElement);
    if (!perfEnabled || !perfPanel) {
        return {
            perfPanel,
            perfControls: null,
            perfStatFields: null,
        };
    }

    return {
        perfPanel,
        perfControls: {
            runButton: requireElement("perf-run-button", HTMLButtonElement),
            copyButton: requireElement("perf-copy-button", HTMLButtonElement),
            status: requireElement("perf-status", HTMLElement),
            progress: requireElement("perf-progress", HTMLProgressElement),
        },
        perfStatFields: {
            tickP50: requireElement("perf-tick-p50", HTMLElement),
            tickP95: requireElement("perf-tick-p95", HTMLElement),
            stepMean: requireElement("perf-step-mean", HTMLElement),
            deltaMean: requireElement("perf-delta-mean", HTMLElement),
            geomMean: requireElement("perf-geom-mean", HTMLElement),
            riverMean: requireElement("perf-river-mean", HTMLElement),
        },
    };
}

export interface EraScaleWeightFields {
    geology: HTMLElement;
    climate: HTMLElement;
    ecology: HTMLElement;
    civilization: HTMLElement;
}

export interface StatFields {
    level: HTMLElement;
    seed: HTMLElement;
    plates: HTMLElement;
    land: HTMLElement;
}

export interface AppElements {
    appShell: HTMLElement;
    canvas: HTMLCanvasElement;
    loadingOverlayCanvas: HTMLCanvasElement;
    viewportPanel: HTMLDivElement;
    seedForm: HTMLFormElement;
    seedInput: HTMLInputElement;
    sidebarToggle: HTMLButtonElement | null;
    statusMessage: HTMLElement;
    statusEraLabel: HTMLElement;
    plateHoverPopup: HTMLDivElement;
    debugToggleInput: HTMLInputElement;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    viewModeInputs: HTMLInputElement[];
    climateLegend: ClimateLegendElements | null;
    domesticatesLegend: DomesticatesLegendElements | null;
    controlHelpModal: HTMLDivElement | null;
    controlHelpCloseButton: HTMLButtonElement | null;
    playbackControls: PlaybackControlsElements;
    eventLogList: HTMLUListElement;
    perfPanel: HTMLElement | null;
    perfControls: PerfControlsElements | null;
    perfStatFields: PerfStatFields | null;
    statFields: StatFields;
    setSidebarOpen?: (isOpen: boolean) => void;
}

export function collectAppElements(options: { perfEnabled?: boolean } = {}): AppElements {
    const perfEnabled = options.perfEnabled === true;
    const canvas = requireElement("mesh-canvas", HTMLCanvasElement);
    const loadingOverlayCanvas = requireElement("loading-overlay-canvas", HTMLCanvasElement);
    const appShell = canvas.closest(".app-shell");
    if (!(appShell instanceof HTMLElement)) {
        throw new Error("required app shell is missing");
    }

    const viewportPanel = requireElement("viewport-panel", HTMLDivElement);
    const seedForm = requireElement("seed-form", HTMLFormElement);
    const seedInput = requireElement("seed-input", HTMLInputElement);
    const sidebarToggle = optionalElement("sidebar-toggle", HTMLButtonElement);
    const statusMessage = requireElement("status-message", HTMLElement);
    const statusEraLabel = requireElement("status-era", HTMLElement);
    const plateHoverPopup = requireElement("plate-hover-popup", HTMLDivElement);
    const debugToggleInput = requireElement("debug-mode-toggle", HTMLInputElement);
    const eraScaleSelect = requireElement("era-scale-select", HTMLSelectElement);
    const eraScaleTickLabel = requireElement("era-scale-tick-label", HTMLElement);

    const viewModeInputs = Array.from(document.querySelectorAll('input[name="view-mode"]'))
        .filter((input): input is HTMLInputElement => input instanceof HTMLInputElement);

    const climateLegend = collectClimateLegend();
    const domesticatesLegend = collectDomesticatesLegend();
    const controlHelpModal = optionalElement("control-help-modal", HTMLDivElement);
    const controlHelpCloseButton = optionalElement("control-help-close", HTMLButtonElement);
    const playbackControls = collectPlaybackControls();
    const eventLogList = requireElement("event-log-list", HTMLUListElement);

    const { perfPanel, perfControls, perfStatFields } = collectPerfElements(perfEnabled);
    const statFields: StatFields = {
        level: requireElement("stat-level", HTMLElement),
        seed: requireElement("stat-seed", HTMLElement),
        plates: requireElement("stat-plates", HTMLElement),
        land: requireElement("stat-land", HTMLElement),
    };

    const eraScaleWeightFields: EraScaleWeightFields = {
        geology: requireElement("era-weight-geology", HTMLElement),
        climate: requireElement("era-weight-climate", HTMLElement),
        ecology: requireElement("era-weight-ecology", HTMLElement),
        civilization: requireElement("era-weight-civilization", HTMLElement),
    };

    return {
        appShell,
        canvas,
        loadingOverlayCanvas,
        viewportPanel,
        seedForm,
        seedInput,
        sidebarToggle,
        statusMessage,
        statusEraLabel,
        plateHoverPopup,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        viewModeInputs,
        climateLegend,
        domesticatesLegend,
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfPanel,
        perfControls,
        perfStatFields,
        statFields,
    };
}
