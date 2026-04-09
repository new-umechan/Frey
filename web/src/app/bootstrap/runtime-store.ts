import { DEFAULT_ERA_SCALE } from "../../shared/constants";
import { createMutableStateStore, createWorldState, type AppState, type WorldState } from "../state/app-state";
import { type EraMetrics } from "../state/era-presets";
import { type RuntimeState } from "../runtime/state";

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
    setState: (patch: Partial<Omit<AppState, "worldTick">>) => void;
    getCurrentEraMetrics: () => EraMetrics;
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
        currentEraMetrics,
        debugEnabled,
        worldTick: () => world.tick,
    });

    mutableStateStore.setState({ currentTerrainData: world.core });

    function setState(patch: Partial<Omit<AppState, "worldTick">> = {}) {
        mutableStateStore.setState(patch);
        if (patch.currentEraMetrics) {
            currentEraMetrics = patch.currentEraMetrics;
        }
    }

    return {
        world,
        worldState,
        getState: mutableStateStore.getState,
        setState,
        getCurrentEraMetrics: () => currentEraMetrics,
    };
}
