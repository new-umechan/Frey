export function createTerrainGenerationController(options: any = {}) {
    const {
        seedForm,
        seedInput,
        worldSimController,
        level,
        terrainParams,
        world,
        worldState,
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
        onInitWorldStart = () => {},
        onInitWorldEnd = () => {},
    } = options;

    let generationToken = 0;
    const updateTerrain = async (seed) => {
        const token = ++generationToken;
        const nextSeed = seed.trim() || getCurrentSeed();

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        try {
            await onInitWorldStart();
            const initResult = worldSimController.init_world(nextSeed, level, {
                geology_params: terrainParams,
            });
            if (token !== generationToken) {
                return;
            }

            const currentEraMetrics = resetWorldProgress(
                world,
                worldState,
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
            onInitWorldEnd();
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
        }
    };

    return {
        updateTerrain,
    };
}
