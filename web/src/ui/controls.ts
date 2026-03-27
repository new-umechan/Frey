import { createControlHelpController } from "./controls/control-help-controller";
import { createGlobalKeyboardHandler } from "./controls/keyboard-shortcuts";
import {
    bindPerfEvents,
    bindPlaybackUiEvents,
} from "./controls/ui-event-bindings";
import { createViewCuiController } from "./controls/view-cui-controller";
import {
    type PlaybackControlsElements,
    type PerfControlsElements,
} from "./dom";

export interface CanvasInputHandlers {
    onPointerDown?: (event: PointerEvent) => void;
    onPointerMove?: (event: PointerEvent) => void;
    onPointerUp?: (event: PointerEvent) => void;
    onPointerCancel?: (event: PointerEvent) => void;
    onWheel?: (event: WheelEvent) => boolean;
    onLeave?: (event: PointerEvent) => void;
}

export interface SetupUiControlsOptions {
    canvas: HTMLCanvasElement;
    viewportPanel: HTMLDivElement;
    sidebarToggle: HTMLButtonElement | null;
    eraScaleSelect: HTMLSelectElement;
    debugToggleInput: HTMLInputElement;
    viewModeInputs: HTMLInputElement[];
    controlHelpModal: HTMLDivElement | null;
    controlHelpCloseButton: HTMLButtonElement | null;
    playbackControls: PlaybackControlsElements;
    eventLogList: HTMLUListElement;
    perfEnabled: boolean;
    perfControls: PerfControlsElements | null;
    seedForm: HTMLFormElement;
    seedInput: HTMLInputElement;
    onResize: () => void;
    onSidebarToggle: () => void;
    canvasInputHandlers?: CanvasInputHandlers;
    onDebugToggle: (enabled: boolean) => void;
    onEraScaleChange: (value: string, isDisabled: boolean) => void;
    onViewModeChange: (mode: string) => void;
    onCellMetricChange: (metric: string) => void;
    onToggleSurface: (mode: string) => void;
    onToggleDebug: (enabled: boolean) => void;
    onTogglePlay: () => void;
    onStepForward: () => void;
    onRewind: () => void;
    onHistorySeek: (tick: number) => void;
    onHistoryStepDirection: (dir: number) => void;
    onEventLogJump: (tick: number) => void;
    onRunPerfBenchmark: () => void;
    onCopyPerfBenchmark: () => void;
    getDebugEnabled: () => boolean;
    getCurrentSurfaceMode: () => string;
    getCurrentCellMetric: () => string;
    onSubmitSeed: (seed: string) => Promise<void>;
    onSubmitSeedError: (error: any) => void;
}

export function setupUiControls(options: SetupUiControlsOptions) {
    const {
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
    } = options;

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
    canvas.addEventListener("pointerleave", (event) => onLeave(event));
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
