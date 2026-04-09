import {
    DEFAULT_CELL_METRIC,
    DEFAULT_ERA_SCALE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
} from "../../shared/constants";
import {
    createInitialBudgets,
    createInitialRuntimeState,
    type RuntimeState,
} from "../runtime/state";
import { type EraMetrics } from "./era-presets";
import { createInitialEngineViewState, type EngineViewState } from "./engine-view-state";
import { type CoreBuffers } from "../sim/sync/types";

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
        budgets: createInitialBudgets(),
        runtime: createInitialRuntimeState(currentEraMetrics.runtimeTickMs),
    };
    return {
        world,
        worldState: world.runtime,
    };
}

export interface AppState {
    currentSeed: string;
    currentSurfaceMode: string;
    currentViewMode: string;
    currentCellMetric: string;
    currentEraScale: string;
    currentEraMetrics: EraMetrics;
    debugEnabled: boolean;
}

export function createMutableStateStore(options: {
    currentEraMetrics: EraMetrics;
    debugEnabled?: boolean;
}) {
    let activeWorldId: string | null = null;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentTerrainData: CoreBuffers | null = null;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentCellMetric = DEFAULT_CELL_METRIC;
    let currentEraScale = DEFAULT_ERA_SCALE;
    let currentEraMetrics = options.currentEraMetrics;
    let debugEnabled = Boolean(options.debugEnabled);

    const stateSetters: Record<string, (value: unknown) => void> = {
        currentSeed: (value) => {
            currentSeed = value;
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
            currentSeed,
            currentSurfaceMode,
            currentViewMode,
            currentCellMetric,
            currentEraScale,
            currentEraMetrics,
            debugEnabled,
        };
    };

    const setState = (patch: Partial<AppState> = {}) => {
        for (const [key, value] of Object.entries(patch)) {
            stateSetters[key]?.(value);
        }
    };

    return {
        getState,
        setState,
        getCurrentTerrainData: () => currentTerrainData,
        setCurrentTerrainData: (nextTerrainData: CoreBuffers | null) => {
            currentTerrainData = nextTerrainData;
        },
        getActiveWorldId: () => activeWorldId,
        setActiveWorldId: (nextWorldId: string | null) => {
            activeWorldId = nextWorldId;
        },
    };
}
