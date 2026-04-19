export interface CellMetricDef {
    key: string;
    fieldKind: string;
    dataKey: string;
    label: string;
    unit: string;
    category: string;
    palette: string;
    formatter: (value: number) => string;
    overlayFieldKind?: string;
    overlayDataKey?: string;
}

const CROPS = Object.freeze([
    "wheat",
    "rice",
    "maize",
    "millet",
    "potato",
    "cassava",
    "sorghum",
    "yam",
] as const);
const LIVESTOCK = Object.freeze(["cattle", "horse", "sheep", "pig", "camel"] as const);
const BIOME_LABELS = Object.freeze([
    "熱帯林",
    "サバンナ",
    "砂漠",
    "草原",
    "温帯林",
    "針葉樹林",
    "ツンドラ",
    "湿地",
    "高山帯",
]);

function biomeLabelFromId(value: number): string {
    const index = Math.trunc(value);
    if (index < 0 || index >= BIOME_LABELS.length) {
        return `未知(${index})`;
    }
    return BIOME_LABELS[index];
}

function cropLabel(name: string): string {
    return `作物 ${name[0].toUpperCase()}${name.slice(1)}`;
}

function livestockLabel(name: string): string {
    return `家畜 ${name[0].toUpperCase()}${name.slice(1)}`;
}

function makeCropMetric(name: string): CellMetricDef {
    return {
        key: `crop_adoption_${name}`,
        fieldKind: `crop_adoption_${name}`,
        dataKey: `cropAdoption${name[0].toUpperCase()}${name.slice(1)}`,
        label: cropLabel(name),
        unit: "adoption",
        category: "domesticates",
        palette: "adoption",
        formatter: (value) => value.toFixed(3),
        overlayFieldKind: `crop_available_${name}`,
        overlayDataKey: `cropAvailable${name[0].toUpperCase()}${name.slice(1)}`,
    };
}

function makeLivestockMetric(name: string): CellMetricDef {
    return {
        key: `livestock_adoption_${name}`,
        fieldKind: `livestock_adoption_${name}`,
        dataKey: `livestockAdoption${name[0].toUpperCase()}${name.slice(1)}`,
        label: livestockLabel(name),
        unit: "adoption",
        category: "domesticates",
        palette: "adoption",
        formatter: (value) => value.toFixed(3),
        overlayFieldKind: `livestock_available_${name}`,
        overlayDataKey: `livestockAvailable${name[0].toUpperCase()}${name.slice(1)}`,
    };
}

const DOMESTICATES_METRICS: readonly CellMetricDef[] = Object.freeze([
    ...CROPS.map(makeCropMetric),
    ...LIVESTOCK.map(makeLivestockMetric),
]);

const CELL_METRIC_DEFS: readonly CellMetricDef[] = Object.freeze([
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
        key: "plate_id",
        fieldKind: "plate_id",
        dataKey: "plateId",
        label: "プレート",
        unit: "plate",
        category: "terrain",
        palette: "plate",
        formatter: (value) => `#${Math.trunc(value)}`,
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
        key: "wind_direction",
        fieldKind: "wind_u",
        dataKey: "windU",
        label: "風向",
        unit: "vector",
        category: "climate",
        palette: "wind",
        formatter: (value) => `${value.toFixed(2)} m/s`,
        overlayFieldKind: "wind_v",
        overlayDataKey: "windV",
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
        key: "ice_pressure",
        fieldKind: "ice_pressure",
        dataKey: "icePressure",
        label: "氷圧",
        unit: "norm",
        category: "glaciology",
        palette: "icePressure",
        formatter: (value) => value.toFixed(3),
    },
    {
        key: "biome",
        fieldKind: "biome",
        dataKey: "biome",
        label: "気候種",
        unit: "biome",
        category: "climate",
        palette: "biome",
        formatter: (value) => biomeLabelFromId(value),
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
    ...DOMESTICATES_METRICS,
]);

export interface CategoryMeta {
    key: string;
    label: string;
}

const CATEGORY_META: Record<string, CategoryMeta> = Object.freeze({
    geology: {
        key: "geology",
        label: "地質",
    },
    climate: {
        key: "climate",
        label: "気候",
    },
    river_glaciology: {
        key: "river_glaciology",
        label: "河川・氷河",
    },
    ecology_domesticates: {
        key: "ecology_domesticates",
        label: "生態・家畜",
    },
    population: {
        key: "population",
        label: "人口",
    },
    polity_system: {
        key: "polity_system",
        label: "政治体制",
    },
});

const METRIC_BY_KEY = new Map<string, CellMetricDef>(CELL_METRIC_DEFS.map((metric) => [metric.key, metric]));

const DEFAULT_CELL_METRIC = "height";

export function normalizeCellMetric(metricKey: string): string {
    return METRIC_BY_KEY.has(metricKey) ? metricKey : DEFAULT_CELL_METRIC;
}

export function getCellMetricMeta(metricKey: string): CellMetricDef {
    return METRIC_BY_KEY.get(normalizeCellMetric(metricKey)) ?? CELL_METRIC_DEFS[0];
}

export function getOverlayFieldKindForMetric(metricKey: string): string | null {
    const metric = getCellMetricMeta(metricKey);
    return metric.overlayFieldKind ?? null;
}

export function isDomesticatesMetric(metricKey: string): boolean {
    return getCellMetricMeta(metricKey).category === "domesticates";
}

export function isBiomeMetric(metricKey: string): boolean {
    return normalizeCellMetric(metricKey) === "biome";
}

export function biomeLabels(): readonly string[] {
    return BIOME_LABELS;
}

export function formatBiomeLabel(value: number): string {
    return biomeLabelFromId(value);
}

export interface MetricCategory extends CategoryMeta {
    metrics: CellMetricDef[];
}

export function getMetricCategories(): MetricCategory[] {
    return [
        {
            ...CATEGORY_META.geology,
            metrics: CELL_METRIC_DEFS.filter((metric) => metric.category === "terrain"),
        },
        {
            ...CATEGORY_META.climate,
            metrics: CELL_METRIC_DEFS.filter((metric) => metric.category === "climate"),
        },
        {
            ...CATEGORY_META.river_glaciology,
            metrics: CELL_METRIC_DEFS.filter(
                (metric) => metric.category === "hydrology" || metric.category === "glaciology"
            ),
        },
        {
            ...CATEGORY_META.ecology_domesticates,
            metrics: CELL_METRIC_DEFS.filter((metric) => metric.category === "domesticates"),
        },
        {
            ...CATEGORY_META.population,
            metrics: [],
        },
        {
            ...CATEGORY_META.polity_system,
            metrics: [],
        },
    ];
}
