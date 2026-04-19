import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
    normalizeOverlayMetric,
    resolveOverlayMetricColor,
    supportsMetricOverlay,
} from "./metric-overlay-style";

describe("metric overlay style", () => {
    it("supports climate and hydrology overlay metrics", () => {
        expect(supportsMetricOverlay("temperature")).toBe(true);
        expect(supportsMetricOverlay("river_flux")).toBe(true);
        expect(supportsMetricOverlay("height")).toBe(false);
    });

    it("normalizes overlay metrics into 0..1", () => {
        expect(normalizeOverlayMetric("temperature", -30)).toBe(0);
        expect(normalizeOverlayMetric("temperature", 45)).toBe(1);
        expect(normalizeOverlayMetric("temperature", 7.5)).toBeCloseTo(0.5, 5);
    });

    it("resolves different colors for different metric values", () => {
        const cold = resolveOverlayMetricColor("temperature", -30, new THREE.Color());
        const hot = resolveOverlayMetricColor("temperature", 45, new THREE.Color());

        expect(cold.r).not.toBeCloseTo(hot.r, 5);
        expect(cold.g).not.toBeCloseTo(hot.g, 5);
        expect(cold.b).not.toBeCloseTo(hot.b, 5);
    });
});
