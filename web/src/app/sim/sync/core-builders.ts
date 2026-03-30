import { type WorldSimController } from "../../../interface/wasm";
import { FLOAT32_FIELDS, OPTIONAL_FIELD_KINDS, type FieldKind } from "./constants";
import { type CoreBuffers, type TypedArray } from "./types";

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

function getFieldData(
    controller: WorldSimController,
    worldId: string,
    fieldKind: FieldKind,
    fallbackCellCount = 0
): TypedArray {
    let response: any = null;
    try {
        response = controller.get_field(worldId, fieldKind, 1);
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

export function buildCoreBuffers(controller: WorldSimController, worldId: string): CoreBuffers {
    const heightData = getFieldData(controller, worldId, "height");
    const cellCount = heightData.length;
    return {
        heightData,
        lakeDepth: getFieldData(controller, worldId, "lake_depth", cellCount),
        riverFlux: getFieldData(controller, worldId, "river_flux", cellCount),
        riverNext: getFieldData(controller, worldId, "river_next", cellCount),
        mantleHeat: getFieldData(controller, worldId, "mantle_heat", cellCount),
        erosionRate: getFieldData(controller, worldId, "erosion_rate", cellCount),
        depositionRate: getFieldData(controller, worldId, "deposition_rate", cellCount),
        temperature: getFieldData(controller, worldId, "temperature", cellCount),
        precipitation: getFieldData(controller, worldId, "precipitation", cellCount),
        evapotranspiration: getFieldData(controller, worldId, "evapotranspiration", cellCount),
        aridity: getFieldData(controller, worldId, "aridity", cellCount),
        runoff: getFieldData(controller, worldId, "runoff", cellCount),
        oceanTemperature: getFieldData(controller, worldId, "ocean_temperature", cellCount),
        windU: getFieldData(controller, worldId, "wind_u", cellCount),
        windV: getFieldData(controller, worldId, "wind_v", cellCount),
        moistureFluxU: getFieldData(controller, worldId, "moisture_flux_u", cellCount),
        moistureFluxV: getFieldData(controller, worldId, "moisture_flux_v", cellCount),
        riverTransportCost: getFieldData(controller, worldId, "river_transport_cost", cellCount),
    };
}
