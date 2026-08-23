import type { FieldDeltaResult, ViewDeltaResult } from "./engine-client";

const MAGIC = "FRPB";
const VERSION = 2;

export interface DecodedPlaybackChunk {
    epoch: number;
    tick: number;
    spatialLod: number | null;
    delta: ViewDeltaResult;
}

export class PlaybackCompressionUnsupportedError extends Error {
    constructor() {
        super("zstd DecompressionStream is not supported by this browser");
        this.name = "PlaybackCompressionUnsupportedError";
    }
}

export async function decodePlaybackChunk(encoded: ArrayBuffer): Promise<DecodedPlaybackChunk> {
    const header = new BinaryReader(encoded);
    if (header.readStringBytes(4) !== MAGIC) {
        throw new Error("invalid playback chunk magic");
    }
    const version = header.readU8();
    if (version !== VERSION) {
        throw new Error(`unsupported playback chunk version: ${version}`);
    }
    const epoch = header.readU32();
    const tick = header.readU32();
    const compressedLength = header.readU32();
    const compressed = header.readBytes(compressedLength);
    if (!supportsZstdDecompression()) {
        throw new PlaybackCompressionUnsupportedError();
    }
    const payload = await decompressZstd(compressed);
    return decodePlaybackPayload(payload, epoch, tick);
}

export function decodePlaybackPayload(
    payload: ArrayBuffer,
    epoch: number,
    tick: number,
): DecodedPlaybackChunk {
    const reader = new BinaryReader(payload);
    const headTick = reader.readU32();
    const era = reader.readString();
    const runtimeTickMs = reader.readU32();
    const realYearsPerTick = reader.readF32();
    const budgets = {
        geology: reader.readU32(),
        climate: reader.readU32(),
        ecology: reader.readU32(),
        civilization: reader.readU32(),
    };
    const encodedSpatialLod = reader.readU8();
    const spatialLod = encodedSpatialLod === 0xff ? null : encodedSpatialLod;
    const fieldCount = reader.readU16();
    const deltas: FieldDeltaResult[] = [];
    for (let index = 0; index < fieldCount; index += 1) {
        const fieldKind = reader.readString();
        const mode = decodeMode(reader.readU8());
        const rangeCount = reader.readU16();
        const ranges = [];
        for (let rangeIndex = 0; rangeIndex < rangeCount; rangeIndex += 1) {
            ranges.push({ start: reader.readU32(), end: reader.readU32() });
        }
        const bitmapCount = reader.readU32();
        const dirtyBitmap = bitmapCount > 0 ? reader.readU32Array(bitmapCount) : undefined;
        const dataType = reader.readU8();
        const valueCount = reader.readU32();
        const field: FieldDeltaResult = {
            field_kind: fieldKind,
            mode,
            ranges,
            dirty_bitmap: dirtyBitmap,
        };
        if (dataType === 1) {
            field.f32_data = reader.readF32Array(valueCount);
        } else if (dataType === 2) {
            field.u32_data = reader.readU32Array(valueCount);
        } else if (dataType === 3) {
            field.i32_data = reader.readI32Array(valueCount);
        } else if (dataType !== 0) {
            throw new Error(`unsupported playback field data type: ${dataType}`);
        }
        deltas.push(field);
    }
    reader.assertFullyRead();
    return {
        epoch,
        tick,
        spatialLod,
        delta: {
            world_id: "",
            tick,
            head_tick: headTick,
            era,
            real_years_per_tick: realYearsPerTick,
            runtime_tick_ms: runtimeTickMs,
            budgets,
            deltas,
        },
    };
}

export function supportsZstdDecompression(): boolean {
    try {
        new DecompressionStream("zstd" as CompressionFormat);
        return true;
    } catch {
        return false;
    }
}

async function decompressZstd(compressed: Uint8Array): Promise<ArrayBuffer> {
    const stream = new Blob([compressed]).stream().pipeThrough(
        new DecompressionStream("zstd" as CompressionFormat),
    );
    return await new Response(stream).arrayBuffer();
}

function decodeMode(mode: number): FieldDeltaResult["mode"] {
    if (mode === 0) {
        return "full";
    }
    if (mode === 1) {
        return "delta";
    }
    if (mode === 2) {
        return "bitmap";
    }
    throw new Error(`unsupported playback delta mode: ${mode}`);
}

class BinaryReader {
    private readonly bytes: Uint8Array;
    private readonly view: DataView;
    private offset = 0;

    constructor(buffer: ArrayBuffer | Uint8Array) {
        this.bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
        this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
    }

    readU8(): number {
        this.require(1);
        const value = this.view.getUint8(this.offset);
        this.offset += 1;
        return value;
    }

    readU16(): number {
        this.require(2);
        const value = this.view.getUint16(this.offset, true);
        this.offset += 2;
        return value;
    }

    readU32(): number {
        this.require(4);
        const value = this.view.getUint32(this.offset, true);
        this.offset += 4;
        return value;
    }

    readF32(): number {
        this.require(4);
        const value = this.view.getFloat32(this.offset, true);
        this.offset += 4;
        return value;
    }

    readString(): string {
        return new TextDecoder().decode(this.readBytes(this.readU16()));
    }

    readStringBytes(length: number): string {
        return new TextDecoder().decode(this.readBytes(length));
    }

    readBytes(length: number): Uint8Array {
        this.require(length);
        const value = this.bytes.slice(this.offset, this.offset + length);
        this.offset += length;
        return value;
    }

    readF32Array(length: number): Float32Array {
        return new Float32Array(this.readBytes(length * Float32Array.BYTES_PER_ELEMENT).buffer);
    }

    readU32Array(length: number): Uint32Array {
        return new Uint32Array(this.readBytes(length * Uint32Array.BYTES_PER_ELEMENT).buffer);
    }

    readI32Array(length: number): Int32Array {
        return new Int32Array(this.readBytes(length * Int32Array.BYTES_PER_ELEMENT).buffer);
    }

    assertFullyRead() {
        if (this.offset !== this.bytes.length) {
            throw new Error(`unexpected playback payload bytes: ${this.bytes.length - this.offset}`);
        }
    }

    private require(length: number) {
        if (!Number.isSafeInteger(length) || length < 0 || this.offset + length > this.bytes.length) {
            throw new Error("truncated playback chunk");
        }
    }
}
