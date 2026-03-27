import {
    CORE_KEY_BY_FIELD_KIND,
    FLOAT32_FIELDS,
    OPTIONAL_FIELD_KINDS,
    createWorldChangeset,
    markFieldChange,
} from "./constants.js";

function createFallbackFieldData(fieldKind, fallbackCellCount) {
    const count = Math.max(0, Math.floor(fallbackCellCount || 0));
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(count);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(count);
    }
    return new Int32Array(count);
}

function getFieldData(controller, worldId, fieldKind, fallbackCellCount = 0) {
    let response = null;
    try {
        response = controller.get_field(worldId, fieldKind, 1);
    } catch (error) {
        if (OPTIONAL_FIELD_KINDS.has(fieldKind)) {
            console.warn(`[world-sync] optional field fallback: ${fieldKind}`, error);
            return createFallbackFieldData(fieldKind, fallbackCellCount);
        }
        throw error;
    }
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
}

export function fetchCoreFields(worldSimController, worldId) {
    const heightData = getFieldData(worldSimController, worldId, "height");
    const cellCount = heightData.length;
    return {
        heightData,
        plateId: getFieldData(worldSimController, worldId, "plate_id", cellCount),
        riverFlux: getFieldData(worldSimController, worldId, "river_flux", cellCount),
        riverNext: getFieldData(worldSimController, worldId, "river_next", cellCount),
        mantleHeat: getFieldData(worldSimController, worldId, "mantle_heat", cellCount),
        erosionRate: getFieldData(worldSimController, worldId, "erosion_rate", cellCount),
        depositionRate: getFieldData(worldSimController, worldId, "deposition_rate", cellCount),
        temperature: getFieldData(worldSimController, worldId, "temperature", cellCount),
        precipitation: getFieldData(worldSimController, worldId, "precipitation", cellCount),
        evapotranspiration: getFieldData(worldSimController, worldId, "evapotranspiration", cellCount),
        aridity: getFieldData(worldSimController, worldId, "aridity", cellCount),
        runoff: getFieldData(worldSimController, worldId, "runoff", cellCount),
        oceanTemperature: getFieldData(worldSimController, worldId, "ocean_temperature", cellCount),
        windU: getFieldData(worldSimController, worldId, "wind_u", cellCount),
        windV: getFieldData(worldSimController, worldId, "wind_v", cellCount),
        moistureFluxU: getFieldData(worldSimController, worldId, "moisture_flux_u", cellCount),
        moistureFluxV: getFieldData(worldSimController, worldId, "moisture_flux_v", cellCount),
        riverTransportCost: getFieldData(worldSimController, worldId, "river_transport_cost", cellCount),
    };
}

function applyFieldSnapshotToCore(core, fieldKind, values, changes) {
    const coreKey = CORE_KEY_BY_FIELD_KIND[fieldKind];
    if (!coreKey || !(coreKey in core)) {
        return;
    }
    core[coreKey] = values;
    markFieldChange(changes, fieldKind);
}

export function syncVisibleCoreFieldsFromController({
    worldSimController,
    worldId,
    core,
    fieldKinds,
}) {
    const changes = createWorldChangeset();
    const uniqueFieldKinds = Array.from(new Set(fieldKinds ?? []));
    for (const fieldKind of uniqueFieldKinds) {
        const values = getFieldData(
            worldSimController,
            worldId,
            fieldKind,
            core?.heightData?.length ?? 0,
        );
        applyFieldSnapshotToCore(core, fieldKind, values, changes);
    }
    return changes;
}
