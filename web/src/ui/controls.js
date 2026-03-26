import { createControlHelpController } from "./controls/control-help-controller.js";
import { createGlobalKeyboardHandler } from "./controls/keyboard-shortcuts.js";
import {
    bindPerfEvents,
    bindPlaybackUiEvents,
} from "./controls/ui-event-bindings.js";
import { createViewCuiController } from "./controls/view-cui-controller.js";

export function setupUiControls({
    canvas,
    viewportPanel,
    sidebarToggle,
    eraScaleSelect,
    debugToggleInput,
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
    onSidebarToggle,
    canvasInputHandlers = {},
    onDebugToggle,
    onEraScaleChange,
    onViewModeChange,
    onCellMetricChange,
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
    getCurrentCellMetric,
    onSubmitSeed,
    onSubmitSeedError,
}) {
    window.addEventListener("resize", onResize);
    if (typeof ResizeObserver !== "undefined") {
        const resizeObserver = new ResizeObserver(() => onResize());
        resizeObserver.observe(viewportPanel);
    }

    if (sidebarToggle) {
        sidebarToggle.addEventListener("click", onSidebarToggle);
    }

    const {
        onPointerDown = () => {},
        onPointerMove = () => {},
        onPointerUp = () => {},
        onPointerCancel = () => {},
        onWheel = () => false,
        onLeave = () => {},
    } = canvasInputHandlers;

    canvas.addEventListener("pointerdown", (event) => {
        onPointerDown(event);
    });
    canvas.addEventListener("pointermove", (event) => {
        onPointerMove(event);
    });
    canvas.addEventListener("pointerup", (event) => {
        onPointerUp(event);
    });
    canvas.addEventListener("pointerleave", onLeave);
    canvas.addEventListener("pointercancel", (event) => {
        onPointerCancel(event);
        onLeave(event);
    });
    canvas.addEventListener("wheel", (event) => {
        const shouldPreventDefault = onWheel(event);
        if (shouldPreventDefault) {
            event.preventDefault();
            event.stopPropagation();
        }
    }, { capture: true, passive: false });

    debugToggleInput.addEventListener("change", () => {
        onDebugToggle(debugToggleInput.checked);
    });

    eraScaleSelect.addEventListener("change", () => {
        onEraScaleChange(eraScaleSelect.value, eraScaleSelect.disabled);
    });

    const controlHelp = createControlHelpController(controlHelpModal, controlHelpCloseButton);
    const viewCui = createViewCuiController({
        viewModeInputs,
        getCurrentCellMetric,
        onViewModeChange,
        onCellMetricChange,
    });

    bindPlaybackUiEvents({
        playbackControls,
        eventLogList,
        onTogglePlay,
        onHistorySeek,
        onHistoryStepDirection,
        onEventLogJump,
    });
    bindPerfEvents(perfEnabled, perfControls, onRunPerfBenchmark, onCopyPerfBenchmark);

    document.addEventListener("keydown", createGlobalKeyboardHandler({
        controlHelp,
        viewCui,
        seedInput,
        getDebugEnabled,
        getCurrentSurfaceMode,
        onToggleDebug,
        onToggleSurface,
        onTogglePlay,
        onStepForward,
        onRewind,
        onHistoryStepDirection,
    }));

    seedForm.addEventListener("submit", async (event) => {
        event.preventDefault();
        try {
            await onSubmitSeed(seedInput.value);
        } catch (error) {
            onSubmitSeedError(error);
        }
    });
}
