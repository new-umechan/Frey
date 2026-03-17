export function createWorldSessionController(options = {}) {
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
        return refreshWorldStatsFromController({
            worldSimController,
            worldId: getActiveWorldId(),
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
