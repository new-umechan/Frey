import { getCellMetricMeta, isDomesticatesMetric } from "../visualizers/cell-metric";
import { type ClimateLegendElements, type DomesticatesLegendElements } from "../../components/dom";
import { type CoreBuffers, type TypedArray } from "../sim/sync/types";

function computeLegendStats(values: ArrayLike<number> | null) {
    if (!values || values.length === 0) {
        return null;
    }
    const finiteValues: number[] = [];
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
    const quantile = (ratio: number): number => {
        const index = Math.max(0, Math.min(finiteValues.length - 1, Math.floor((finiteValues.length - 1) * ratio)));
        return finiteValues[index];
    };
    return {
        min: quantile(0.05),
        mid: quantile(0.50),
        max: quantile(0.95),
    };
}

interface ClimateUiControllerOptions {
    climateLegend: ClimateLegendElements | null;
    domesticatesLegend: DomesticatesLegendElements | null;
    getCurrentViewMode: () => string;
    getCurrentCellMetric: () => string;
    getCurrentTerrainData: () => CoreBuffers | null;
}

export function createClimateUiController(options: ClimateUiControllerOptions) {
    const {
        climateLegend,
        domesticatesLegend,
        getCurrentViewMode,
        getCurrentCellMetric,
        getCurrentTerrainData,
    } = options;

    const updateClimateHoverReadout = (payload: { label: string; value: string } | null) => {
        if (!climateLegend) {
            return;
        }
        climateLegend.hover.textContent = payload
            ? `Hover: ${payload.label} ${payload.value}`
            : "Hover: -";
    };

    const syncClimateUi = () => {
        const currentViewMode = getCurrentViewMode();
        const currentCellMetric = getCurrentCellMetric();
        const currentTerrainData = getCurrentTerrainData();
        const isMetricMode = currentViewMode === "metric";
        const domesticatesMetric = isMetricMode && isDomesticatesMetric(currentCellMetric);

        if (climateLegend) {
            climateLegend.panel.hidden = !isMetricMode || domesticatesMetric;
            climateLegend.panel.setAttribute("aria-hidden", String(!isMetricMode || domesticatesMetric));
        }
        if (domesticatesLegend) {
            domesticatesLegend.panel.hidden = !domesticatesMetric;
            domesticatesLegend.panel.setAttribute("aria-hidden", String(!domesticatesMetric));
        }
        if (!isMetricMode) {
            updateClimateHoverReadout(null);
            return;
        }

        const meta = getCellMetricMeta(currentCellMetric);
        if (domesticatesMetric) {
            if (domesticatesLegend) {
                domesticatesLegend.title.textContent = meta.label;
                domesticatesLegend.adoptionScale.textContent = "adoption: 赤グラデーション (0.0 - 1.0)";
                domesticatesLegend.availableHint.textContent = "available: 青ハッチあり=1 / なし=0";
            }
            return;
        }
        if (!climateLegend) {
            return;
        }
        const metricValues = currentTerrainData?.[meta.dataKey] as TypedArray | undefined;
        const stats = computeLegendStats(metricValues ?? null);
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
