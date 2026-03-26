export function createWorldStepper(options = {}) {
    const {
        worldSimController,
        world,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        syncWorldDeltaFromController,
        syncVisibleCoreFieldsFromController,
        getDeltaFieldKindsForView,
        refreshWorldStats,
        syncClimateUi,
        syncAfterWorldStep,
        setStatus,
        getCurrentState,
        pushStepBreakdownSamples,
        getEraScalePreset,
    } = options;

    const shouldRefreshStatsForAdvance = (previousTick, nextTick) => {
        const safePrev = Math.max(0, Math.floor(previousTick ?? 0));
        const safeNext = Math.max(safePrev, Math.floor(nextTick ?? safePrev));
        if (safeNext <= safePrev) {
            return false;
        }
        return Math.floor(safePrev / 8) < Math.floor(safeNext / 8);
    };

    const getCurrentDeltaFieldKinds = () => {
        const state = getCurrentState();
        return getDeltaFieldKindsForView({
            viewMode: state.currentViewMode,
            cellMetric: state.currentCellMetric,
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
            const batchCount = Math.max(1, Math.floor(tickOptions?.batchCount ?? 1));
            const previousTick = world.tick;
            const nextTick = previousTick + batchCount;

            if (perfRecorder) {
                perfRecorder.measure("exec_world", () => {
                    if (sampleStepBreakdown) {
                        const profiled = worldSimController.exec_world_profiled(liveState.activeWorldId, batchCount);
                        pushStepBreakdownSamples(perfRecorder, profiled);
                        return;
                    }
                    worldSimController.exec_world(liveState.activeWorldId, batchCount);
                });
            } else {
                worldSimController.exec_world(liveState.activeWorldId, batchCount);
            }

            const shouldRefreshStats = benchmarkMode ? false : shouldRefreshStatsForAdvance(previousTick, nextTick);
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
            if (!benchmarkMode && (changes?.metric || statsRefreshed)) {
                syncClimateUi();
            }

            if (!benchmarkMode && world.tick > 0 && shouldRefreshStats) {
                const preset = getEraScalePreset(liveState.currentEraScale);
                setStatus(
                    `Running (${liveState.currentSeed}) | ${preset.label} / 1Tick=${liveState.currentEraMetrics.tickLabel} | tick=${world.tick}`,
                );
            }
            if (!benchmarkMode) {
                syncAfterWorldStep({
                    previousTick,
                    nextTick: world.tick,
                    ticksAdvanced: batchCount,
                    batched: tickOptions?.batched === true,
                });
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
