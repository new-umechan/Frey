import { getCellMetricMeta } from "../rendering/cell-metric";

function computeLegendStats(values) {
    if (!values || values.length === 0) {
        return null;
    }
    const finiteValues = [];
    for (let i = 0; i < values.length; i += 1) {
        const value = values[i];
        if (Number.isFinite(value)) {
            finiteValues.push(value);
        }
    }
    if (finiteValues.length === 0) {
        return null;
    }
    finiteValues.sort((a, b) => a - b);
    const quantile = (ratio) => {
        const index = Math.max(0, Math.min(finiteValues.length - 1, Math.floor((finiteValues.length - 1) * ratio)));
        return finiteValues[index];
    };
    return {
        min: quantile(0.05),
        mid: quantile(0.50),
        max: quantile(0.95),
    };
}

export function createClimateUiController(options: any = {}) {
    const {
        climateLegend,
        getCurrentViewMode,
        getCurrentCellMetric,
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
        const currentCellMetric = getCurrentCellMetric();
        const currentTerrainData = getCurrentTerrainData();
        const isMetricMode = currentViewMode === "metric";
        climateLegend.panel.hidden = !isMetricMode;
        climateLegend.panel.setAttribute("aria-hidden", String(!isMetricMode));
        if (!isMetricMode) {
            updateClimateHoverReadout(null);
            return;
        }

        const meta = getCellMetricMeta(currentCellMetric);
        const stats = computeLegendStats(currentTerrainData?.[meta.dataKey]);
        climateLegend.panel.dataset.metric = currentCellMetric;
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
