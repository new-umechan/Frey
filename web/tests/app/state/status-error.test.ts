import { describe, it, expect } from "vitest";
import { formatStatusError } from "../../../src/app/state/status-error";

describe("formatStatusError", () => {
    it("should format Error objects with message", () => {
        const error = new Error("Test error message");
        const result = formatStatusError("Initialization", error);
        expect(result).toBe("Initialization failed: Test error message");
    });

    it("should format string errors", () => {
        const result = formatStatusError("Loading", "Something went wrong");
        expect(result).toBe("Loading failed: Something went wrong");
    });

    it("should format number errors", () => {
        const result = formatStatusError("Operation", 500);
        expect(result).toBe("Operation failed: 500");
    });

    it("should handle undefined phase", () => {
        const error = new Error("Test error");
        const result = formatStatusError(undefined, error);
        expect(result).toBe("Operation failed: Test error");
    });

    it("should handle null phase", () => {
        const error = new Error("Test error");
        const result = formatStatusError(null as any, error);
        expect(result).toBe("Operation failed: Test error");
    });

    it("should handle object errors", () => {
        const error = { code: 500, message: "Internal error" };
        const result = formatStatusError("API", error);
        expect(result).toBe("API failed: [object Object]");
    });

    it("should handle null errors", () => {
        const result = formatStatusError("Test", null);
        expect(result).toBe("Test failed: null");
    });

    it("should handle undefined errors", () => {
        const result = formatStatusError("Test", undefined);
        expect(result).toBe("Test failed: undefined");
    });

    it("should preserve phase text exactly", () => {
        const error = new Error("Error");
        expect(formatStatusError("MyPhase", error)).toBe("MyPhase failed: Error");
        expect(formatStatusError("", error)).toBe(" failed: Error");
    });
});
