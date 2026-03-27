import { type WorldState } from "../core/app-state";
import { type EraMetrics } from "../core/era-presets";
import { type StatFields } from "../../ui/dom";

export interface WorldSessionControllerOptions {
    worldSimController: any;
    world: WorldState;
    terrainRenderer: any;
    createEraMetrics: (era: string) => EraMetrics;
    buildEraMetricsFromRuntime: (era: string, metrics: any) => EraMetrics;
    setEraScale: (era: string) => void;
    syncWorldFromController: (options: any) => any;
    refreshWorldStatsFromController: (options: any) => any;
    setCurrentTerrainData: (data: any) => void;
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
        worldSimController,
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

    const syncWorldFromActiveController = () => {
        const worldId = getActiveWorldId();
        if (!worldId) {
            return null;
        }
        const result = syncWorldFromController({
            worldSimController,
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

    const refreshActiveWorldStats = () => {
        const worldId = getActiveWorldId();
        if (!worldId) {
            return null;
        }
        return refreshWorldStatsFromController({
            worldSimController,
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
