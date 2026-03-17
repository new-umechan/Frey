export function setupUiControls({
    canvas,
    viewportPanel,
    sidebarToggle,
    debugToggleInput,
    eraScaleSelect,
    viewModeInputs,
    climateMetricInputs,
    controlHelpModal,
    controlHelpCloseButton,
    playbackControls,
    eventLogList,
    perfEnabled,
    perfControls,
    seedForm,
    seedInput,
    onResize,
    onSidebarToggle,
    onPointerMove,
    onPointerLeave,
    onDebugToggle,
    onEraScaleChange,
    onViewModeChange,
    onClimateMetricChange,
    onToggleSurface,
    onToggleDebug,
    onTogglePlay,
    onStepForward,
    onRewind,
    onHistorySeek,
    onHistoryStepDirection,
    onEventLogJump,
    onRunPerfBenchmark,
    onCopyPerfBenchmark,
    getDebugEnabled,
    getCurrentSurfaceMode,
    getCurrentViewMode,
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

    for (const input of climateMetricInputs) {
        input.addEventListener("change", () => {
            if (!input.checked) {
                return;
            }
            onClimateMetricChange(input.value);
        });
    }

    playbackControls.playToggleButton.addEventListener("click", () => {
        onTogglePlay();
    });
    playbackControls.historySeekSlider.addEventListener("input", () => {
        onHistorySeek(playbackControls.historySeekSlider.value);
    });
    playbackControls.historySeekSlider.addEventListener("change", () => {
        onHistorySeek(playbackControls.historySeekSlider.value);
    });
    playbackControls.seekBackwardButton.addEventListener("click", () => {
        onHistoryStepDirection(-1);
    });
    playbackControls.seekForwardButton.addEventListener("click", () => {
        onHistoryStepDirection(1);
    });
    eventLogList.addEventListener("click", (event) => {
        const target = event.target;
        if (!(target instanceof HTMLElement)) {
            return;
        }
        const entryButton = target.closest("[data-log-tick]");
        if (!(entryButton instanceof HTMLButtonElement)) {
            return;
        }
        const tickText = entryButton.dataset.logTick;
        if (!tickText) {
            return;
        }
        onEventLogJump(tickText);
    });

    if (perfEnabled && perfControls) {
        perfControls.runButton.addEventListener("click", () => {
            onRunPerfBenchmark();
        });
        perfControls.copyButton.addEventListener("click", () => {
            onCopyPerfBenchmark();
        });
    }

    function isHelpToggleKey(event) {
        return event.key === "?" || (event.code === "Slash" && event.shiftKey);
    }

    function openControlHelp() {
        controlHelpModal.hidden = false;
    }

    function closeControlHelp() {
        controlHelpModal.hidden = true;
    }

    function toggleControlHelp() {
        if (controlHelpModal.hidden) {
            openControlHelp();
            return;
        }
        closeControlHelp();
    }

    controlHelpCloseButton.addEventListener("click", closeControlHelp);
    controlHelpModal.addEventListener("click", (event) => {
        const target = event.target;
        if (!(target instanceof HTMLElement)) {
            return;
        }
        if (target.dataset.controlHelpClose !== undefined) {
            closeControlHelp();
        }
    });

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
                (target instanceof HTMLInputElement && target.type !== "range") ||
                target instanceof HTMLTextAreaElement ||
                target instanceof HTMLSelectElement)
        ) {
            return;
        }

        if (isHelpToggleKey(event)) {
            event.preventDefault();
            toggleControlHelp();
            return;
        }

        if (!controlHelpModal.hidden) {
            if (event.key === "Escape") {
                event.preventDefault();
                closeControlHelp();
            }
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

        if (event.key === "3") {
            event.preventDefault();
            onViewModeChange("mantle");
            return;
        }

        if (event.key === "4") {
            event.preventDefault();
            onViewModeChange("climate");
            return;
        }

        if (getCurrentViewMode() === "climate" && event.key.toLowerCase() === "q") {
            event.preventDefault();
            onClimateMetricChange("temperature");
            return;
        }

        if (getCurrentViewMode() === "climate" && event.key.toLowerCase() === "w") {
            event.preventDefault();
            onClimateMetricChange("precipitation");
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
            return;
        }

        if (event.code === "Space") {
            event.preventDefault();
            onTogglePlay();
            return;
        }

        if (event.key === ".") {
            event.preventDefault();
            onStepForward();
            return;
        }

        if (event.key === ",") {
            event.preventDefault();
            onRewind();
            return;
        }

        if (event.key === "ArrowLeft") {
            event.preventDefault();
            onHistoryStepDirection(-1);
            return;
        }

        if (event.key === "ArrowRight") {
            event.preventDefault();
            onHistoryStepDirection(1);
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
