import {
    CORE_KEY_BY_FIELD_KIND,
    createWorldChangeset,
    getChangeKindByFieldKind,
    markFieldChange,
    type FieldKind,
    type WorldDeltaApplyResult,
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

export function applyWorldDeltaToCore(core: CoreBuffers, worldDelta: { deltas?: FieldDelta[] }): WorldDeltaApplyResult {
    const changes = createWorldChangeset();
    const defaultTargetLength = core.heightData?.length ?? 0;
    const heightDirty = createDirtyAccumulator(defaultTargetLength);
    const metricDirty = createDirtyAccumulator(defaultTargetLength);

    for (const delta of worldDelta?.deltas ?? []) {
        const fieldKind = delta?.field_kind as FieldKind | undefined;
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
            const dirtyIndices = collectDirtyCellIndices(delta, target.length);
            const changeKind = getChangeKindByFieldKind(fieldKind);
            if (changeKind === "height") {
                mergeDirtyCells(heightDirty, dirtyIndices, target.length);
            } else if (changeKind === "metric") {
                mergeDirtyCells(metricDirty, dirtyIndices, target.length);
            }
        }
    }

    return {
        changes,
        dirtyCells: {
            height: changes.height ? finalizeDirtyCells(heightDirty) : new Uint32Array(0),
            metric: changes.metric ? finalizeDirtyCells(metricDirty) : new Uint32Array(0),
        },
    };
}

interface DirtyAccumulator {
    full: boolean;
    flags: Uint8Array | null;
    count: number;
    targetLength: number;
}

function createDirtyAccumulator(targetLength: number): DirtyAccumulator {
    return {
        full: false,
        flags: null,
        count: 0,
        targetLength: Math.max(0, Math.floor(targetLength)),
    };
}

function mergeDirtyCells(accumulator: DirtyAccumulator, dirtyCells: Uint32Array | null, targetLength: number) {
    ensureDirtyCapacity(accumulator, targetLength);
    if (accumulator.full) {
        return;
    }
    if (dirtyCells === null) {
        accumulator.full = true;
        accumulator.flags = null;
        accumulator.count = accumulator.targetLength;
        return;
    }
    if (dirtyCells.length < 1) {
        return;
    }
    if (!accumulator.flags) {
        accumulator.flags = new Uint8Array(accumulator.targetLength);
    }
    for (let i = 0; i < dirtyCells.length; i += 1) {
        const cellId = dirtyCells[i];
        if (cellId >= accumulator.targetLength) {
            continue;
        }
        if (accumulator.flags[cellId] === 1) {
            continue;
        }
        accumulator.flags[cellId] = 1;
        accumulator.count += 1;
    }
}

function ensureDirtyCapacity(accumulator: DirtyAccumulator, targetLength: number) {
    const nextLength = Math.max(0, Math.floor(targetLength));
    if (nextLength <= accumulator.targetLength) {
        return;
    }
    const nextFlags = new Uint8Array(nextLength);
    if (accumulator.flags) {
        nextFlags.set(accumulator.flags.subarray(0, accumulator.flags.length));
    }
    accumulator.flags = nextFlags;
    accumulator.targetLength = nextLength;
}

function finalizeDirtyCells(accumulator: DirtyAccumulator): Uint32Array | null {
    if (accumulator.full) {
        return null;
    }
    if (!accumulator.flags || accumulator.count < 1) {
        return new Uint32Array(0);
    }
    const result = new Uint32Array(accumulator.count);
    let offset = 0;
    for (let i = 0; i < accumulator.flags.length; i += 1) {
        if (accumulator.flags[i] !== 1) {
            continue;
        }
        result[offset] = i;
        offset += 1;
    }
    return result;
}

function collectDirtyCellIndices(delta: FieldDelta, targetLength: number): Uint32Array | null {
    if (delta?.mode === "full") {
        return null;
    }

    if (delta?.mode === "bitmap") {
        const bitmap = delta?.dirty_bitmap;
        if (!bitmap || bitmap.length < 1) {
            return new Uint32Array(0);
        }
        const cells: number[] = [];
        for (let wordIndex = 0; wordIndex < bitmap.length; wordIndex += 1) {
            let word = Number(bitmap[wordIndex] ?? 0) >>> 0;
            while (word !== 0) {
                const bit = Math.clz32(word & -word) ^ 31;
                const cellIndex = wordIndex * 32 + bit;
                if (cellIndex >= targetLength) {
                    break;
                }
                cells.push(cellIndex);
                word &= word - 1;
            }
        }
        return Uint32Array.from(cells);
    }

    const ranges = Array.isArray(delta?.ranges) ? delta.ranges : [];
    if (ranges.length < 1) {
        return new Uint32Array(0);
    }
    const cells: number[] = [];
    for (const range of ranges) {
        const start = Math.max(0, Math.floor(range?.start ?? 0));
        const end = Math.min(targetLength, Math.floor(range?.end ?? 0));
        for (let i = start; i < end; i += 1) {
            cells.push(i);
        }
    }
    return Uint32Array.from(cells);
}
