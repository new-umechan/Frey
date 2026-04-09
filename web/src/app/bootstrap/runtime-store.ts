import { DEFAULT_ERA_SCALE } from "../../shared/constants";
import { createMutableStateStore, createWorldState, type AppState, type WorldState } from "../state/app-state";
import { type EraMetrics } from "../state/era-presets";
import { type RuntimeState } from "../runtime/state";
import { type CoreBuffers } from "../sim/sync/types";

export interface RuntimeStoreOptions {
    basePositions: Float32Array;
    indices: Uint32Array;
    createEraMetrics: (era: string) => EraMetrics;
    debugEnabled?: boolean;
}

export interface RuntimeStore {
    world: WorldState;
    worldState: RuntimeState;
    getState: () => AppState;
    setState: (patch: Partial<AppState>) => void;
    getCurrentEraMetrics: () => EraMetrics;
    setCurrentEraMetrics: (nextMetrics: EraMetrics) => void;
    getCurrentTerrainData: () => CoreBuffers | null;
    setCurrentTerrainData: (nextData: CoreBuffers | null) => void;
    getActiveWorldId: () => string | null;
    setActiveWorldId: (nextWorldId: string | null) => void;
}

export function createRuntimeStore(options: RuntimeStoreOptions): RuntimeStore {
    const {
        basePositions,
        indices,
        createEraMetrics,
        debugEnabled,
    } = options;

    let currentEraMetrics = createEraMetrics(DEFAULT_ERA_SCALE);
    const { world, worldState } = createWorldState({
        basePositions,
        indices,
        currentEraMetrics,
    });
    const mutableStateStore = createMutableStateStore({
        debugEnabled,
    });

    mutableStateStore.setCurrentTerrainData(null);

    function setState(patch: Partial<AppState> = {}) {
        mutableStateStore.setState(patch);
    }

    return {
        world,
        worldState,
        getState: mutableStateStore.getState,
        setState,
        getCurrentEraMetrics: () => currentEraMetrics,
        setCurrentEraMetrics: (nextMetrics: EraMetrics) => {
            currentEraMetrics = nextMetrics;
        },
        getCurrentTerrainData: mutableStateStore.getCurrentTerrainData,
        setCurrentTerrainData: (nextData: CoreBuffers | null) => {
            mutableStateStore.setCurrentTerrainData(nextData);
        },
        getActiveWorldId: mutableStateStore.getActiveWorldId,
        setActiveWorldId: (nextWorldId: string | null) => {
            mutableStateStore.setActiveWorldId(nextWorldId);
        },
    };
}
