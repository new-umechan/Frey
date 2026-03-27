import { setupUiControls, type PlaybackControlsElements, type PerfControlsElements } from "../../ui/controls";
import { renderEraScaleControls, type EraMetrics, type EraScaleWeightFields } from "../core/era-presets";
import { createCanvasInputHandlers } from "../input/canvas-input-handlers";
import { formatStatusError } from "../core/status-error";
import type { PlateHoverController } from "../input/plate-hover";
import type { GlobePinchFocusController } from "../../gfx/views/globe-pinch-focus-controller";
import type { PlaybackController } from "../playback/playback-controller";

function createSidebarToggleHandler(
    sidebarToggle: HTMLButtonElement | null,
    setSidebarOpen: ((isOpen: boolean) => void) | undefined,
    onResize: () => void
) {
    return () => {
        if (!sidebarToggle || !setSidebarOpen) {
            return;
        }
        const isOpen = sidebarToggle.getAttribute("aria-expanded") === "true";
        setSidebarOpen(!isOpen);
        requestAnimationFrame(onResize);
    };
}

interface EraScaleHandlerDeps {
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    setEraScale: (value: string) => void;
    getCurrentEraScale: () => string;
    getCurrentEraMetrics: () => EraMetrics;
}

function createEraScaleChangeHandler(deps: EraScaleHandlerDeps) {
    return (value: string, isDisabled: boolean) => {
        if (isDisabled) {
            renderEraScaleControls(
                deps.eraScaleSelect,
                deps.eraScaleTickLabel,
                deps.eraScaleWeightFields,
                deps.getCurrentEraScale(),
                deps.getCurrentEraMetrics()
            );
            return;
        }
        deps.setEraScale(value);
    };
}

function createSubmitSeedErrorHandler(
    setStatus: (msg: string) => void,
    seedInput: HTMLInputElement,
    seedForm: HTMLFormElement
) {
    return (error: unknown) => {
        setStatus(formatStatusError("Generation", error));
        seedInput.removeAttribute("disabled");
        seedForm.querySelector("button")?.removeAttribute("disabled");
        console.error(error);
    };
}

interface CreateUiHandlersOptions {
    sidebarToggle: HTMLButtonElement | null;
    setSidebarOpen: ((isOpen: boolean) => void) | undefined;
    onResize: () => void;
    setEraScale: (value: string) => void;
    setStatus: (msg: string) => void;
    seedInput: HTMLInputElement;
    seedForm: HTMLFormElement;
    plateHover: PlateHoverController;
    globePinchFocusController: GlobePinchFocusController;
    setDebugModeEnabled: (enabled: boolean) => void;
    setViewMode: (mode: string) => void;
    setCellMetric: (metric: string) => void;
    setSurfaceMode: (mode: string) => void;
    playbackController: PlaybackController;
    runPerf: () => void;
    copyPerfResult: () => void;
    updateTerrain: (seed: string) => Promise<void>;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
    getCurrentEraScale: () => string;
    getCurrentEraMetrics: () => EraMetrics;
}

function createUiHandlers(options: CreateUiHandlersOptions) {
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
        runPerf,
        copyPerfResult,
        updateTerrain,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        getCurrentEraScale,
        getCurrentEraMetrics,
    } = options;

    return {
        onSidebarToggle: createSidebarToggleHandler(sidebarToggle, setSidebarOpen, onResize),
        onEraScaleChange: createEraScaleChangeHandler({
            eraScaleSelect,
            eraScaleTickLabel,
            eraScaleWeightFields,
            setEraScale,
            getCurrentEraScale,
            getCurrentEraMetrics,
        }),
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
        onRunPerfBenchmark: runPerf,
        onCopyPerfBenchmark: copyPerfResult,
        onSubmitSeed: updateTerrain,
    };
}

export interface BindAppUiControlsOptions {
    canvas: HTMLCanvasElement;
    viewportPanel: HTMLDivElement;
    sidebarToggle: HTMLButtonElement | null;
    debugToggleInput: HTMLInputElement;
    eraScaleSelect: HTMLSelectElement;
    eraScaleTickLabel: HTMLElement;
    eraScaleWeightFields: EraScaleWeightFields;
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
    setSidebarOpen: ((isOpen: boolean) => void) | undefined;
    plateHover: PlateHoverController;
    globePinchFocusController: GlobePinchFocusController;
    setDebugModeEnabled: (enabled: boolean) => void;
    setEraScale: (value: string) => void;
    setViewMode: (mode: string) => void;
    setCellMetric: (metric: string) => void;
    setSurfaceMode: (mode: string) => void;
    playbackController: PlaybackController;
    runPerf: () => void;
    copyPerfResult: () => void;
    getDebugEnabled: () => boolean;
    getCurrentSurfaceMode: () => string;
    getCurrentCellMetric: () => string;
    getCurrentEraScale: () => string;
    getCurrentEraMetrics: () => EraMetrics;
    updateTerrain: (seed: string) => Promise<void>;
    setStatus: (msg: string) => void;
}

export function bindAppUiControls(options: BindAppUiControlsOptions) {
    const {
        canvas,
        viewportPanel,
        sidebarToggle,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
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
        runPerf,
        copyPerfResult,
        getDebugEnabled,
        getCurrentSurfaceMode,
        getCurrentCellMetric,
        getCurrentEraScale,
        getCurrentEraMetrics,
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
        runPerf,
        copyPerfResult,
        updateTerrain,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        getCurrentEraScale,
        getCurrentEraMetrics,
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
        getCurrentCellMetric,
        onSubmitSeed: handlers.onSubmitSeed,
        onSubmitSeedError: handlers.onSubmitSeedError,
    });
}
