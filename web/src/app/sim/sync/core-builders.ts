import { type EngineClient } from "../../engine/engine-client";
import { CORE_KEY_BY_FIELD_KIND, FLOAT32_FIELDS, OPTIONAL_FIELD_KINDS, type FieldKind } from "./constants";
import { type CoreBuffers, type TypedArray } from "./types";

interface FieldResponse {
    f32_data?: Float32Array;
    i32_data?: Int32Array;
    u32_data?: Uint32Array;
}

function createFieldData(fieldKind: FieldKind, cellCount: number): TypedArray {
    const count = Math.max(0, Math.floor(cellCount || 0));
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(count);
    }
    if (fieldKind === ("plate_id" as FieldKind)) {
        return new Uint32Array(count);
    }
    return new Int32Array(count);
}

async function getFieldData(
    controller: EngineClient,
    worldId: string,
    fieldKind: FieldKind,
    fallbackCellCount = 0
): Promise<TypedArray> {
    let response: FieldResponse | null = null;
    try {
        response = await controller.get_field(worldId, fieldKind, 1) as FieldResponse;
    } catch (error) {
        if (OPTIONAL_FIELD_KINDS.has(fieldKind)) {
            return createFieldData(fieldKind, fallbackCellCount);
        }
        throw error;
    }
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === ("plate_id" as FieldKind)) {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
}

export async function buildCoreBuffers(controller: EngineClient, worldId: string): Promise<CoreBuffers> {
    const heightData = await getFieldData(controller, worldId, "height");
    const cellCount = heightData.length;
    const fieldRequests = new Map<FieldKind, Promise<TypedArray>>(
        (Object.keys(CORE_KEY_BY_FIELD_KIND) as FieldKind[])
            .filter((fieldKind) => fieldKind !== "height")
            .map((fieldKind) => [
                fieldKind,
                getFieldData(controller, worldId, fieldKind, cellCount),
            ]),
    );
    return {
        heightData,
        lakeDepth: await fieldRequests.get("lake_depth")!,
        plateId: await fieldRequests.get("plate_id")!,
        riverFlux: await fieldRequests.get("river_flux")!,
        riverNext: await fieldRequests.get("river_next")!,
        mantleHeat: await fieldRequests.get("mantle_heat")!,
        erosionRate: await fieldRequests.get("erosion_rate")!,
        depositionRate: await fieldRequests.get("deposition_rate")!,
        temperature: await fieldRequests.get("temperature")!,
        precipitation: await fieldRequests.get("precipitation")!,
        evapotranspiration: await fieldRequests.get("evapotranspiration")!,
        aridity: await fieldRequests.get("aridity")!,
        runoff: await fieldRequests.get("runoff")!,
        icePressure: await fieldRequests.get("ice_pressure")!,
        oceanTemperature: await fieldRequests.get("ocean_temperature")!,
        windU: await fieldRequests.get("wind_u")!,
        windV: await fieldRequests.get("wind_v")!,
        moistureFluxU: await fieldRequests.get("moisture_flux_u")!,
        moistureFluxV: await fieldRequests.get("moisture_flux_v")!,
        biome: await fieldRequests.get("biome")!,
        riverTransportCost: await fieldRequests.get("river_transport_cost")!,
        cropAdoptionWheat: await fieldRequests.get("crop_adoption_wheat")!,
        cropAdoptionRice: await fieldRequests.get("crop_adoption_rice")!,
        cropAdoptionMaize: await fieldRequests.get("crop_adoption_maize")!,
        cropAdoptionMillet: await fieldRequests.get("crop_adoption_millet")!,
        cropAdoptionPotato: await fieldRequests.get("crop_adoption_potato")!,
        cropAdoptionCassava: await fieldRequests.get("crop_adoption_cassava")!,
        cropAdoptionSorghum: await fieldRequests.get("crop_adoption_sorghum")!,
        cropAdoptionYam: await fieldRequests.get("crop_adoption_yam")!,
        cropAvailableWheat: await fieldRequests.get("crop_available_wheat")!,
        cropAvailableRice: await fieldRequests.get("crop_available_rice")!,
        cropAvailableMaize: await fieldRequests.get("crop_available_maize")!,
        cropAvailableMillet: await fieldRequests.get("crop_available_millet")!,
        cropAvailablePotato: await fieldRequests.get("crop_available_potato")!,
        cropAvailableCassava: await fieldRequests.get("crop_available_cassava")!,
        cropAvailableSorghum: await fieldRequests.get("crop_available_sorghum")!,
        cropAvailableYam: await fieldRequests.get("crop_available_yam")!,
        livestockAdoptionCattle: await fieldRequests.get("livestock_adoption_cattle")!,
        livestockAdoptionHorse: await fieldRequests.get("livestock_adoption_horse")!,
        livestockAdoptionSheep: await fieldRequests.get("livestock_adoption_sheep")!,
        livestockAdoptionPig: await fieldRequests.get("livestock_adoption_pig")!,
        livestockAdoptionCamel: await fieldRequests.get("livestock_adoption_camel")!,
        livestockAvailableCattle: await fieldRequests.get("livestock_available_cattle")!,
        livestockAvailableHorse: await fieldRequests.get("livestock_available_horse")!,
        livestockAvailableSheep: await fieldRequests.get("livestock_available_sheep")!,
        livestockAvailablePig: await fieldRequests.get("livestock_available_pig")!,
        livestockAvailableCamel: await fieldRequests.get("livestock_available_camel")!,
    };
}
