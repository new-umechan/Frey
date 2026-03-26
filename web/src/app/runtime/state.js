import { LAYER_KIND } from "../../core/constants.js";

function createInitialPlaybackState() {
    return {
        isPlaying: true,
        historyInterval: 32,
        selectedTick: null,
        availableTicks: [],
        eventLog: [],
        nextLogId: 1,
    };
}

export function createEmptyCore() {
    return null;
}

export function createEmptyLayers() {
    return {
        [LAYER_KIND.CLIMATE]: null,
        [LAYER_KIND.ECOLOGY]: null,
        [LAYER_KIND.CIVILIZATION]: null,
    };
}

export function createInitialBudgets() {
    return {
        geology: 0,
        climate: 0,
        ecology: 0,
        civilization: 0,
    };
}

export function createInitialRuntimeState(defaultRuntimeTickMs) {
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
