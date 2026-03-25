const CELL_METRIC_DEFS = Object.freeze([
    {
        key: "height",
        fieldKind: "height",
        dataKey: "heightData",
        label: "標高",
        unit: "rel",
        category: "terrain",
        palette: "terrain",
        formatter: (value) => value.toFixed(3),
    },
    {
        key: "mantle_heat",
        fieldKind: "mantle_heat",
        dataKey: "mantleHeat",
        label: "熱量",
        unit: "norm",
        category: "terrain",
        palette: "magma",
        formatter: (value) => value.toFixed(3),
    },
    {
        key: "erosion_rate",
        fieldKind: "erosion_rate",
        dataKey: "erosionRate",
        label: "侵食",
        unit: "rel/tick",
        category: "terrain",
        palette: "amber",
        formatter: (value) => value.toFixed(4),
    },
    {
        key: "deposition_rate",
        fieldKind: "deposition_rate",
        dataKey: "depositionRate",
        label: "堆積",
        unit: "rel/tick",
        category: "terrain",
        palette: "teal",
        formatter: (value) => value.toFixed(4),
    },
    {
        key: "temperature",
        fieldKind: "temperature",
        dataKey: "temperature",
        label: "気温",
        unit: "℃",
        category: "climate",
        palette: "temp",
        formatter: (value) => `${value.toFixed(1)} ℃`,
    },
    {
        key: "precipitation",
        fieldKind: "precipitation",
        dataKey: "precipitation",
        label: "降水",
        unit: "mm/yr",
        category: "climate",
        palette: "rain",
        formatter: (value) => `${value.toFixed(0)} mm/yr`,
    },
    {
        key: "evapotranspiration",
        fieldKind: "evapotranspiration",
        dataKey: "evapotranspiration",
        label: "蒸散",
        unit: "mm/yr",
        category: "climate",
        palette: "rain",
        formatter: (value) => `${value.toFixed(0)} mm/yr`,
    },
    {
        key: "aridity",
        fieldKind: "aridity",
        dataKey: "aridity",
        label: "乾燥",
        unit: "index",
        category: "climate",
        palette: "dryness",
        formatter: (value) => value.toFixed(2),
    },
    {
        key: "ocean_temperature",
        fieldKind: "ocean_temperature",
        dataKey: "oceanTemperature",
        label: "海温",
        unit: "℃",
        category: "climate",
        palette: "temp",
        formatter: (value) => `${value.toFixed(1)} ℃`,
    },
    {
        key: "river_flux",
        fieldKind: "river_flux",
        dataKey: "riverFlux",
        label: "流量",
        unit: "norm",
        category: "hydrology",
        palette: "river",
        formatter: (value) => value.toFixed(3),
    },
    {
        key: "runoff",
        fieldKind: "runoff",
        dataKey: "runoff",
        label: "流出",
        unit: "mm/yr",
        category: "hydrology",
        palette: "rain",
        formatter: (value) => `${value.toFixed(0)} mm/yr`,
    },
    {
        key: "river_transport_cost",
        fieldKind: "river_transport_cost",
        dataKey: "riverTransportCost",
        label: "輸送",
        unit: "cost",
        category: "hydrology",
        palette: "cost",
        formatter: (value) => value.toFixed(3),
    },
]);

const CATEGORY_META = Object.freeze({
    terrain: {
        key: "terrain",
        label: "地形",
    },
    climate: {
        key: "climate",
        label: "気候",
    },
    hydrology: {
        key: "hydrology",
        label: "侵食",
    },
});

const METRIC_BY_KEY = new Map(CELL_METRIC_DEFS.map((metric) => [metric.key, metric]));
const METRIC_KIND_BY_KEY = new Map(CELL_METRIC_DEFS.map((metric, index) => [metric.key, index]));

export const DEFAULT_CELL_METRIC = "height";
export const DEFAULT_VIEW_MODE = "normal";

export function getCellMetricDefs() {
    return CELL_METRIC_DEFS;
}

export function normalizeCellMetric(metricKey) {
    return METRIC_BY_KEY.has(metricKey) ? metricKey : DEFAULT_CELL_METRIC;
}

export function getCellMetricMeta(metricKey) {
    return METRIC_BY_KEY.get(normalizeCellMetric(metricKey)) ?? CELL_METRIC_DEFS[0];
}

export function getMetricKindIndex(metricKey) {
    return METRIC_KIND_BY_KEY.get(normalizeCellMetric(metricKey)) ?? 0;
}

export function getMetricCategories() {
    return [
        {
            ...CATEGORY_META.terrain,
            metrics: CELL_METRIC_DEFS.filter((metric) => metric.category === "terrain"),
        },
        {
            ...CATEGORY_META.climate,
            metrics: CELL_METRIC_DEFS.filter((metric) => metric.category === "climate"),
        },
        {
            ...CATEGORY_META.hydrology,
            metrics: CELL_METRIC_DEFS.filter((metric) => metric.category === "hydrology"),
        },
    ];
}
