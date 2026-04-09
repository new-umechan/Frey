import {
    DEFAULT_CELL_METRIC,
    DEFAULT_ERA_SCALE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
} from "../../shared/constants";
import {
    createEmptyCore,
    createEmptyLayers,
    createInitialBudgets,
    createInitialRuntimeState,
    type RuntimeState,
} from "../runtime/state";
import { type EraMetrics } from "./era-presets";
import { createInitialEngineViewState, type EngineViewState } from "./engine-view-state";

export interface Mesh {
    positions: Float32Array;
    indices: Uint32Array;
    nbrOffsets: Int32Array | null;
    nbrs: Int32Array | null;
}

export function createMeshBuffers(mesh: { positions: number[] | Float32Array; indices: number[] | Uint32Array }) {
    return {
        basePositions: new Float32Array(mesh.positions),
        indices: new Uint32Array(mesh.indices),
    };
}

export interface WorldState {
    tick: number;
    era: string;
    mesh: Mesh;
    engineView: EngineViewState;
    core: any;
    layers: Record<string, any>;
    budgets: Record<string, number>;
    runtime: RuntimeState;
}

export function createWorldState(options: {
    basePositions: Float32Array;
    indices: Uint32Array;
    currentEraMetrics: EraMetrics;
}): { world: WorldState; worldState: RuntimeState } {
    const { basePositions, indices, currentEraMetrics } = options;
    const world: WorldState = {
        tick: 0,
        era: DEFAULT_ERA_SCALE,
        mesh: {
            positions: basePositions,
            indices,
            nbrOffsets: null,
            nbrs: null,
        },
        engineView: createInitialEngineViewState(),
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

export interface AppState {
    activeWorldId: string | null;
    currentSeed: string;
    currentTerrainData: any;
    currentSurfaceMode: string;
    currentViewMode: string;
    currentCellMetric: string;
    currentEraScale: string;
    currentEraMetrics: EraMetrics;
    debugEnabled: boolean;
    worldTick: number;
}

export function createMutableStateStore(options: {
    worldTick: () => number;
    currentEraMetrics: EraMetrics;
    debugEnabled?: boolean;
}) {
    const { worldTick } = options;
    let activeWorldId: string | null = null;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentTerrainData: any = null;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentCellMetric = DEFAULT_CELL_METRIC;
    let currentEraScale = DEFAULT_ERA_SCALE;
    let currentEraMetrics = options.currentEraMetrics;
    let debugEnabled = Boolean(options.debugEnabled);

    const stateSetters: Record<string, (value: any) => void> = {
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

    const getState = (): AppState => {
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

    const setState = (patch: Partial<Omit<AppState, "worldTick">> = {}) => {
        for (const [key, value] of Object.entries(patch)) {
            stateSetters[key]?.(value);
        }
    };

    return {
        getState,
        setState,
    };
}
