export type FieldKind =
    | "height"
    | "lake_depth"
    | "plate_id"
    | "river_flux"
    | "river_next"
    | "mantle_heat"
    | "erosion_rate"
    | "deposition_rate"
    | "temperature"
    | "precipitation"
    | "evapotranspiration"
    | "aridity"
    | "runoff"
    | "ice_pressure"
    | "ocean_temperature"
    | "wind_u"
    | "wind_v"
    | "moisture_flux_u"
    | "moisture_flux_v"
    | "river_transport_cost";

export const FLOAT32_FIELDS = new Set<FieldKind>([
    "height",
    "lake_depth",
    "river_flux",
    "mantle_heat",
    "erosion_rate",
    "deposition_rate",
    "temperature",
    "precipitation",
    "evapotranspiration",
    "aridity",
    "runoff",
    "ice_pressure",
    "ocean_temperature",
    "wind_u",
    "wind_v",
    "moisture_flux_u",
    "moisture_flux_v",
    "river_transport_cost",
]);

export const OPTIONAL_FIELD_KINDS = new Set<FieldKind>([
    "erosion_rate",
    "deposition_rate",
    "evapotranspiration",
    "aridity",
    "river_transport_cost",
    "runoff",
    "ice_pressure",
    "ocean_temperature",
    "wind_u",
    "wind_v",
    "moisture_flux_u",
    "moisture_flux_v",
]);

export interface WorldChangeset {
    height: boolean;
    river: boolean;
    mantleHeat: boolean;
    metric: boolean;
}

export const WORLD_CHANGESET: WorldChangeset = Object.freeze({
    height: false,
    river: false,
    mantleHeat: false,
    metric: false,
});

export const DELTA_FIELD_KIND_BY_VIEW: Record<string, FieldKind[]> = Object.freeze({
    normal: ["height", "lake_depth", "river_flux", "river_next"],
    metric: ["height", "lake_depth", "river_flux", "river_next"],
});

export const CORE_KEY_BY_FIELD_KIND: Record<FieldKind, string> = Object.freeze({
    height: "heightData",
    lake_depth: "lakeDepth",
    plate_id: "plateId",
    river_flux: "riverFlux",
    river_next: "riverNext",
    mantle_heat: "mantleHeat",
    erosion_rate: "erosionRate",
    deposition_rate: "depositionRate",
    temperature: "temperature",
    precipitation: "precipitation",
    evapotranspiration: "evapotranspiration",
    aridity: "aridity",
    runoff: "runoff",
    ice_pressure: "icePressure",
    ocean_temperature: "oceanTemperature",
    wind_u: "windU",
    wind_v: "windV",
    moisture_flux_u: "moistureFluxU",
    moisture_flux_v: "moistureFluxV",
    river_transport_cost: "riverTransportCost",
});

const CHANGE_KIND_BY_FIELD_KIND: Record<FieldKind, keyof WorldChangeset> = Object.freeze({
    height: "height",
    lake_depth: "height",
    plate_id: "metric",
    river_flux: "river",
    river_next: "river",
    mantle_heat: "mantleHeat",
    erosion_rate: "metric",
    deposition_rate: "metric",
    temperature: "metric",
    precipitation: "metric",
    evapotranspiration: "metric",
    aridity: "metric",
    runoff: "metric",
    ice_pressure: "metric",
    ocean_temperature: "metric",
    wind_u: "metric",
    wind_v: "metric",
    moisture_flux_u: "metric",
    moisture_flux_v: "metric",
    river_transport_cost: "metric",
});

export function createWorldChangeset(): WorldChangeset {
    return { ...WORLD_CHANGESET };
}

export function markFieldChange(changes: WorldChangeset, fieldKind: FieldKind) {
    const changeKey = CHANGE_KIND_BY_FIELD_KIND[fieldKind];
    if (changeKey) {
        changes[changeKey] = true;
    }
}
