import { FLOAT32_FIELDS } from "./constants.js";

const OPTIONAL_FIELD_KINDS = new Set([
    "erosion_rate",
    "deposition_rate",
    "evapotranspiration",
    "aridity",
    "river_transport_cost",
    "runoff",
    "ocean_temperature",
]);

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

export function buildCoreBuffers(controller, worldId) {
    const heightData = getFieldData(controller, worldId, "height");
    const cellCount = heightData.length;
    return {
        heightData,
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
        riverTransportCost: getFieldData(controller, worldId, "river_transport_cost", cellCount),
    };
}

function applyNumericDelta(target, fieldDelta) {
    const ranges = Array.isArray(fieldDelta?.ranges) ? fieldDelta.ranges : [];
    const values = fieldDelta?.f32_data ?? fieldDelta?.i32_data ?? [];
    const canFastCopy = typeof target?.set === "function" && ArrayBuffer.isView(values);
    if (fieldDelta?.mode === "full") {
        const copyLength = Math.min(target.length, values.length);
        if (canFastCopy) {
            target.set(values.subarray(0, copyLength), 0);
            return copyLength > 0;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[i] = values[i];
        }
        return copyLength > 0;
    }

    let offset = 0;
    for (const range of ranges) {
        const start = Math.max(0, Math.floor(range?.start ?? 0));
        const end = Math.min(target.length, Math.floor(range?.end ?? 0));
        if (end <= start) {
            continue;
        }
        const rangeLength = end - start;
        const copyLength = Math.max(0, Math.min(rangeLength, values.length - offset));
        if (canFastCopy && copyLength > 0) {
            target.set(values.subarray(offset, offset + copyLength), start);
            offset += rangeLength;
            continue;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[start + i] = values[offset + i];
        }
        offset += rangeLength;
    }
    return ranges.length > 0;
}

function countDeltaCells(delta, targetLength) {
    if (delta?.mode === "full") {
        return targetLength;
    }
    let count = 0;
    for (const range of delta?.ranges ?? []) {
        const start = Math.max(0, Math.floor(range?.start ?? 0));
        const end = Math.min(targetLength, Math.floor(range?.end ?? 0));
        if (end > start) {
            count += end - start;
        }
    }
    return count;
}

export function applyWorldDeltaToCore(core, worldDelta) {
    const changes = {
        height: false,
        heightChangedCount: 0,
        river: false,
        mantleHeat: false,
        metric: false,
    };

    for (const delta of worldDelta?.deltas ?? []) {
        switch (delta?.field_kind) {
        case "height":
            changes.height = applyNumericDelta(core.heightData, delta);
            if (changes.height) {
                changes.heightChangedCount += countDeltaCells(delta, core.heightData.length);
            }
            break;
        case "river_flux":
            changes.river = applyNumericDelta(core.riverFlux, delta) || changes.river;
            break;
        case "river_next":
            changes.river = applyNumericDelta(core.riverNext, delta) || changes.river;
            break;
        case "mantle_heat":
            changes.mantleHeat = applyNumericDelta(core.mantleHeat, delta);
            break;
        case "erosion_rate":
            changes.metric = applyNumericDelta(core.erosionRate, delta) || changes.metric;
            break;
        case "deposition_rate":
            changes.metric = applyNumericDelta(core.depositionRate, delta) || changes.metric;
            break;
        case "temperature":
            changes.metric = applyNumericDelta(core.temperature, delta) || changes.metric;
            break;
        case "precipitation":
            changes.metric = applyNumericDelta(core.precipitation, delta) || changes.metric;
            break;
        case "evapotranspiration":
            changes.metric = applyNumericDelta(core.evapotranspiration, delta) || changes.metric;
            break;
        case "aridity":
            changes.metric = applyNumericDelta(core.aridity, delta) || changes.metric;
            break;
        case "runoff":
            changes.metric = applyNumericDelta(core.runoff, delta) || changes.metric;
            break;
        case "ocean_temperature":
            changes.metric = applyNumericDelta(core.oceanTemperature, delta) || changes.metric;
            break;
        case "river_transport_cost":
            changes.metric = applyNumericDelta(core.riverTransportCost, delta) || changes.metric;
            break;
        default:
            break;
        }
    }
    return changes;
}

export function estimateRiverMaskUpdate(riverNext, riverFlux) {
    let activeSegments = 0;
    for (let i = 0; i < riverNext.length; i += 1) {
        const next = riverNext[i];
        if (next < 0 || next >= riverNext.length) {
            continue;
        }
        if (Number.isFinite(riverFlux[i]) && riverFlux[i] > 0) {
            activeSegments += 1;
        }
    }
    return activeSegments;
}
