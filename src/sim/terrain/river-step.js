import { RIVER_BUDGET_SCALE_BY_ERA, SUBSYSTEM_ACTIVITY_SIGNAL_GAIN } from "../../core/constants.js";
import { recordSubsystemActivity } from "../runtime/activity.js";
import { step_erosion_automaton } from "../../interface/wasm.js";
import { applyLandRatioFloor, syncTerrainHeightToErosionState } from "./core-step.js";

function computeRiverBudgetCells(currentTerrainData, currentEraScale, riverWeight) {
    if (!currentTerrainData || !Number.isFinite(riverWeight) || riverWeight <= 0) {
        return 0;
    }
    const scale = RIVER_BUDGET_SCALE_BY_ERA[currentEraScale] ?? RIVER_BUDGET_SCALE_BY_ERA.crust;
    const vertexBudgetBase = Math.max(64, Math.floor(currentTerrainData.heightData.length * 0.01));
    return Math.max(1, Math.floor(vertexBudgetBase * riverWeight * scale));
}

function applyErosionAutomatonStateToTerrain({
    currentTerrainData,
    worldState,
    currentEraScale,
    erosionState,
    applyTerrainVisualUpdates,
}) {
    if (!currentTerrainData || !erosionState) {
        return;
    }

    const nextHeight = new Float32Array(erosionState.height);
    const nextRiverFlux = new Float32Array(erosionState.river_flux);
    const nextRiverNext = new Int32Array(erosionState.river_next);
    if (
        nextHeight.length !== currentTerrainData.heightData.length ||
        nextRiverFlux.length !== currentTerrainData.riverFlux.length ||
        nextRiverNext.length !== currentTerrainData.riverNext.length
    ) {
        return;
    }

    const landPreserveDelta = applyLandRatioFloor(
        nextHeight,
        currentTerrainData.plateId,
        currentTerrainData.plateInfo?.isOcean,
        currentTerrainData.targetLandRatio,
        currentEraScale,
    );
    currentTerrainData.heightData = nextHeight;
    currentTerrainData.riverFlux = nextRiverFlux;
    currentTerrainData.riverNext = nextRiverNext;
    if (landPreserveDelta > 0) {
        syncTerrainHeightToErosionState(worldState, currentTerrainData);
    }

    applyTerrainVisualUpdates();
}

function estimateRiverActivitySignal(erosionState, currentTerrainData) {
    if (!erosionState || !currentTerrainData) {
        return 0;
    }
    const changedCount = Array.isArray(erosionState.recent_changed)
        ? erosionState.recent_changed.length
        : 0;
    const cellCount = Math.max(1, currentTerrainData.heightData?.length ?? 1);
    return Math.min(1, changedCount / cellCount);
}

function stepRiverForCurrentTick({
    worldState,
    currentTerrainData,
    currentEraScale,
    preset,
}) {
    if (!worldState.erosionAutomatonState || !preset) {
        return;
    }
    const budgetCells = computeRiverBudgetCells(currentTerrainData, currentEraScale, preset.weights.river ?? 0);
    if (budgetCells <= 0) {
        return;
    }

    worldState.erosionAutomatonState = step_erosion_automaton(
        worldState.erosionAutomatonState,
        budgetCells,
    );
    worldState.executedSteps.river += 1;
    recordSubsystemActivity(
        worldState,
        "river",
        estimateRiverActivitySignal(worldState.erosionAutomatonState, currentTerrainData) *
            SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.river,
    );
    worldState.terrainErosionDirty = true;
}

export function enqueueRiverStep(worldState, steps) {
    if (!Number.isFinite(steps) || steps <= 0) {
        return;
    }
    worldState.pendingRiverSteps += steps;
}

export function drainRiverQueue({
    worldState,
    currentTerrainData,
    currentEraScale,
    preset,
    applyTerrainVisualUpdates,
}) {
    if (!preset || worldState.pendingRiverSteps <= 0) {
        return;
    }

    const maxRiverStepsPerFrame = Math.max(1, worldState.maxRiverStepsPerFrame ?? 1);
    let drained = 0;
    while (worldState.pendingRiverSteps > 0 && drained < maxRiverStepsPerFrame) {
        stepRiverForCurrentTick({
            worldState,
            currentTerrainData,
            currentEraScale,
            preset,
        });
        worldState.pendingRiverSteps -= 1;
        drained += 1;
    }

    if (worldState.terrainErosionDirty) {
        applyErosionAutomatonStateToTerrain({
            currentTerrainData,
            worldState,
            currentEraScale,
            erosionState: worldState.erosionAutomatonState,
            applyTerrainVisualUpdates,
        });
        worldState.terrainErosionDirty = false;
    }
}
