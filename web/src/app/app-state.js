import {
    DEFAULT_CELL_METRIC,
    DEFAULT_ERA_SCALE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
} from "../core/constants.js";
import {
    createEmptyCore,
    createEmptyLayers,
    createInitialBudgets,
    createInitialRuntimeState,
} from "./runtime/state.js";

export function createMeshBuffers(mesh) {
    return {
        basePositions: new Float32Array(mesh.positions),
        indices: new Uint32Array(mesh.indices),
    };
}

export function createWorldState(options = {}) {
    const { basePositions, indices, currentEraMetrics } = options;
    const world = {
        tick: 0,
        era: DEFAULT_ERA_SCALE,
        mesh: {
            positions: basePositions,
            indices,
            nbrOffsets: null,
            nbrs: null,
        },
        core: createEmptyCore(),
        layers: createEmptyLayers(),
        budgets: createInitialBudgets(),
        runtime: createInitialRuntimeState(currentEraMetrics.runtimeTickMs),
    };
    return {
        world,
        worldState: world.runtime,
    };
}

export function createMutableStateStore(options = {}) {
    const { worldTick } = options;
    let activeWorldId = null;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentTerrainData = null;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentCellMetric = DEFAULT_CELL_METRIC;
    let currentEraScale = DEFAULT_ERA_SCALE;
    let currentEraMetrics = options.currentEraMetrics;
    let debugEnabled = Boolean(options.debugEnabled);

    const stateSetters = {
        activeWorldId: (value) => {
            activeWorldId = value;
        },
        currentSeed: (value) => {
            currentSeed = value;
        },
        currentTerrainData: (value) => {
            currentTerrainData = value;
        },
        currentSurfaceMode: (value) => {
            currentSurfaceMode = value;
        },
        currentViewMode: (value) => {
            currentViewMode = value;
        },
        currentCellMetric: (value) => {
            currentCellMetric = value;
        },
        currentEraScale: (value) => {
            currentEraScale = value;
        },
        currentEraMetrics: (value) => {
            currentEraMetrics = value;
        },
        debugEnabled: (value) => {
            debugEnabled = Boolean(value);
        },
    };

    const getState = () => {
        return {
            activeWorldId,
            currentSeed,
            currentTerrainData,
            currentSurfaceMode,
            currentViewMode,
            currentCellMetric,
            currentEraScale,
            currentEraMetrics,
            debugEnabled,
            worldTick: worldTick(),
        };
    };

    const setState = (patch = {}) => {
        for (const [key, value] of Object.entries(patch)) {
            stateSetters[key]?.(value);
        }
    };

    return {
        getState,
        setState,
    };
}
