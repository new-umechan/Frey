import { describe, expect, it, vi } from "vitest";
import * as THREE from "three";
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
    it("keeps the overlay hidden when not initializing and never requires pointer input", () => {
        const loadingOverlayCanvas = document.createElement("canvas");
        const viewportPanel = document.createElement("div");
        const sphere = new THREE.Mesh(new THREE.SphereGeometry(1, 8, 8));
        const camera = new THREE.PerspectiveCamera(42, 400 / 300, 0.1, 100);
        camera.position.set(0, 0, 3.2);
        camera.lookAt(0, 0, 0);
        camera.updateProjectionMatrix();

        Object.defineProperty(viewportPanel, "getBoundingClientRect", {
            value: () => ({ width: 400, height: 300 }),
        });
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
            viewportPanel,
            sphere,
            getCamera: () => camera,
        });

        expect(loadingOverlayCanvas.hidden).toBe(true);

        controller.setWorldInitializing(true);
        controller.render();

        expect(loadingOverlayCanvas.hidden).toBe(false);
        expect(context.arc).toHaveBeenCalled();

        controller.setWorldInitializing(false);
        controller.render();

        expect(loadingOverlayCanvas.hidden).toBe(true);
    });
});
