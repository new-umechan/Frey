function createNoopPinchFocusController() {
    return {
        reset: () => {},
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
}) {
    const pinchFocusController = globePinchFocusController ?? createNoopPinchFocusController();
    return {
        onPointerDown: (event) => {
            pinchFocusController.onPointerDown(event);
            if (event.pointerType === "touch") {
                plateHover.hidePopup();
            }
        },
        onPointerMove: (event) => {
            pinchFocusController.onPointerMove(event);
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
        },
    };
}
