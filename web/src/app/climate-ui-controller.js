import { getClimateMetricMeta } from "./climate-metric.js";

function computeClimateLegendStats(values) {
    if (!values || values.length === 0) {
        return null;
    }
    let min = Number.POSITIVE_INFINITY;
    let max = Number.NEGATIVE_INFINITY;
    for (let i = 0; i < values.length; i += 1) {
        const value = values[i];
        if (!Number.isFinite(value)) {
            continue;
        }
        min = Math.min(min, value);
        max = Math.max(max, value);
    }
    if (!Number.isFinite(min) || !Number.isFinite(max)) {
        return null;
    }
    return {
        min,
        mid: (min + max) * 0.5,
        max,
    };
}

export function createClimateUiController(options = {}) {
    const {
        climateLegend,
        getCurrentViewMode,
        getCurrentClimateMetric,
        getCurrentTerrainData,
    } = options;

    const updateClimateHoverReadout = (payload) => {
        if (!climateLegend) {
            return;
        }
        climateLegend.hover.textContent = payload
            ? `Hover: ${payload.label} ${payload.value}`
            : "Hover: -";
    };

    const syncClimateUi = () => {
        if (!climateLegend) {
            return;
        }
        const currentViewMode = getCurrentViewMode();
        const currentClimateMetric = getCurrentClimateMetric();
        const currentTerrainData = getCurrentTerrainData();
        const isClimateMode = currentViewMode === "climate";
        climateLegend.panel.hidden = !isClimateMode;
        climateLegend.panel.setAttribute("aria-hidden", String(!isClimateMode));
        if (!isClimateMode) {
            updateClimateHoverReadout(null);
            return;
        }

        const meta = getClimateMetricMeta(currentClimateMetric);
        const stats = computeClimateLegendStats(currentTerrainData?.[meta.key]);
        climateLegend.panel.dataset.metric = currentClimateMetric;
        climateLegend.title.textContent = `${meta.label} (${meta.unit})`;
        climateLegend.min.textContent = stats ? meta.formatter(stats.min) : "-";
        climateLegend.mid.textContent = stats ? meta.formatter(stats.mid) : "-";
        climateLegend.max.textContent = stats ? meta.formatter(stats.max) : "-";
    };

    return {
        syncClimateUi,
        updateClimateHoverReadout,
    };
}
