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
    | "biome"
    | "river_transport_cost"
    | "crop_adoption_wheat"
    | "crop_adoption_rice"
    | "crop_adoption_maize"
    | "crop_adoption_millet"
    | "crop_adoption_potato"
    | "crop_adoption_cassava"
    | "crop_adoption_sorghum"
    | "crop_adoption_yam"
    | "crop_available_wheat"
    | "crop_available_rice"
    | "crop_available_maize"
    | "crop_available_millet"
    | "crop_available_potato"
    | "crop_available_cassava"
    | "crop_available_sorghum"
    | "crop_available_yam"
    | "livestock_adoption_cattle"
    | "livestock_adoption_horse"
    | "livestock_adoption_sheep"
    | "livestock_adoption_pig"
    | "livestock_adoption_camel"
    | "livestock_available_cattle"
    | "livestock_available_horse"
    | "livestock_available_sheep"
    | "livestock_available_pig"
    | "livestock_available_camel";

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
    "crop_adoption_wheat",
    "crop_adoption_rice",
    "crop_adoption_maize",
    "crop_adoption_millet",
    "crop_adoption_potato",
    "crop_adoption_cassava",
    "crop_adoption_sorghum",
    "crop_adoption_yam",
    "crop_available_wheat",
    "crop_available_rice",
    "crop_available_maize",
    "crop_available_millet",
    "crop_available_potato",
    "crop_available_cassava",
    "crop_available_sorghum",
    "crop_available_yam",
    "livestock_adoption_cattle",
    "livestock_adoption_horse",
    "livestock_adoption_sheep",
    "livestock_adoption_pig",
    "livestock_adoption_camel",
    "livestock_available_cattle",
    "livestock_available_horse",
    "livestock_available_sheep",
    "livestock_available_pig",
    "livestock_available_camel",
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

export interface WorldDirtyCells {
    height: Uint32Array | null;
    metric: Uint32Array | null;
}

export interface WorldDeltaApplyResult {
    changes: WorldChangeset;
    dirtyCells: WorldDirtyCells;
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
    biome: "biome",
    river_transport_cost: "riverTransportCost",
    crop_adoption_wheat: "cropAdoptionWheat",
    crop_adoption_rice: "cropAdoptionRice",
    crop_adoption_maize: "cropAdoptionMaize",
    crop_adoption_millet: "cropAdoptionMillet",
    crop_adoption_potato: "cropAdoptionPotato",
    crop_adoption_cassava: "cropAdoptionCassava",
    crop_adoption_sorghum: "cropAdoptionSorghum",
    crop_adoption_yam: "cropAdoptionYam",
    crop_available_wheat: "cropAvailableWheat",
    crop_available_rice: "cropAvailableRice",
    crop_available_maize: "cropAvailableMaize",
    crop_available_millet: "cropAvailableMillet",
    crop_available_potato: "cropAvailablePotato",
    crop_available_cassava: "cropAvailableCassava",
    crop_available_sorghum: "cropAvailableSorghum",
    crop_available_yam: "cropAvailableYam",
    livestock_adoption_cattle: "livestockAdoptionCattle",
    livestock_adoption_horse: "livestockAdoptionHorse",
    livestock_adoption_sheep: "livestockAdoptionSheep",
    livestock_adoption_pig: "livestockAdoptionPig",
    livestock_adoption_camel: "livestockAdoptionCamel",
    livestock_available_cattle: "livestockAvailableCattle",
    livestock_available_horse: "livestockAvailableHorse",
    livestock_available_sheep: "livestockAvailableSheep",
    livestock_available_pig: "livestockAvailablePig",
    livestock_available_camel: "livestockAvailableCamel",
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
    biome: "metric",
    river_transport_cost: "metric",
    crop_adoption_wheat: "metric",
    crop_adoption_rice: "metric",
    crop_adoption_maize: "metric",
    crop_adoption_millet: "metric",
    crop_adoption_potato: "metric",
    crop_adoption_cassava: "metric",
    crop_adoption_sorghum: "metric",
    crop_adoption_yam: "metric",
    crop_available_wheat: "metric",
    crop_available_rice: "metric",
    crop_available_maize: "metric",
    crop_available_millet: "metric",
    crop_available_potato: "metric",
    crop_available_cassava: "metric",
    crop_available_sorghum: "metric",
    crop_available_yam: "metric",
    livestock_adoption_cattle: "metric",
    livestock_adoption_horse: "metric",
    livestock_adoption_sheep: "metric",
    livestock_adoption_pig: "metric",
    livestock_adoption_camel: "metric",
    livestock_available_cattle: "metric",
    livestock_available_horse: "metric",
    livestock_available_sheep: "metric",
    livestock_available_pig: "metric",
    livestock_available_camel: "metric",
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

export function getChangeKindByFieldKind(fieldKind: FieldKind): keyof WorldChangeset | undefined {
    return CHANGE_KIND_BY_FIELD_KIND[fieldKind];
}
