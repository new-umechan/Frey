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
    metricCellOverlayPositions: Float32Array;
    metricCellOverlayCellIds: Uint32Array;
    nbrOffsets: Int32Array | null;
    nbrs: Int32Array | null;
}

export function createMeshBuffers(mesh: {
    positions: number[] | Float32Array;
    indices: number[] | Uint32Array;
    cell_overlay_positions: number[] | Float32Array;
    cell_overlay_cell_ids: number[] | Uint32Array;
    cell_overlay_lift: number[] | Float32Array;
}) {
    return {
        basePositions: new Float32Array(mesh.positions),
        indices: new Uint32Array(mesh.indices),
        metricCellOverlayMesh: {
            positions: new Float32Array(mesh.cell_overlay_positions),
            cellIds: new Uint32Array(mesh.cell_overlay_cell_ids),
            lift: new Float32Array(mesh.cell_overlay_lift),
        },
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
            metricCellOverlayPositions: new Float32Array(0),
            metricCellOverlayCellIds: new Uint32Array(0),
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
}

export function createMutableStateStore(options: {
    initialSeed?: string;
}) {
    let activeWorldId: string | null = null;
    let currentSeed = options.initialSeed ?? DEFAULT_TERRAIN_SEED;
    let currentTerrainData: CoreBuffers | null = null;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentCellMetric = DEFAULT_CELL_METRIC;
    let currentEraScale = DEFAULT_ERA_SCALE;

    const stateSetters: Record<string, (value: unknown) => void> = {
        currentSeed: (value) => {
            currentSeed = String(value);
        },
        currentSurfaceMode: (value) => {
            currentSurfaceMode = String(value);
        },
        currentViewMode: (value) => {
            currentViewMode = String(value);
        },
        currentCellMetric: (value) => {
            currentCellMetric = String(value);
        },
        currentEraScale: (value) => {
            currentEraScale = String(value);
        },
    };

    const getState = (): AppState => {
        return {
            currentSeed,
            currentSurfaceMode,
            currentViewMode,
            currentCellMetric,
            currentEraScale,
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
