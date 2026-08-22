import type {
    FieldDeltaResult,
    FieldResult,
    MetricsResult,
    TimelineStateResult,
    ViewDeltaResult,
} from "./engine-client";

const DEFAULT_EXACT_CAPACITY = 8;
const DEFAULT_COARSE_CAPACITY = 16;

type NumericFieldData = Float32Array | Uint32Array | Int32Array;

export interface CachedTickFrame {
    tick: number;
    headTick: number;
    metrics: MetricsResult;
    timeline: TimelineStateResult;
    fields: Map<string, FieldResult>;
    preview: boolean;
}

interface StreamFrameMessage {
    tick: number;
    metrics: MetricsResult;
    timeline: TimelineStateResult;
    frame: ViewDeltaResult;
}

export class TimelinePrefetchCache {
    private readonly exactCapacity: number;
    private readonly coarseCapacity: number;
    private readonly exactFrames = new Map<number, CachedTickFrame>();
    private readonly coarseFrames = new Map<number, CachedTickFrame>();

    constructor(exactCapacity = DEFAULT_EXACT_CAPACITY, coarseCapacity = DEFAULT_COARSE_CAPACITY) {
        this.exactCapacity = Math.max(1, Math.floor(exactCapacity));
        this.coarseCapacity = Math.max(1, Math.floor(coarseCapacity));
    }

    acceptExactAnchor(message: StreamFrameMessage): CachedTickFrame {
        const frame = frameFromFullDelta(message, false);
        this.insertBounded(this.exactFrames, frame, this.exactCapacity);
        return frame;
    }

    acceptExactDelta(tick: number, delta: ViewDeltaResult): CachedTickFrame | null {
        const previous = this.exactFrames.get(tick - 1);
        if (!previous) {
            return null;
        }
        const frame = applyDelta(previous, tick, delta);
        this.insertBounded(this.exactFrames, frame, this.exactCapacity);
        return frame;
    }

    acceptCoarseFrame(message: StreamFrameMessage): CachedTickFrame {
        const frame = frameFromFullDelta(message, true);
        this.insertBounded(this.coarseFrames, frame, this.coarseCapacity);
        return frame;
    }

    getExact(tick: number): CachedTickFrame | null {
        return this.exactFrames.get(tick) ?? null;
    }

    getNearestExact(tick: number): CachedTickFrame | null {
        return nearestFrame(this.exactFrames, tick);
    }

    getNearestCoarse(tick: number): CachedTickFrame | null {
        return nearestFrame(this.coarseFrames, tick);
    }

    composeCoarsePreview(tick: number, base: CachedTickFrame): CachedTickFrame | null {
        const coarse = this.getNearestCoarse(tick);
        if (!coarse) {
            return null;
        }
        const fields = new Map(base.fields);
        for (const [fieldKind, field] of coarse.fields) {
            fields.set(fieldKind, field);
        }
        return {
            tick,
            headTick: Math.max(base.headTick, coarse.headTick),
            metrics: {
                ...coarse.metrics,
                world_id: base.metrics.world_id,
                tick,
            },
            timeline: {
                ...coarse.timeline,
                world_id: base.timeline.world_id,
                current_tick: tick,
            },
            fields,
            preview: true,
        };
    }

    exactTicks(): number[] {
        return [...this.exactFrames.keys()].sort((a, b) => a - b);
    }

    coarseTicks(): number[] {
        return [...this.coarseFrames.keys()].sort((a, b) => a - b);
    }

    private insertBounded(
        frames: Map<number, CachedTickFrame>,
        frame: CachedTickFrame,
        capacity: number,
    ) {
        frames.delete(frame.tick);
        frames.set(frame.tick, frame);
        while (frames.size > capacity) {
            const oldestTick = frames.keys().next().value as number | undefined;
            if (oldestTick === undefined) {
                break;
            }
            frames.delete(oldestTick);
        }
    }
}

export function fieldFromCachedFrame(frame: CachedTickFrame, fieldKind: string): FieldResult | null {
    return frame.fields.get(fieldKind) ?? null;
}

export function viewDeltaFromCachedFrame(
    frame: CachedTickFrame,
    includeFields?: string[],
): ViewDeltaResult {
    const include = includeFields ? new Set(includeFields) : null;
    const deltas: FieldDeltaResult[] = [];
    for (const [fieldKind, field] of frame.fields) {
        if (include && !include.has(fieldKind)) {
            continue;
        }
        deltas.push({
            field_kind: fieldKind,
            mode: "full",
            ranges: [],
            f32_data: field.f32_data as Float32Array | undefined,
            u32_data: field.u32_data as Uint32Array | undefined,
            i32_data: field.i32_data as Int32Array | undefined,
        });
    }
    return {
        world_id: frame.metrics.world_id,
        tick: frame.tick,
        head_tick: frame.headTick,
        era: frame.metrics.era,
        real_years_per_tick: frame.metrics.real_years_per_tick,
        runtime_tick_ms: frame.metrics.runtime_tick_ms,
        budgets: frame.metrics.budgets,
        deltas,
    };
}

