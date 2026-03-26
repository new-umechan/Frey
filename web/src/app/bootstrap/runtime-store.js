import { DEFAULT_ERA_SCALE } from "../../core/constants.js";
import { createMutableStateStore, createWorldState } from "../core/app-state.js";

export function createRuntimeStore(options = {}) {
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

    function setState(patch = {}) {
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
