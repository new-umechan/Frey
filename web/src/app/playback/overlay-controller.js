export function createPlaybackOverlayController({ overlay, idleMs }) {
    let hideTimerId = null;

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

    function bindActivityEvents(viewportPanel) {
        viewportPanel.addEventListener("pointermove", noteActivity);
        viewportPanel.addEventListener("pointerenter", noteActivity);
        viewportPanel.addEventListener("wheel", noteActivity, { passive: true });
        viewportPanel.addEventListener("touchstart", noteActivity, { passive: true });

        overlay.addEventListener("pointerenter", noteActivity);
        overlay.addEventListener("pointermove", noteActivity);
        overlay.addEventListener("focusin", noteActivity);
        overlay.addEventListener("pointerleave", scheduleAutoHide);
        overlay.addEventListener("focusout", scheduleAutoHide);

        document.addEventListener("keydown", (event) => {
            if (event.code === "Space" || event.key === "ArrowLeft" || event.key === "ArrowRight") {
                noteActivity();
            }
        });
    }

    return {
        noteActivity,
        bindActivityEvents,
    };
}