function frameFromFullDelta(message: StreamFrameMessage, preview: boolean): CachedTickFrame {
    const fields = new Map<string, FieldResult>();
    for (const delta of message.frame.deltas ?? []) {
        const data = typedDataFromDelta(delta);
        if (!data) {
            continue;
        }
        fields.set(delta.field_kind, fieldResult(delta.field_kind, data));
    }
    return {
        tick: message.tick,
        headTick: sanitizeTick(message.frame.head_tick ?? message.timeline.head_tick),
        metrics: message.metrics,
        timeline: message.timeline,
        fields,
        preview,
    };
}

function applyDelta(previous: CachedTickFrame, tick: number, delta: ViewDeltaResult): CachedTickFrame {
    const fields = new Map(previous.fields);
    for (const fieldDelta of delta.deltas ?? []) {
        const existing = fields.get(fieldDelta.field_kind);
        const source = numericDataFromField(existing);
        if (!source) {
            const fullData = typedDataFromDelta(fieldDelta);
            if (fullData) {
                fields.set(fieldDelta.field_kind, fieldResult(fieldDelta.field_kind, fullData));
            }
            continue;
        }
        const target = source.slice() as NumericFieldData;
        applyNumericDelta(target, fieldDelta);
        fields.set(fieldDelta.field_kind, fieldResult(fieldDelta.field_kind, target));
    }
    return {
        tick,
        headTick: sanitizeTick(delta.head_tick ?? previous.headTick),
        metrics: {
            ...previous.metrics,
            tick,
            era: delta.era,
            real_years_per_tick: delta.real_years_per_tick,
            runtime_tick_ms: delta.runtime_tick_ms,
            budgets: delta.budgets,
        },
        timeline: {
            ...previous.timeline,
            current_tick: tick,
            head_tick: sanitizeTick(delta.head_tick ?? previous.timeline.head_tick),
        },
        fields,
        preview: false,
    };
}

function applyNumericDelta(target: NumericFieldData, delta: FieldDeltaResult) {
    const values = typedDataFromDelta(delta);
    if (!values) {
        return;
    }
    if (delta.mode === "full") {
        target.set(values.subarray(0, target.length));
        return;
    }
    if (delta.mode === "bitmap") {
        let valueOffset = 0;
        for (let wordIndex = 0; wordIndex < (delta.dirty_bitmap?.length ?? 0); wordIndex += 1) {
            let word = Number(delta.dirty_bitmap?.[wordIndex] ?? 0) >>> 0;
            while (word !== 0 && valueOffset < values.length) {
                const bit = Math.clz32(word & -word) ^ 31;
                const cellIndex = wordIndex * 32 + bit;
                if (cellIndex >= target.length) {
                    return;
                }
                target[cellIndex] = values[valueOffset];
                valueOffset += 1;
                word &= word - 1;
            }
        }
        return;
    }
    let valueOffset = 0;
    for (const range of delta.ranges ?? []) {
        const start = Math.max(0, Math.floor(range.start));
        const end = Math.min(target.length, Math.floor(range.end));
        const copyLength = Math.min(end - start, values.length - valueOffset);
        if (copyLength > 0) {
            target.set(values.subarray(valueOffset, valueOffset + copyLength), start);
        }
        valueOffset += Math.max(0, end - start);
    }
}

function typedDataFromDelta(delta: FieldDeltaResult): NumericFieldData | null {
    if (delta.f32_data) {
        return delta.f32_data instanceof Float32Array
            ? delta.f32_data
            : new Float32Array(delta.f32_data);
    }
    if (delta.u32_data) {
        return delta.u32_data instanceof Uint32Array
            ? delta.u32_data
            : new Uint32Array(delta.u32_data);
    }
    if (delta.i32_data) {
        return delta.i32_data instanceof Int32Array
            ? delta.i32_data
            : new Int32Array(delta.i32_data);
    }
    return null;
}

function numericDataFromField(field: FieldResult | undefined): NumericFieldData | null {
    const data = field?.f32_data ?? field?.u32_data ?? field?.i32_data;
    return data instanceof Float32Array || data instanceof Uint32Array || data instanceof Int32Array
        ? data
        : null;
}

function fieldResult(fieldKind: string, data: NumericFieldData): FieldResult {
    return {
        field_kind: fieldKind,
        stride: 1,
        cell_count: data.length,
        sampled_count: data.length,
        f32_data: data instanceof Float32Array ? data : undefined,
        u32_data: data instanceof Uint32Array ? data : undefined,
        i32_data: data instanceof Int32Array ? data : undefined,
    };
}

function nearestFrame(frames: Map<number, CachedTickFrame>, tick: number): CachedTickFrame | null {
    let nearest: CachedTickFrame | null = null;
    let nearestDistance = Number.POSITIVE_INFINITY;
    for (const frame of frames.values()) {
        const distance = Math.abs(frame.tick - tick);
        if (distance < nearestDistance) {
            nearest = frame;
            nearestDistance = distance;
        }
    }
    return nearest;
}

function sanitizeTick(value: unknown): number {
    const tick = Math.floor(Number(value));
    return Number.isFinite(tick) && tick >= 0 ? tick : 0;
}
