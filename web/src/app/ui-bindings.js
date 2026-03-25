import { setupUiControls } from "../ui/controls.js";
import { renderEraScaleControls } from "./era-presets.js";
import { createCanvasInputHandlers } from "./canvas-input-handlers.js";

function createSidebarToggleHandler(sidebarToggle, setSidebarOpen, onResize) {
    return () => {
        if (!sidebarToggle) {
            return;
        }
        const isOpen = sidebarToggle.getAttribute("aria-expanded") === "true";
        setSidebarOpen(!isOpen);
        requestAnimationFrame(onResize);
    };
}

function createEraScaleChangeHandler(setEraScale) {
    return (value, isDisabled) => {
        if (isDisabled) {
            renderEraScaleControls();
            return;
        }
        setEraScale(value);
    };
}

function createSubmitSeedErrorHandler(setStatus, seedInput, seedForm) {
    return (error) => {
        setStatus(`Generation failed: ${String(error)}`);
        seedInput.removeAttribute("disabled");
        seedForm.querySelector("button")?.removeAttribute("disabled");
        console.error(error);
    };
}

export function bindAppUiControls(options = {}) {
    const {
        canvas,
        viewportPanel,
        sidebarToggle,
        debugToggleInput,
        eraScaleSelect,
        viewModeInputs,
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
        globePinchFocusController,
        setDebugModeEnabled,
        setEraScale,
        setViewMode,
        setCellMetric,
        setSurfaceMode,
        playbackController,
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        getDebugEnabled,
        getCurrentSurfaceMode,
        getCurrentViewMode,
        getCurrentCellMetric,
        updateTerrain,
        setStatus,
    } = options;

    const handleSidebarToggle = createSidebarToggleHandler(sidebarToggle, setSidebarOpen, onResize);
    const handleEraScaleChange = createEraScaleChangeHandler(setEraScale);
    const handleSubmitSeedError = createSubmitSeedErrorHandler(setStatus, seedInput, seedForm);
    const canvasInputHandlers = createCanvasInputHandlers({
        plateHover,
        globePinchFocusController,
    });

    setupUiControls({
        canvas,
        viewportPanel,
        sidebarToggle,
        debugToggleInput,
        eraScaleSelect,
        viewModeInputs,
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfEnabled,
        perfControls,
        seedForm,
        seedInput,
        onResize,
        onSidebarToggle: handleSidebarToggle,
        canvasInputHandlers,
        onDebugToggle: setDebugModeEnabled,
        onEraScaleChange: handleEraScaleChange,
        onViewModeChange: setViewMode,
        onCellMetricChange: setCellMetric,
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
        getCurrentCellMetric,
        onSubmitSeed: updateTerrain,
        onSubmitSeedError: handleSubmitSeedError,
    });
}
