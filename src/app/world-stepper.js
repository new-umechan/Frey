export function createWorldStepper(options = {}) {
    const {
        worldSimController,
        world,
        worldState,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldDeltaFromController,
        syncVisibleCoreFieldsFromController,
        getDeltaFieldKindsForView,
        refreshWorldStats,
        saveDebugSnapshotIfNeeded,
        isDev,
        debugSnapshotTickSet,
        debugSnapshotSavedTicks,
        syncClimateUi,
        syncAfterWorldStep,
        setStatus,
        getCurrentState,
        pushStepBreakdownSamples,
        getEraScalePreset,
    } = options;

    const shouldRefreshStatsAtTick = (tick) => {
        return (tick % 8) === 0;
    };

    const getCurrentDeltaFieldKinds = () => {
        const state = getCurrentState();
        return getDeltaFieldKindsForView({
            viewMode: state.currentViewMode,
            climateMetric: state.currentClimateMetric,
        });
    };

    const syncVisibleFieldsForCurrentView = () => {
        const state = getCurrentState();
        if (!state.activeWorldId || !state.currentTerrainData) {
            return;
        }
        const changes = syncVisibleCoreFieldsFromController({
            worldSimController,
            worldId: state.activeWorldId,
            core: state.currentTerrainData,
            fieldKinds: getCurrentDeltaFieldKinds(),
        });
        terrainRenderer.applyCoreChanges(state.currentTerrainData, changes, state.currentSurfaceMode, world.tick);
    };

    const stepWorldTick = (perfRecorder = null, tickOptions = {}) => {
        const state = getCurrentState();
        if (!state.activeWorldId || !state.currentTerrainData) {
            return false;
        }

        const runTick = () => {
            const liveState = getCurrentState();
            const benchmarkMode = tickOptions?.benchmarkMode === true;
            const sampleStepBreakdown = tickOptions?.sampleStepBreakdown === true;
            const nextTick = world.tick + 1;
            const prevHeightForSnapshot = debugSnapshotTickSet.has(nextTick) && liveState.currentTerrainData?.heightData
                ? liveState.currentTerrainData.heightData.slice()
                : null;

            if (perfRecorder) {
                perfRecorder.measure("step_world", () => {
                    if (sampleStepBreakdown) {
                        const profiled = worldSimController.step_world_profiled(liveState.activeWorldId, 1);
                        pushStepBreakdownSamples(perfRecorder, profiled);
                        return;
                    }
                    worldSimController.step_world(liveState.activeWorldId, 1);
                });
            } else {
                worldSimController.step_world(liveState.activeWorldId, 1);
            }

            const shouldRefreshStats = benchmarkMode ? false : shouldRefreshStatsAtTick(nextTick);
            const { changes, statsRefreshed } = syncWorldDeltaFromController({
                worldSimController,
                worldId: liveState.activeWorldId,
                world,
                currentSurfaceMode: liveState.currentSurfaceMode,
                terrainRenderer,
                createEraMetrics,
                buildEraMetricsFromRuntime,
                setEraScale,
                refreshStats: shouldRefreshStats,
                refreshWorldStats,
                deltaFieldKinds: getCurrentDeltaFieldKinds(),
                perfRecorder,
            });
            if (!benchmarkMode && (changes?.climate || statsRefreshed)) {
                syncClimateUi();
            }

            if (!benchmarkMode) {
                void saveDebugSnapshotIfNeeded({
                    isDev,
                    tick: world.tick,
                    debugSnapshotTickSet,
                    debugSnapshotSavedTicks,
                    currentTerrainData: liveState.currentTerrainData,
                    currentSeed: liveState.currentSeed,
                    currentEraScale: liveState.currentEraScale,
                    world,
                    worldState,
                    prevHeightForSnapshot,
                    setStatus,
                });
            }

            if (!benchmarkMode && world.tick > 0 && shouldRefreshStats) {
                const preset = getEraScalePreset(liveState.currentEraScale);
                setStatus(
                    `Running (${liveState.currentSeed}) | ${preset.label} / 1Tick=${liveState.currentEraMetrics.tickLabel} | tick=${world.tick}`,
                );
            }
            if (!benchmarkMode) {
                syncAfterWorldStep();
            }
            return true;
        };

        if (perfRecorder) {
            return perfRecorder.measure("tick_total", runTick);
        }
        return runTick();
    };

    return {
        syncVisibleFieldsForCurrentView,
        stepWorldTick,
    };
}
