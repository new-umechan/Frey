import { type WorldState } from "../state/app-state";
import { type EraMetrics } from "../state/era-presets";
import { type StatFields } from "../../components/dom";
import { type EngineClient, type MetricsResult } from "../engine/engine-client";
import { type TerrainRenderer } from "../visualizers/terrain-renderer";
import { type SyncOptions, type CoreBuffers, type SyncWorldResult } from "../sim/sync/types";

export interface WorldSessionControllerOptions {
    engineClient: EngineClient;
    world: WorldState;
    terrainRenderer: TerrainRenderer;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: MetricsResult) => EraMetrics;
    setEraScale: (era: string) => void;
    syncWorldFromController: (options: SyncOptions) => Promise<SyncWorldResult | null>;
    refreshWorldStatsFromController: (options: {
        engineClient: WorldSessionControllerOptions["engineClient"];
        worldId: string | null;
        world: WorldState;
        currentSeed: string;
        statFields: StatFields;
        level: number;
    }) => Promise<boolean>;
    setCurrentTerrainData: (data: CoreBuffers) => void;
    syncClimateUi: () => void;
    hidePlateHover: () => void;
    syncAfterWorldSync: () => void;
    getCurrentSeed: () => string;
    getCurrentSurfaceMode: () => string;
    getActiveWorldId: () => string | null;
    statFields: StatFields;
    level: number;
}

export function createWorldSessionController(options: WorldSessionControllerOptions) {
    const {
        engineClient,
        world,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldFromController,
        refreshWorldStatsFromController,
        setCurrentTerrainData,
        syncClimateUi,
        hidePlateHover,
        syncAfterWorldSync,
        getCurrentSeed,
        getCurrentSurfaceMode,
        getActiveWorldId,
        statFields,
        level,
    } = options;

    const syncWorldFromActiveController = async (): Promise<SyncWorldResult | null> => {
        const worldId = getActiveWorldId();
        if (!worldId) {
            return null;
        }
        const result = await syncWorldFromController({
            engineClient,
            worldId,
            world,
            currentSeed: getCurrentSeed(),
            currentSurfaceMode: getCurrentSurfaceMode(),
            terrainRenderer,
            createEraMetrics,
            buildEraMetricsFromRuntime,
            setEraScale,
            setCurrentTerrainData,
            statFields,
            level,
        });
        syncClimateUi();
        hidePlateHover();
        syncAfterWorldSync();
        return result;
    };

    const refreshActiveWorldStats = async (): Promise<boolean> => {
        const worldId = getActiveWorldId();
        if (!worldId) {
            return false;
        }
        return refreshWorldStatsFromController({
            engineClient,
            worldId,
            world,
            currentSeed: getCurrentSeed(),
            statFields,
            level,
        });
    };

    return {
        syncWorldFromActiveController,
        refreshActiveWorldStats,
    };
}
