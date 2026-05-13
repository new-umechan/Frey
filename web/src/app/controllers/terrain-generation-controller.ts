import { type EngineClient } from "../engine/engine-client";
import { type AppState, type WorldState } from "../state/app-state";
import { type RuntimeState } from "../runtime/state";
import { type EraMetrics, type EraScaleConfig } from "../state/era-presets";
import { type SyncWorldResult } from "../sim/sync/types";
import { DEFAULT_CELL_METRIC, DEFAULT_VIEW_MODE } from "../../shared/constants";

export interface TerrainGenerationController {
    updateTerrain: (seed: string, options?: { devSnapshotStage?: string }) => Promise<void>;
}

export interface TerrainGenerationControllerOptions {
    seedForm: HTMLFormElement;
    seedInput: HTMLInputElement;
    engineClient: EngineClient;
    level: number;
    terrainParams: Record<string, number>;
    world: WorldState;
    worldState: RuntimeState;
    createInitialBudgets: () => Record<string, number>;
    createEraMetrics: (era: string) => EraMetrics;
    resetWorldProgress: (
        world: WorldState,
        worldState: RuntimeState,
        createInitialBudgets: () => Record<string, number>,
        createEraMetrics: (era: string) => EraMetrics,
    ) => EraMetrics;
    getEraScalePreset: (era: string) => EraScaleConfig & { key: string };
    setStatus: (msg: string) => void;
    syncWorldFromActiveController: () => Promise<SyncWorldResult | null>;
    getCurrentEraScale: () => string;
    getCurrentSeed: () => string;
    setActiveWorldId: (worldId: string | null) => void;
    setCurrentState: (patch: Partial<AppState>) => void;
    setCurrentEraMetrics: (metrics: EraMetrics) => void;
    setPlaybackRunning: (isPlaying: boolean) => void;
    appendPlaybackEvent: (type: string, label: string, detail?: string) => void;
    onViewStateReset?: () => void;
    onWorldInitialized?: (worldId: string) => Promise<void>;
    onInitWorldStart?: () => Promise<void>;
    onInitWorldEnd?: () => void;
}

export function createTerrainGenerationController(options: TerrainGenerationControllerOptions): TerrainGenerationController {
    const {
        seedForm,
        seedInput,
        engineClient,
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
        setActiveWorldId,
        setCurrentState,
        setCurrentEraMetrics,
        setPlaybackRunning,
        appendPlaybackEvent,
        onViewStateReset = () => {},
        onWorldInitialized = async () => {},
        onInitWorldStart = () => {},
        onInitWorldEnd = () => {},
    } = options;

    let generationToken = 0;
    const updateTerrain = async (seed: string, options?: { devSnapshotStage?: string }) => {
        const token = ++generationToken;
        const nextSeed = seed.trim() || getCurrentSeed();
        const requestedStage = options?.devSnapshotStage;

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        try {
            await onInitWorldStart();
            const initResult = await engineClient.init_world(nextSeed, level, {
                geology_params: terrainParams,
            }, {
                devSnapshotStage: requestedStage,
            });
            const worldId = (() => {
                const value = (initResult as { world_id?: unknown; worldId?: unknown }).world_id
                    ?? (initResult as { world_id?: unknown; worldId?: unknown }).worldId;
                return typeof value === "string" && value.length > 0 ? value : null;
            })();
            if (!worldId) {
                throw new Error("init_world response does not include a valid world id");
            }
            if (token !== generationToken) {
                return;
            }

            const currentEraMetrics = resetWorldProgress(
                world,
                worldState,
                createInitialBudgets,
                createEraMetrics,
            );
            setActiveWorldId(worldId);
            setCurrentState({
                currentSeed: nextSeed,
                currentViewMode: DEFAULT_VIEW_MODE,
                currentCellMetric: DEFAULT_CELL_METRIC,
            });
            setCurrentEraMetrics(currentEraMetrics);
            await onWorldInitialized(worldId);
            onViewStateReset();

            setPlaybackRunning(true);
            await syncWorldFromActiveController();
            appendPlaybackEvent("world-generated", "地形生成", `seed=${nextSeed}`);
            const snapshotStatus = (initResult as { dev_snapshot_restore_status?: unknown }).dev_snapshot_restore_status;
            const snapshotStage = (initResult as { dev_snapshot_stage?: unknown }).dev_snapshot_stage;
            const snapshotReason = (initResult as { dev_snapshot_reason?: unknown }).dev_snapshot_reason;
            if (snapshotStatus === "used" && typeof snapshotStage === "string" && snapshotStage.length > 0) {
                appendPlaybackEvent("dev-snapshot-used", "Dev Jump", `snapshot=${snapshotStage}`);
            } else if (snapshotStatus === "fallback" && typeof snapshotStage === "string" && snapshotStage.length > 0) {
                const reason = typeof snapshotReason === "string" && snapshotReason.length > 0
                    ? snapshotReason
                    : "unknown";
                appendPlaybackEvent("dev-snapshot-fallback", "Dev Jump", `fallback (${snapshotStage}): ${reason}`);
            }

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
