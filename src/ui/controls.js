export function setupUiControls({
    canvas,
    viewportPanel,
    sidebarToggle,
    debugToggleInput,
    eraScaleSelect,
    viewModeInputs,
    seedForm,
    seedInput,
    onResize,
    onSidebarToggle,
    onPointerMove,
    onPointerLeave,
    onDebugToggle,
    onEraScaleChange,
    onViewModeChange,
    onToggleSurface,
    onToggleDebug,
    getDebugEnabled,
    getCurrentSurfaceMode,
    onSubmitSeed,
    onSubmitSeedError,
}) {
    window.addEventListener("resize", onResize);
    if (typeof ResizeObserver !== "undefined") {
        const resizeObserver = new ResizeObserver(() => onResize());
        resizeObserver.observe(viewportPanel);
    }

    sidebarToggle.addEventListener("click", onSidebarToggle);

    canvas.addEventListener("pointermove", (event) => {
        onPointerMove(event);
    });
    canvas.addEventListener("pointerleave", onPointerLeave);
    canvas.addEventListener("pointercancel", onPointerLeave);

    debugToggleInput.addEventListener("change", () => {
        onDebugToggle(debugToggleInput.checked);
    });

    eraScaleSelect.addEventListener("change", () => {
        onEraScaleChange(eraScaleSelect.value, eraScaleSelect.disabled);
    });

    for (const input of viewModeInputs) {
        input.addEventListener("change", () => {
            if (!input.checked) {
                return;
            }
            onViewModeChange(input.value);
        });
    }

    document.addEventListener("keydown", (event) => {
        if (
            event.defaultPrevented ||
            event.metaKey ||
            event.ctrlKey ||
            event.altKey
        ) {
            return;
        }

        const target = event.target;
        if (
            target instanceof HTMLElement &&
            (target.isContentEditable ||
                target instanceof HTMLInputElement ||
                target instanceof HTMLTextAreaElement ||
                target instanceof HTMLSelectElement)
        ) {
            return;
        }

        if (event.key === "1") {
            event.preventDefault();
            onViewModeChange("normal");
            return;
        }

        if (event.key === "2") {
            event.preventDefault();
            onViewModeChange("plates");
            return;
        }

        if (event.key.toLowerCase() === "t") {
            event.preventDefault();
            seedInput.focus();
            seedInput.select();
            return;
        }

        if (event.key.toLowerCase() === "d") {
            event.preventDefault();
            onToggleDebug(!getDebugEnabled());
            return;
        }

        if (event.key.toLowerCase() === "v") {
            event.preventDefault();
            onToggleSurface(getCurrentSurfaceMode() === "globe" ? "map" : "globe");
        }
    });

    seedForm.addEventListener("submit", async (event) => {
        event.preventDefault();
        try {
            await onSubmitSeed(seedInput.value);
        } catch (error) {
            onSubmitSeedError(error);
        }
    });
}
