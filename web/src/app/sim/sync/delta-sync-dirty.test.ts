import { describe, expect, it } from "vitest";
import { applyWorldDeltaToCore } from "./delta-sync";
import { type CoreBuffers } from "./types";

describe("delta sync dirty cells", () => {
    it("collects metric dirty cells from bitmap deltas", () => {
        const core = {
            heightData: new Float32Array([0, 0, 0, 0, 0]),
            temperature: new Float32Array([0, 0, 0, 0, 0]),
        } as unknown as CoreBuffers;

        const result = applyWorldDeltaToCore(core, {
            deltas: [{
                field_kind: "temperature",
                mode: "bitmap",
                dirty_bitmap: [0b00001010],
                f32_data: new Float32Array([1, 2]),
            }],
        });

        expect(result.changes.metric).toBe(true);
        expect(Array.from(result.dirtyCells.metric ?? [])).toEqual([1, 3]);
        expect(Array.from(result.dirtyCells.height ?? [])).toEqual([]);
    });

    it("treats full metric updates as full dirty set", () => {
        const core = {
            heightData: new Float32Array([0, 0, 0]),
            temperature: new Float32Array([0, 0, 0]),
        } as unknown as CoreBuffers;

        const result = applyWorldDeltaToCore(core, {
            deltas: [{
                field_kind: "temperature",
                mode: "full",
                f32_data: new Float32Array([3, 2, 1]),
            }],
        });

        expect(result.changes.metric).toBe(true);
        expect(result.dirtyCells.metric).toBeNull();
    });

    it("collects height dirty cells from range deltas", () => {
        const core = {
            heightData: new Float32Array([0, 0, 0, 0, 0]),
        } as unknown as CoreBuffers;

        const result = applyWorldDeltaToCore(core, {
            deltas: [{
                field_kind: "height",
                mode: "delta",
                ranges: [{ start: 2, end: 4 }],
                f32_data: new Float32Array([0.1, 0.2]),
            }],
        });

        expect(result.changes.height).toBe(true);
        expect(Array.from(result.dirtyCells.height ?? [])).toEqual([2, 3]);
    });
});
