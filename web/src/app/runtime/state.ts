import { LAYER_KIND, type WorldSubsystemKey } from "../../core/constants";
import { type CoreBuffers } from "../world-sync/types";

export interface PlaybackState {
    isPlaying: boolean;
    historyInterval: number;
    selectedTick: number | null;
    availableTicks: number[];
    eventLog: any[];
    nextLogId: number;
}

function createInitialPlaybackState(): PlaybackState {
    return {
        isPlaying: true,
        historyInterval: 32,
        selectedTick: null,
        availableTicks: [],
        eventLog: [],
        nextLogId: 1,
    };
}

export function createEmptyCore(): CoreBuffers | null {
    return null;
}

export function createEmptyLayers(): Record<string, any> {
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
    slicePhase: string;
    maxRiverStepsPerFrame: number;
    erosionAutomatonState: any;
    pendingRiverSteps: number;
    terrainErosionDirty: boolean;
    terrainCoreDirty: boolean;
    terrainDynamics: any;
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
        slicePhase: "feedback",
        maxRiverStepsPerFrame: 4,
        erosionAutomatonState: null,
        pendingRiverSteps: 0,
        terrainErosionDirty: false,
        terrainCoreDirty: false,
        terrainDynamics: null,
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
