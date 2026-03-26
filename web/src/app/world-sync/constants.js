export const FLOAT32_FIELDS = new Set([
    "height",
    "river_flux",
    "mantle_heat",
    "erosion_rate",
    "deposition_rate",
    "temperature",
    "precipitation",
    "evapotranspiration",
    "aridity",
    "runoff",
    "ocean_temperature",
    "river_transport_cost",
]);

export const OPTIONAL_FIELD_KINDS = new Set([
    "erosion_rate",
    "deposition_rate",
    "evapotranspiration",
    "aridity",
    "river_transport_cost",
    "runoff",
    "ocean_temperature",
]);

export const WORLD_CHANGESET = Object.freeze({
    height: false,
    river: false,
    mantleHeat: false,
    metric: false,
});

export const DELTA_FIELD_KIND_BY_VIEW = Object.freeze({
    normal: ["height", "river_flux", "river_next"],
    metric: ["height", "river_flux", "river_next"],
});

export const CORE_KEY_BY_FIELD_KIND = Object.freeze({
    height: "heightData",
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
    ocean_temperature: "oceanTemperature",
    river_transport_cost: "riverTransportCost",
});

export const CHANGE_KIND_BY_FIELD_KIND = Object.freeze({
    height: "height",
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
    ocean_temperature: "metric",
    river_transport_cost: "metric",
});

export function createWorldChangeset() {
    return { ...WORLD_CHANGESET };
}

export function markFieldChange(changes, fieldKind) {
    const changeKey = CHANGE_KIND_BY_FIELD_KIND[fieldKind];
    if (changeKey) {
        changes[changeKey] = true;
    }
}
