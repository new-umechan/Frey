export function normalizeClimateMetric(metric) {
    return metric === "precipitation" ? "precipitation" : "temperature";
}

export function getClimateMetricMeta(metric) {
    if (normalizeClimateMetric(metric) === "precipitation") {
        return {
            key: "precipitation",
            label: "降水量",
            unit: "mm/yr",
            formatter: (value) => `${value.toFixed(0)} mm/yr`,
        };
    }
    return {
        key: "temperature",
        label: "気温",
        unit: "℃",
        formatter: (value) => `${value.toFixed(1)} ℃`,
    };
}
