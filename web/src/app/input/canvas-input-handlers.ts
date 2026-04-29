import type { GlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller";
import type { CausalExplorationLayer } from "../../gfx/views/causal-exploration-layer";
import type { PlateHoverController } from "./plate-hover";

export interface CanvasInputHandlers {
    onPointerDown: (event: PointerEvent) => void;
    onPointerMove: (event: PointerEvent) => void;
    onPointerUp: (event: PointerEvent) => void;
    onPointerCancel: (event: PointerEvent) => void;
    onWheel: (event: WheelEvent) => boolean;
    onLeave: () => void;
}

function createNoopPinchFocusController(): GlobePinchFocusController {
    return {
        reset: () => {},
        update: () => {},
        onPointerDown: () => {},
        onPointerMove: () => {},
        onPointerUp: () => {},
        onPointerCancel: () => {},
        onWheel: () => false,
    };
}

export function createCanvasInputHandlers({
    plateHover,
    globePinchFocusController,
    causalExplorationLayer,
}: {
    plateHover: PlateHoverController;
    globePinchFocusController: GlobePinchFocusController | null;
    causalExplorationLayer: CausalExplorationLayer;
}): CanvasInputHandlers {
    const pinchFocusController = globePinchFocusController ?? createNoopPinchFocusController();
    return {
        onPointerDown: (event) => {
            pinchFocusController.onPointerDown(event);
            causalExplorationLayer.handlePointerDown(event);
            if (event.pointerType === "touch") {
                plateHover.hidePopup();
            }
        },
        onPointerMove: (event) => {
            pinchFocusController.onPointerMove(event);
            causalExplorationLayer.handlePointerMove(event);
            if (event.pointerType === "touch") {
                return;
            }
            plateHover.updateFromPointer(event);
        },
        onPointerUp: (event) => {
            pinchFocusController.onPointerUp(event);
        },
        onPointerCancel: (event) => {
            pinchFocusController.onPointerCancel(event);
        },
        onWheel: (event) => {
            return pinchFocusController.onWheel(event);
        },
        onLeave: () => {
            pinchFocusController.reset();
            plateHover.hidePopup();
            causalExplorationLayer.handlePointerLeave();
        },
    };
}
