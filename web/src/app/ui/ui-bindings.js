import { setupUiControls } from "../../ui/controls.js";
import { renderEraScaleControls } from "../core/era-presets.js";
import { createCanvasInputHandlers } from "../input/canvas-input-handlers.js";
import { formatStatusError } from "../core/status-error.js";

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
        setStatus(formatStatusError("Generation", error));
        seedInput.removeAttribute("disabled");
        seedForm.querySelector("button")?.removeAttribute("disabled");
        console.error(error);
    };
}

function createUiHandlers(options = {}) {
    const {
        sidebarToggle,
        setSidebarOpen,
        onResize,
        setEraScale,
        setStatus,
        seedInput,
        seedForm,
        plateHover,
        globePinchFocusController,
        setDebugModeEnabled,
        setViewMode,
        setCellMetric,
        setSurfaceMode,
        playbackController,
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        updateTerrain,
    } = options;

    return {
        onSidebarToggle: createSidebarToggleHandler(sidebarToggle, setSidebarOpen, onResize),
        onEraScaleChange: createEraScaleChangeHandler(setEraScale),
        onSubmitSeedError: createSubmitSeedErrorHandler(setStatus, seedInput, seedForm),
        canvasInputHandlers: createCanvasInputHandlers({
            plateHover,
            globePinchFocusController,
        }),
        onDebugToggle: setDebugModeEnabled,
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
        onSubmitSeed: updateTerrain,
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

    const handlers = createUiHandlers({
        sidebarToggle,
        setSidebarOpen,
        onResize,
        setEraScale,
        setStatus,
        seedInput,
        seedForm,
        plateHover,
        globePinchFocusController,
        setDebugModeEnabled,
        setViewMode,
        setCellMetric,
        setSurfaceMode,
        playbackController,
        runPerfBenchmark,
        copyPerfBenchmarkResult,
        updateTerrain,
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
        onSidebarToggle: handlers.onSidebarToggle,
        canvasInputHandlers: handlers.canvasInputHandlers,
        onDebugToggle: handlers.onDebugToggle,
        onEraScaleChange: handlers.onEraScaleChange,
        onViewModeChange: handlers.onViewModeChange,
        onCellMetricChange: handlers.onCellMetricChange,
        onToggleSurface: handlers.onToggleSurface,
        onToggleDebug: handlers.onToggleDebug,
        onTogglePlay: handlers.onTogglePlay,
        onStepForward: handlers.onStepForward,
        onRewind: handlers.onRewind,
        onHistorySeek: handlers.onHistorySeek,
        onHistoryStepDirection: handlers.onHistoryStepDirection,
        onEventLogJump: handlers.onEventLogJump,
        onRunPerfBenchmark: handlers.onRunPerfBenchmark,
        onCopyPerfBenchmark: handlers.onCopyPerfBenchmark,
        getDebugEnabled,
        getCurrentSurfaceMode,
        getCurrentViewMode,
        getCurrentCellMetric,
        onSubmitSeed: handlers.onSubmitSeed,
        onSubmitSeedError: handlers.onSubmitSeedError,
    });
}
