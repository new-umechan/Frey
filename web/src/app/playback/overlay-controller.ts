interface PlaybackOverlayOptions {
    overlay: HTMLElement;
    idleMs: number;
}

export function createPlaybackOverlayController({ overlay, idleMs }: PlaybackOverlayOptions) {
    let hideTimerId: number | null = null;

    function clearHideTimer() {
        if (hideTimerId !== null) {
            window.clearTimeout(hideTimerId);
            hideTimerId = null;
        }
    }

    function isStickyVisible() {
        const activeElement = document.activeElement;
        return overlay.matches(":hover")
            || (activeElement instanceof HTMLElement && overlay.contains(activeElement));
    }

    function show() {
        overlay.classList.remove("is-idle-hidden");
    }

    function scheduleAutoHide() {
        clearHideTimer();
        hideTimerId = window.setTimeout(() => {
            if (isStickyVisible()) {
                scheduleAutoHide();
                return;
            }
            overlay.classList.add("is-idle-hidden");
        }, idleMs);
    }

    function noteActivity() {
        show();
        scheduleAutoHide();
    }

    function bindActivityEvents(viewportPanel: HTMLElement): () => void {
        const cleanupListeners: Array<() => void> = [];

        const viewportHandlers = ["pointermove", "pointerenter", "wheel", "touchstart"];
        for (const type of viewportHandlers) {
            const handler = type === "wheel" || type === "touchstart"
                ? () => noteActivity()
                : () => noteActivity();
            viewportPanel.addEventListener(type, handler, type === "wheel" || type === "touchstart" ? { passive: true } : undefined);
            cleanupListeners.push(() => viewportPanel.removeEventListener(type, handler));
        }

        const overlayHandlers = ["pointerenter", "pointermove", "focusin"];
        for (const type of overlayHandlers) {
            const handler = () => noteActivity();
            overlay.addEventListener(type, handler);
            cleanupListeners.push(() => overlay.removeEventListener(type, handler));
        }

        const overlayLeaveHandlers = ["pointerleave", "focusout"];
        for (const type of overlayLeaveHandlers) {
            const handler = scheduleAutoHide;
            overlay.addEventListener(type, handler);
            cleanupListeners.push(() => overlay.removeEventListener(type, handler));
        }

        const keydownHandler = (event: KeyboardEvent) => {
            if (event.code === "Space" || event.key === "ArrowLeft" || event.key === "ArrowRight") {
                noteActivity();
            }
        };
        document.addEventListener("keydown", keydownHandler);
        cleanupListeners.push(() => document.removeEventListener("keydown", keydownHandler));

        return () => {
            for (const cleanup of cleanupListeners) {
                cleanup();
            }
        };
    }

    return {
        noteActivity,
        bindActivityEvents,
    };
}
