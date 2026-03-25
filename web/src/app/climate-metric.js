import { getCellMetricMeta, normalizeCellMetric } from "./cell-metric.js";

export function normalizeClimateMetric(metric) {
    if (metric === "precipitation") {
        return "precipitation";
    }
    return "temperature";
}

export function getClimateMetricMeta(metric) {
    return getCellMetricMeta(normalizeCellMetric(normalizeClimateMetric(metric)));
}
