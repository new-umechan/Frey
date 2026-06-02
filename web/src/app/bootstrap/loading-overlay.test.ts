import { describe, expect, it, vi } from "vitest";
import { createLoadingOverlayController } from "./loading-overlay";

function createMockContext() {
    return {
        setTransform: vi.fn(),
        clearRect: vi.fn(),
        beginPath: vi.fn(),
        arc: vi.fn(),
        fill: vi.fn(),
        fillStyle: "",
    } as unknown as CanvasRenderingContext2D;
}

describe("loading overlay controller", () => {
    it("keeps the overlay hidden while initializing and does not draw a placeholder circle", () => {
        const loadingOverlayCanvas = document.createElement("canvas");
        Object.defineProperty(window, "devicePixelRatio", {
            value: 1,
            configurable: true,
        });

        const context = createMockContext();
        Object.defineProperty(loadingOverlayCanvas, "getContext", {
            value: () => context,
        });

        const controller = createLoadingOverlayController({
            loadingOverlayCanvas,
        });

        expect(loadingOverlayCanvas.hidden).toBe(true);

        controller.setWorldInitializing(true);
        controller.render();

        expect(loadingOverlayCanvas.hidden).toBe(true);
        expect(context.arc).not.toHaveBeenCalled();

        controller.setWorldInitializing(false);
        controller.render();

        expect(loadingOverlayCanvas.hidden).toBe(true);
    });
});
