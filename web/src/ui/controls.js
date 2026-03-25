import { createControlHelpController } from "./controls/control-help-controller.js";
import {
    isHelpToggleKey,
    isInteractiveTarget,
} from "./controls/keyboard-guards.js";
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

    document.addEventListener("keydown", (event) => {
        if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) {
            return;
        }

        if (isInteractiveTarget(event.target)) {
            return;
        }

        const lowerKey = event.key.toLowerCase();

        if (isHelpToggleKey(event)) {
            event.preventDefault();
            controlHelp.toggleControlHelp();
            return;
        }

        if (controlHelp.isOpen()) {
            if (event.key === "Escape") {
                event.preventDefault();
                controlHelp.closeControlHelp();
            }
            return;
        }

        if (event.key === "ArrowUp" || lowerKey === "k") {
            event.preventDefault();
            viewCui.moveViewCursor(-1);
            return;
        }

        if (event.key === "ArrowDown" || lowerKey === "j") {
            event.preventDefault();
            viewCui.moveViewCursor(1);
            return;
        }

        if (event.key === "Enter" || lowerKey === "l") {
            event.preventDefault();
            viewCui.commitViewSelection();
            return;
        }

        if ((event.key === "Escape" || lowerKey === "h") && viewCui.backViewMenu()) {
            event.preventDefault();
            return;
        }

        if (viewCui.handleDigitSelect(event.key)) {
            event.preventDefault();
            return;
        }

        if (lowerKey === "t" || lowerKey === "s") {
            event.preventDefault();
            seedInput.focus();
            seedInput.select();
            return;
        }

        if (lowerKey === "d") {
            event.preventDefault();
            onToggleDebug(!getDebugEnabled());
            return;
        }

        if (lowerKey === "v") {
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
