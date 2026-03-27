import { DEFAULT_ERA_SCALE } from "../../core/constants.js";
import { createMutableStateStore, createWorldState, type WorldState } from "../core/app-state.js";
import { type EraMetrics } from "../core/era-presets.js";
import { type RuntimeState } from "../runtime/state.js";

export interface RuntimeStoreOptions {
    basePositions: Float32Array;
    indices: Uint32Array;
    createEraMetrics: (era: string) => EraMetrics;
    debugEnabled?: boolean;
}

export interface RuntimeStore {
    world: WorldState;
    worldState: RuntimeState;
    getState: () => any;
    setState: (patch: any) => void;
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

    function setState(patch: any = {}) {
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
