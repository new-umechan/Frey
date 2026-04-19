import { describe, it, expect } from "vitest";
import { applyWorldDeltaToCore } from "./delta-sync";
import { getDeltaFieldKindsForView } from "./view-mode";
import { type CoreBuffers } from "./types";

describe("plate_id sync", () => {
    it("includes plate_id when Plate View is selected", () => {
        expect(getDeltaFieldKindsForView({
            viewMode: "metric",
            cellMetric: "plate_id",
        })).toContain("plate_id");
    });

    it("applies u32 plate_id deltas into core plateId buffer", () => {
        const core = {
            plateId: new Uint32Array([0, 1, 2, 3, 4]),
        } as unknown as CoreBuffers;

        const result = applyWorldDeltaToCore(core, {
            deltas: [{
                field_kind: "plate_id",
                mode: "delta",
                ranges: [{ start: 1, end: 3 }],
                u32_data: new Uint32Array([8, 9]),
            }],
        });

        expect(Array.from(core.plateId as Uint32Array)).toEqual([0, 8, 9, 3, 4]);
        expect(result.changes.metric).toBe(true);
    });

    it("applies bitmap deltas into core plateId buffer", () => {
        const core = {
            plateId: new Uint32Array([0, 1, 2, 3, 4, 5]),
        } as unknown as CoreBuffers;

        const result = applyWorldDeltaToCore(core, {
            deltas: [{
                field_kind: "plate_id",
                mode: "bitmap",
                ranges: [],
                dirty_bitmap: [0b00101010],
                u32_data: new Uint32Array([8, 9, 10]),
            }],
        });

        expect(Array.from(core.plateId as Uint32Array)).toEqual([0, 8, 2, 9, 4, 10]);
        expect(result.changes.metric).toBe(true);
    });
});
