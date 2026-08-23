import { describe, expect, it } from "vitest";
import { decodePlaybackPayload } from "./playback-chunk-codec";

function pushU16(output: number[], value: number) {
    output.push(value & 0xff, (value >>> 8) & 0xff);
}

function pushU32(output: number[], value: number) {
    output.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
}

function pushF32(output: number[], value: number) {
    const bytes = new Uint8Array(4);
    new DataView(bytes.buffer).setFloat32(0, value, true);
    output.push(...bytes);
}

function pushString(output: number[], value: string) {
    const bytes = new TextEncoder().encode(value);
    pushU16(output, bytes.length);
    output.push(...bytes);
}

describe("PlaybackChunk payload codec", () => {
    it("range と f32 delta を typed array として復元する", () => {
        const payload: number[] = [];
        pushU32(payload, 1600);
        pushString(payload, "geologic");
        pushU32(payload, 70);
        pushF32(payload, 1_000_000);
        pushU32(payload, 1);
        pushU32(payload, 2);
        pushU32(payload, 3);
        pushU32(payload, 4);
        payload.push(2);
        pushU16(payload, 1);
        pushString(payload, "height");
        payload.push(1);
        pushU16(payload, 1);
        pushU32(payload, 2);
        pushU32(payload, 4);
        pushU32(payload, 0);
        payload.push(1);
        pushU32(payload, 2);
        pushF32(payload, 12.5);
        pushF32(payload, -3.25);

        const decoded = decodePlaybackPayload(new Uint8Array(payload).buffer, 7, 57);

        expect(decoded.epoch).toBe(7);
        expect(decoded.tick).toBe(57);
        expect(decoded.spatialLod).toBe(2);
        expect(decoded.delta).toMatchObject({
            tick: 57,
            head_tick: 1600,
            era: "geologic",
            runtime_tick_ms: 70,
            budgets: { geology: 1, climate: 2, ecology: 3, civilization: 4 },
        });
        expect(decoded.delta.deltas[0]).toMatchObject({
            field_kind: "height",
            mode: "delta",
            ranges: [{ start: 2, end: 4 }],
        });
        expect(decoded.delta.deltas[0].f32_data).toEqual(new Float32Array([12.5, -3.25]));
    });
});
