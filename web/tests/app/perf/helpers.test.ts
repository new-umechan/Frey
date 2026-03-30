import { describe, it, expect, vi } from "vitest";
import {
    defaultNowMs,
    roundMs,
    roundRatio,
    formatError,
    getDeltaFieldKindsForProfile,
} from "../../../src/app/perf/helpers";

describe("perf/helpers", () => {
    describe("defaultNowMs", () => {
        it("should use performance.now when available", () => {
            const originalPerformance = globalThis.performance;
            const mockNow = vi.fn(() => 12345.678);
            globalThis.performance = { now: mockNow } as any;

            const result = defaultNowMs();
            expect(mockNow).toHaveBeenCalled();
            expect(result).toBe(12345.678);

            globalThis.performance = originalPerformance;
        });

        it("should fall back to Date.now when performance is not available", () => {
            const originalPerformance = globalThis.performance;
            const originalDateNow = Date.now;
            
            // Mock performance to be undefined
            (globalThis as any).performance = undefined;
            Date.now = vi.fn(() => 1000000);

            const result = defaultNowMs();
            expect(result).toBe(1000000);

            globalThis.performance = originalPerformance;
            Date.now = originalDateNow;
        });
    });

    describe("roundMs", () => {
        it("should round to 3 decimal places", () => {
            expect(roundMs(1.234567)).toBe(1.235);
            expect(roundMs(1.234444)).toBe(1.234);
            expect(roundMs(0.0005)).toBe(0.001);
        });

        it("should return 0 for non-finite values", () => {
            expect(roundMs(NaN)).toBe(0);
            expect(roundMs(Infinity)).toBe(0);
            expect(roundMs(-Infinity)).toBe(0);
        });

        it("should handle zero and negative values", () => {
            expect(roundMs(0)).toBe(0);
            expect(roundMs(-1.2345)).toBe(-1.234);
        });
    });

    describe("roundRatio", () => {
        it("should round to 6 decimal places", () => {
            expect(roundRatio(0.123456789)).toBe(0.123457);
            expect(roundRatio(0.9999994)).toBe(0.999999);
            expect(roundRatio(0.9999996)).toBe(1);
        });

        it("should return 0 for non-finite values", () => {
            expect(roundRatio(NaN)).toBe(0);
            expect(roundRatio(Infinity)).toBe(0);
            expect(roundRatio(-Infinity)).toBe(0);
        });

        it("should handle zero and negative values", () => {
            expect(roundRatio(0)).toBe(0);
            expect(roundRatio(-0.123456789)).toBe(-0.123457);
        });
    });

    describe("formatError", () => {
        it("should format Error objects with name and message", () => {
            const error = new Error("Test message");
            expect(formatError(error)).toBe("Error: Test message");
        });

        it("should format TypeError objects", () => {
            const error = new TypeError("Type mismatch");
            expect(formatError(error)).toBe("TypeError: Type mismatch");
        });

        it("should convert non-Error values to string", () => {
            expect(formatError("string error")).toBe("string error");
            expect(formatError(500)).toBe("500");
            expect(formatError(null)).toBe("null");
            expect(formatError(undefined)).toBe("undefined");
            expect(formatError({ message: "obj error" })).toBe("[object Object]");
        });
    });

    describe("getDeltaFieldKindsForProfile", () => {
        it("should return metric field for metric view mode", () => {
            const profile = { viewMode: "metric", cellMetric: "height" };
            const result = getDeltaFieldKindsForProfile(profile);
            expect(result).toContain("height");
            expect(result).toContain("river_flux");
            expect(result).toContain("river_next");
        });

        it("should return metric field for temperature", () => {
            const profile = { viewMode: "metric", cellMetric: "temperature" };
            const result = getDeltaFieldKindsForProfile(profile);
            expect(result).toContain("temperature");
        });

        it("should return default fields for normal view mode", () => {
            const profile = { viewMode: "normal" };
            const result = getDeltaFieldKindsForProfile(profile);
            expect(Array.isArray(result)).toBe(true);
        });

        it("should return default fields for unknown view mode", () => {
            const profile = { viewMode: "unknown" };
            const result = getDeltaFieldKindsForProfile(profile);
            expect(Array.isArray(result)).toBe(true);
        });

        it("should handle undefined profile", () => {
            const result = getDeltaFieldKindsForProfile(undefined);
            expect(Array.isArray(result)).toBe(true);
        });

        it("should handle null profile", () => {
            const result = getDeltaFieldKindsForProfile(null as any);
            expect(Array.isArray(result)).toBe(true);
        });

        it("should handle empty profile", () => {
            const result = getDeltaFieldKindsForProfile({});
            expect(Array.isArray(result)).toBe(true);
        });
    });
});
