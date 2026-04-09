import { type EngineClient } from "../engine/engine-client";
import { type WorldState } from "../state/app-state";
import { type RuntimeState } from "../runtime/state";
import { type EraMetrics, type EraScaleConfig } from "../state/era-presets";

export interface TerrainGenerationController {
    updateTerrain: (seed: string) => Promise<void>;
}

export interface TerrainGenerationControllerOptions {
    seedForm: HTMLFormElement;
    seedInput: HTMLInputElement;
    worldSimController: EngineClient;
    level: number;
    terrainParams: any;
    world: WorldState;
    worldState: RuntimeState;
    createInitialBudgets: () => any;
    createEraMetrics: (era: string) => EraMetrics;
    resetWorldProgress: (
        world: WorldState,
        worldState: RuntimeState,
        createInitialBudgets: () => any,
        createEraMetrics: (era: string) => EraMetrics,
    ) => EraMetrics;
    getEraScalePreset: (era: string) => EraScaleConfig & { key: string };
    setStatus: (msg: string) => void;
    syncWorldFromActiveController: () => Promise<any>;
    getCurrentEraScale: () => string;
    getCurrentSeed: () => string;
    setCurrentState: (patch: any) => void;
    setPlaybackRunning: (isPlaying: boolean) => void;
    appendPlaybackEvent: (type: string, label: string, detail?: string) => void;
    onInitWorldStart?: () => Promise<void>;
    onInitWorldEnd?: () => void;
}

export function createTerrainGenerationController(options: TerrainGenerationControllerOptions): TerrainGenerationController {
    const {
        seedForm,
        seedInput,
        worldSimController,
        level,
        terrainParams,
        world,
        worldState,
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
    const updateTerrain = async (seed: string) => {
        const token = ++generationToken;
        const nextSeed = seed.trim() || getCurrentSeed();

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        try {
            await onInitWorldStart();
            const initResult = await worldSimController.init_world(nextSeed, level, {
                geology_params: terrainParams,
            });
            if (token !== generationToken) {
                return;
            }

            const currentEraMetrics = resetWorldProgress(
                world,
                worldState,
                createInitialBudgets,
                createEraMetrics,
            );
            setCurrentState({
                currentSeed: nextSeed,
                activeWorldId: initResult.world_id,
                currentEraMetrics,
            });

            setPlaybackRunning(true);
            await syncWorldFromActiveController();
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
