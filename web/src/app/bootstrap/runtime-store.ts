import { DEFAULT_ERA_SCALE } from "../../shared/constants";
import { createMutableStateStore, createWorldState, type AppState, type WorldState } from "../state/app-state";
import { type EraMetrics } from "../state/era-presets";
import { type RuntimeState } from "../runtime/state";
import { type CoreBuffers } from "../sim/sync/types";
import { setYearsPerTick } from "../ui/sim-time";
import { setEraBudgets } from "../state/era-runtime";

export interface RuntimeStoreOptions {
    basePositions: Float32Array;
    indices: Uint32Array;
    createEraMetrics: (era: string) => EraMetrics;
    initialSeed?: string;
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
        initialSeed,
    } = options;

    let currentEraMetrics = createEraMetrics(DEFAULT_ERA_SCALE);
    const { world, worldState } = createWorldState({
        basePositions,
        indices,
        currentEraMetrics,
    });
    const mutableStateStore = createMutableStateStore({
        initialSeed,
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
            // runtime 由来の実年数を年前表示へ反映(全 era metrics 更新の中心経路)。
            setYearsPerTick(nextMetrics.realYearsPerTick);
            // 計算予算を共有(計算されていない指標のグレーアウトに使う)。
            setEraBudgets(nextMetrics.budgets);
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
