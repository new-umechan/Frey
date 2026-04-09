import { type EngineClient } from "../../engine/engine-client";
import { FLOAT32_FIELDS, OPTIONAL_FIELD_KINDS, type FieldKind } from "./constants";
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
    return {
        heightData,
        lakeDepth: await getFieldData(controller, worldId, "lake_depth", cellCount),
        plateId: await getFieldData(controller, worldId, "plate_id", cellCount),
        riverFlux: await getFieldData(controller, worldId, "river_flux", cellCount),
        riverNext: await getFieldData(controller, worldId, "river_next", cellCount),
        mantleHeat: await getFieldData(controller, worldId, "mantle_heat", cellCount),
        erosionRate: await getFieldData(controller, worldId, "erosion_rate", cellCount),
        depositionRate: await getFieldData(controller, worldId, "deposition_rate", cellCount),
        temperature: await getFieldData(controller, worldId, "temperature", cellCount),
        precipitation: await getFieldData(controller, worldId, "precipitation", cellCount),
        evapotranspiration: await getFieldData(controller, worldId, "evapotranspiration", cellCount),
        aridity: await getFieldData(controller, worldId, "aridity", cellCount),
        runoff: await getFieldData(controller, worldId, "runoff", cellCount),
        icePressure: await getFieldData(controller, worldId, "ice_pressure", cellCount),
        oceanTemperature: await getFieldData(controller, worldId, "ocean_temperature", cellCount),
        windU: await getFieldData(controller, worldId, "wind_u", cellCount),
        windV: await getFieldData(controller, worldId, "wind_v", cellCount),
        moistureFluxU: await getFieldData(controller, worldId, "moisture_flux_u", cellCount),
        moistureFluxV: await getFieldData(controller, worldId, "moisture_flux_v", cellCount),
        riverTransportCost: await getFieldData(controller, worldId, "river_transport_cost", cellCount),
    };
}
