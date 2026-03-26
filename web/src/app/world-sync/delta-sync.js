import {
    CORE_KEY_BY_FIELD_KIND,
    createWorldChangeset,
    markFieldChange,
} from "./constants.js";

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

export function applyWorldDeltaToCore(core, worldDelta) {
    const changes = createWorldChangeset();
    for (const delta of worldDelta?.deltas ?? []) {
        const fieldKind = delta?.field_kind;
        const coreKey = CORE_KEY_BY_FIELD_KIND[fieldKind];
        if (!coreKey || !(coreKey in core)) {
            continue;
        }
        const didChange = applyNumericDelta(core[coreKey], delta);
        if (didChange) {
            markFieldChange(changes, fieldKind);
        }
    }
    return changes;
}
