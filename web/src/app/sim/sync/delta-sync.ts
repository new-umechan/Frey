import {
    CORE_KEY_BY_FIELD_KIND,
    createWorldChangeset,
    markFieldChange,
    type WorldChangeset,
} from "./constants";
import { type CoreBuffers, type TypedArray } from "./types";
import { type FieldDelta } from "../../perf/world-core";

type NumericArray = TypedArray | number[];

function applyNumericDelta(target: NumericArray, fieldDelta: FieldDelta): boolean {
    const ranges = Array.isArray(fieldDelta?.ranges) ? fieldDelta.ranges : [];
    const values: ArrayLike<number> = fieldDelta?.f32_data ?? fieldDelta?.i32_data ?? fieldDelta?.u32_data ?? [];
    const canFastCopy =
        (target instanceof Float32Array || target instanceof Int32Array || target instanceof Uint32Array) &&
        (values instanceof Float32Array || values instanceof Int32Array || values instanceof Uint32Array);

    if (fieldDelta?.mode === "full") {
        const copyLength = Math.min(target.length, values.length);
        if (canFastCopy) {
            (target as TypedArray).set(
                (values as TypedArray).subarray(0, copyLength),
                0
            );
            return copyLength > 0;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[i] = Number(values[i] ?? 0);
        }
        return copyLength > 0;
    }

    if (fieldDelta?.mode === "bitmap") {
        const bitmap = fieldDelta?.dirty_bitmap;
        if (!bitmap || bitmap.length === 0) {
            return false;
        }
        let valueOffset = 0;
        for (let wordIndex = 0; wordIndex < bitmap.length; wordIndex += 1) {
            let word = Number(bitmap[wordIndex] ?? 0) >>> 0;
            while (word !== 0) {
                const bit = Math.clz32(word & -word) ^ 31;
                const cellIndex = wordIndex * 32 + bit;
                if (cellIndex >= target.length || valueOffset >= values.length) {
                    return valueOffset > 0;
                }
                target[cellIndex] = Number(values[valueOffset] ?? 0);
                valueOffset += 1;
                word &= word - 1;
            }
        }
        return valueOffset > 0;
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
            (target as TypedArray).set(
                (values as TypedArray).subarray(offset, offset + copyLength),
                start
            );
            offset += rangeLength;
            continue;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[start + i] = Number(values[offset + i] ?? 0);
        }
        offset += rangeLength;
    }
    return ranges.length > 0;
}

export function applyWorldDeltaToCore(core: CoreBuffers, worldDelta: { deltas?: FieldDelta[] }): WorldChangeset {
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
        const target = core[coreKey];
        if (!target || !(target instanceof Float32Array || target instanceof Int32Array || target instanceof Uint32Array)) {
            continue;
        }
        const didChange = applyNumericDelta(target, delta);
        if (didChange) {
            markFieldChange(changes, fieldKind);
        }
    }
    return changes;
}
