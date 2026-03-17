export function createTerrainGenerationController(options = {}) {
    const {
        seedForm,
        seedInput,
        worldSimController,
        level,
        terrainParams,
        world,
        worldState,
        debugSnapshotSavedTicks,
        createEmptyLayers,
        createInitialBudgets,
        createEraMetrics,
        resetWorldProgress,
        getEraScalePreset,
        setStatus,
        syncWorldFromActiveController,
        getCurrentEraScale,
        getCurrentSeed,
        setCurrentState,
        setPlaybackRunning,
        appendPlaybackEvent,
    } = options;

    let generationToken = 0;

    const updateTerrain = async (seed) => {
        const token = ++generationToken;
        const nextSeed = seed.trim() || getCurrentSeed();

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        try {
            const initResult = worldSimController.init_world(nextSeed, level, {
                terrain_params: terrainParams,
            });
            if (token !== generationToken) {
                return;
            }

            const currentEraMetrics = resetWorldProgress(
                world,
                worldState,
                debugSnapshotSavedTicks,
                createEmptyLayers,
                createInitialBudgets,
                createEraMetrics,
            );
            setCurrentState({
                currentSeed: nextSeed,
                activeWorldId: initResult.world_id,
                currentEraMetrics,
            });

            setPlaybackRunning(true);
            syncWorldFromActiveController();
            appendPlaybackEvent("world-generated", "地形生成", `seed=${nextSeed}`);

            const eraPreset = getEraScalePreset(getCurrentEraScale());
            setStatus(`Ready (${nextSeed}) | ${eraPreset.label} / 1Tick=${currentEraMetrics.tickLabel}`);
            seedInput.value = nextSeed;
            const activeElement = document.activeElement;
            if (activeElement instanceof HTMLElement && seedForm.contains(activeElement)) {
                activeElement.blur();
            }
        } finally {
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
        }
    };

    return {
        updateTerrain,
    };
}
