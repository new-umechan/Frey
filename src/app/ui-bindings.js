import { setupUiControls } from "../ui/controls.js";
import { renderEraScaleControls } from "./era-presets.js";

export function bindAppUiControls(options = {}) {
    const {
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
        setSidebarOpen,
        plateHover,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setClimateMetric,
        setSurfaceMode,
        playbackController,
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        getDebugEnabled,
        getCurrentSurfaceMode,
        getCurrentViewMode,
        updateTerrain,
        setStatus,
    } = options;

    setupUiControls({
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
        onSidebarToggle: () => {
            const isOpen = sidebarToggle.getAttribute("aria-expanded") === "true";
            setSidebarOpen(!isOpen);
            requestAnimationFrame(onResize);
        },
        onPointerMove: plateHover.updateFromPointer,
        onPointerLeave: plateHover.hidePopup,
        onDebugToggle: setDebugModeEnabled,
        onEraScaleChange: (value, isDisabled) => {
            if (isDisabled) {
                renderEraScaleControls();
                return;
            }
            setEraScale(value);
        },
        onViewModeChange: setViewMode,
        onClimateMetricChange: setClimateMetric,
        onToggleSurface: setSurfaceMode,
        onToggleDebug: setDebugModeEnabled,
        onTogglePlay: playbackController.handleTogglePlay,
        onStepForward: playbackController.handleStepForward,
        onRewind: playbackController.handleRewind,
        onHistorySeek: playbackController.handleHistorySeek,
        onHistoryStepDirection: playbackController.handleHistoryStepDirection,
        onEventLogJump: playbackController.handleHistoryJump,
        onRunPerfBenchmark: runPerfBenchmark,
        onCopyPerfBenchmark: copyPerfBenchmarkResult,
        getDebugEnabled,
        getCurrentSurfaceMode,
        getCurrentViewMode,
        onSubmitSeed: updateTerrain,
        onSubmitSeedError: (error) => {
            setStatus(`Generation failed: ${String(error)}`);
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
            console.error(error);
        },
    });
}
