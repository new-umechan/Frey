import { LAYER_KIND, type WorldSubsystemKey } from "../../shared/constants";
import { type CoreBuffers } from "../sim/sync/types";

export interface ExecModuleDocRecordState {
    phase: string;
    module: string;
    description: string;
    inbox: string;
    profile: string;
    display: string;
    execution: string;
    tick_boundary: boolean;
    reads: string[];
    writes: string[];
    feedback_targets: string[];
    depends_on: string[];
}

export interface ExecModuleGraphEdgeRecordState {
    from_phase: string;
    from_module: string;
    to_phase: string;
    to_module: string;
}

export interface ExecModuleGraphState {
    modules: ExecModuleDocRecordState[];
    edges: ExecModuleGraphEdgeRecordState[];
}

export function getDefaultExecDisplayPhase(runtimeState: Pick<RuntimeState, "execModules">): string {
    return runtimeState.execModules[0]?.display ?? "feedback";
}

export function describeExecModuleGraph(
    runtimeState: Pick<RuntimeState, "execModules" | "execModuleGraph">,
): string {
    const moduleCount = runtimeState.execModules.length;
    const edgeCount = runtimeState.execModuleGraph?.edges.length ?? 0;
    const firstModule = runtimeState.execModules[0];
    const lastModule = runtimeState.execModules[moduleCount - 1];
    const firstPhase = firstModule?.phase ?? "unknown";
    const lastPhase = lastModule?.phase ?? "unknown";
    return `${moduleCount} modules / ${edgeCount} edges / ${firstPhase} -> ${lastPhase}`;
}

export interface PlaybackEvent {
    id: number;
    type: string;
    tick: number;
    label: string;
    detail: string;
    createdAtMs: number;
}

export interface PlaybackState {
    isPlaying: boolean;
    historyInterval: number;
    maxKnownTick: number;
    selectedTick: number | null;
    availableTicks: number[];
    eventLog: PlaybackEvent[];
    nextLogId: number;
}

function createInitialPlaybackState(): PlaybackState {
    return {
        isPlaying: true,
        historyInterval: 32,
        maxKnownTick: 0,
        selectedTick: null,
        availableTicks: [],
        eventLog: [],
        nextLogId: 1,
    };
}

export function createEmptyCore(): CoreBuffers | null {
    return null;
}

export interface WorldLayers {
    [LAYER_KIND.CLIMATE]: unknown; // TODO: define climate layer type
    [LAYER_KIND.ECOLOGY]: unknown; // TODO: define ecology layer type
    [LAYER_KIND.CIVILIZATION]: unknown; // TODO: define civilization layer type
}

export function createEmptyLayers(): WorldLayers {
    return {
        [LAYER_KIND.CLIMATE]: null,
        [LAYER_KIND.ECOLOGY]: null,
        [LAYER_KIND.CIVILIZATION]: null,
    };
}

export function createInitialBudgets(): Record<WorldSubsystemKey, number> {
    return {
        geology: 0,
        climate: 0,
        ecology: 0,
        civilization: 0,
    };
}

export interface RuntimeState {
    isRunning: boolean;
    accumulatorMs: number;
    lastFrameTimeMs: number | null;
    runtimeTickMs: number;
    maxTicksPerFrame: number;
    sliceWorkBudget: number;
    sliceBusy: boolean;
    sliceRequestInFlight: boolean;
    slicePhase: string;
    maxRiverStepsPerFrame: number;
    erosionAutomatonState: unknown | null; // TODO: define automaton state type
    pendingRiverSteps: number;
    terrainErosionDirty: boolean;
    terrainCoreDirty: boolean;
    terrainDynamics: unknown | null; // TODO: define dynamics type
    execModules: ExecModuleDocRecordState[];
    execModuleGraph: ExecModuleGraphState | null;
    latestActivity: Record<WorldSubsystemKey, number>;
    carry: Record<WorldSubsystemKey, number>;
    executedSteps: Record<WorldSubsystemKey, number>;
    playback: PlaybackState;
}

export function createInitialRuntimeState(defaultRuntimeTickMs: number): RuntimeState {
    return {
        isRunning: true,
        accumulatorMs: 0,
        lastFrameTimeMs: null,
        runtimeTickMs: defaultRuntimeTickMs,
        maxTicksPerFrame: 20,
        sliceWorkBudget: 1,
        sliceBusy: false,
        sliceRequestInFlight: false,
        slicePhase: "feedback",
        maxRiverStepsPerFrame: 4,
        erosionAutomatonState: null,
        pendingRiverSteps: 0,
        terrainErosionDirty: false,
        terrainCoreDirty: false,
        terrainDynamics: null,
        execModules: [],
        execModuleGraph: null,
        latestActivity: {
            geology: 0,
            climate: 1,
            ecology: 1,
            civilization: 1,
        },
        carry: {
            geology: 0,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        executedSteps: {
            geology: 0,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        playback: createInitialPlaybackState(),
    };
}
