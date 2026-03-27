import {
    CORE_KEY_BY_FIELD_KIND,
    createWorldChangeset,
    markFieldChange,
    type FieldKind,
} from "./constants.js";

type NumericArray = Float32Array | Int32Array | Uint32Array | number[];

interface DeltaRange {
    start: number;
    end: number;
}

interface FieldDelta {
    mode?: "full" | "range";
    field_kind?: FieldKind;
    ranges?: DeltaRange[];
    f32_data?: Float32Array;
    i32_data?: Int32Array;
    u32_data?: Uint32Array;
}

function applyNumericDelta(target: NumericArray, fieldDelta: FieldDelta): boolean {
    const ranges = Array.isArray(fieldDelta?.ranges) ? fieldDelta.ranges : [];
    const values = fieldDelta?.f32_data ?? fieldDelta?.i32_data ?? fieldDelta?.u32_data ?? [];
    const canFastCopy =
        (target instanceof Float32Array || target instanceof Int32Array || target instanceof Uint32Array) &&
        (values instanceof Float32Array || values instanceof Int32Array || values instanceof Uint32Array);

    if (fieldDelta?.mode === "full") {
        const copyLength = Math.min(target.length, values.length);
        if (canFastCopy) {
            (target as Float32Array | Int32Array | Uint32Array).set(
                (values as Float32Array | Int32Array | Uint32Array).subarray(0, copyLength),
                0
            );
            return copyLength > 0;
        }
        for (let i = 0; i < copyLength; i += 1) {
            (target as any)[i] = (values as any)[i];
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
        const copyLength = Math.max(0, Math.min(rangeLength, (values as any).length - offset));
        if (canFastCopy && copyLength > 0) {
            (target as Float32Array | Int32Array | Uint32Array).set(
                (values as Float32Array | Int32Array | Uint32Array).subarray(offset, offset + copyLength),
                start
            );
            offset += rangeLength;
            continue;
        }
        for (let i = 0; i < copyLength; i += 1) {
            (target as any)[start + i] = (values as any)[offset + i];
        }
        offset += rangeLength;
    }
    return ranges.length > 0;
}

export function applyWorldDeltaToCore(core: any, worldDelta: { deltas?: FieldDelta[] }) {
    const changes = createWorldChangeset();
    for (const delta of worldDelta?.deltas ?? []) {
        const fieldKind = delta?.field_kind;
        if (!fieldKind) {
            continue;
        }
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
