export function requireElement(id, type) {
    const element = document.getElementById(id);
    if (!(element instanceof type)) {
        throw new Error(`required DOM element is missing: #${id}`);
    }
    return element;
}

export function collectAppElements() {
    const canvas = requireElement("mesh-canvas", HTMLCanvasElement);
    const appShell = canvas.closest(".app-shell");
    const viewportPanel = requireElement("viewport-panel", HTMLDivElement);
    const seedForm = requireElement("seed-form", HTMLFormElement);
    const seedInput = requireElement("seed-input", HTMLInputElement);
    const sidebarToggle = requireElement("sidebar-toggle", HTMLButtonElement);
    const statusMessage = requireElement("status-message", HTMLElement);
    const plateHoverPopup = requireElement("plate-hover-popup", HTMLDivElement);
    const debugToggleInput = requireElement("debug-mode-toggle", HTMLInputElement);
    const eraScaleSelect = requireElement("era-scale-select", HTMLSelectElement);
    const eraScaleTickLabel = requireElement("era-scale-tick-label", HTMLElement);
    const viewModeInputs = Array.from(
        document.querySelectorAll('input[name="view-mode"]'),
    ).filter((input) => input instanceof HTMLInputElement);
    const climateMetricGroup = requireElement("climate-metric-group", HTMLDivElement);
    const climateMetricInputs = Array.from(
        document.querySelectorAll('input[name="climate-metric"]'),
    ).filter((input) => input instanceof HTMLInputElement);
    const climateLegendPanel = requireElement("climate-legend-panel", HTMLElement);
    const climateLegend = {
        panel: climateLegendPanel,
        title: requireElement("climate-legend-title", HTMLElement),
        min: requireElement("climate-legend-min", HTMLElement),
        mid: requireElement("climate-legend-mid", HTMLElement),
        max: requireElement("climate-legend-max", HTMLElement),
        hover: requireElement("climate-legend-hover", HTMLElement),
    };
    const climateControlHint = requireElement("control-climate-hint", HTMLElement);

    if (!(appShell instanceof HTMLElement)) {
        throw new Error("required app shell is missing");
    }

    const statFields = {
        vertices: requireElement("stat-vertices", HTMLElement),
        level: requireElement("stat-level", HTMLElement),
        seed: requireElement("stat-seed", HTMLElement),
        plates: requireElement("stat-plates", HTMLElement),
        land: requireElement("stat-land", HTMLElement),
    };
    const eraScaleWeightFields = {
        geology: requireElement("era-weight-geology", HTMLElement),
        climate: requireElement("era-weight-climate", HTMLElement),
        ecology: requireElement("era-weight-ecology", HTMLElement),
        civilization: requireElement("era-weight-civilization", HTMLElement),
    };

    return {
        appShell,
        canvas,
        viewportPanel,
        seedForm,
        seedInput,
        sidebarToggle,
        statusMessage,
        plateHoverPopup,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        viewModeInputs,
        climateMetricGroup,
        climateMetricInputs,
        climateLegend,
        climateControlHint,
        statFields,
    };
}
