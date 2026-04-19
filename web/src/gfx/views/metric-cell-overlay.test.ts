import { describe, expect, it } from "vitest";
import { createMetricCellOverlayLayer } from "./metric-cell-overlay";

describe("metric cell overlay", () => {
    it("applies metric displacement only to vertices with lift=1", () => {
        const layer = createMetricCellOverlayLayer({
            positions: new Float32Array([
                1, 0, 0,
                0, 1, 0,
                0, 0, 1,
            ]),
            cellIds: new Uint32Array([0, 0, 0]),
            lift: new Float32Array([1, 1, 0]),
        });

        layer.update(
            new Float32Array([0]),
            new Float32Array([45]),
            "temperature",
        );

        const positions = layer.mesh.geometry.getAttribute("position").array as Float32Array;
        expect(positions[0]).toBeCloseTo(1.064, 6);
        expect(positions[4]).toBeCloseTo(1.064, 6);
        expect(positions[8]).toBeCloseTo(1.004, 6);
    });

    it("updates only dirty cells when dirty cell ids are provided", () => {
        const layer = createMetricCellOverlayLayer({
            positions: new Float32Array([
                1, 0, 0,
                0, 1, 0,
                0, 0, 1,
                -1, 0, 0,
                0, -1, 0,
                0, 0, -1,
            ]),
            cellIds: new Uint32Array([0, 0, 0, 1, 1, 1]),
            lift: new Float32Array([1, 1, 1, 1, 1, 1]),
        });

        layer.update(
            new Float32Array([0, 0]),
            new Float32Array([-30, 45]),
            "temperature",
            new Uint32Array([1]),
        );

        const positions = layer.mesh.geometry.getAttribute("position").array as Float32Array;
        expect(positions[0]).toBeCloseTo(1.0, 6);
        expect(positions[3]).toBeCloseTo(0.0, 6);
        expect(positions[9]).toBeCloseTo(-1.064, 6);
        expect(positions[13]).toBeCloseTo(-1.064, 6);
    });
});
