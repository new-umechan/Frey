import { type StatFields } from "../../../components/dom";
import { type WorldState } from "../../state/app-state";
import { type EngineClient } from "../../engine/engine-client";

interface WorldMetricsResponse {
    tick?: number;
    era_scale?: string;
    budget_geology?: number;
    budget_climate?: number;
    budget_ecology?: number;
    budget_civilization?: number;
    plate_count?: number;
    land_ratio?: number;
}

function formatPercent(ratio: number): string {
    if (!Number.isFinite(ratio)) {
        return "- %";
    }
    return `${(ratio * 100).toFixed(1)}%`;
}

export async function refreshWorldStatsFromController(options: {
    engineClient: EngineClient;
    worldId: string | null;
    world: WorldState;
    currentSeed: string;
    statFields: StatFields;
    level: number;
}) {
    const { engineClient, worldId, world, currentSeed, statFields, level } = options;
    if (!worldId) {
        return false;
    }

    const metrics = await engineClient.get_metrics(worldId) as WorldMetricsResponse | null;
    if (!metrics) {
        return false;
    }

    world.tick = Math.floor(metrics.tick ?? 0);
    world.engineView.tick = world.tick;
    world.engineView.era = String(metrics.era_scale ?? world.era);
    world.engineView.budgets = {
        geology: Math.max(0, Math.floor(metrics?.budget_geology ?? world.engineView.budgets.geology ?? 0)),
        climate: Math.max(0, Math.floor(metrics?.budget_climate ?? world.engineView.budgets.climate ?? 0)),
        ecology: Math.max(0, Math.floor(metrics?.budget_ecology ?? world.engineView.budgets.ecology ?? 0)),
        civilization: Math.max(0, Math.floor(metrics?.budget_civilization ?? world.engineView.budgets.civilization ?? 0)),
    };
    statFields.level.textContent = `L${level}`;
    statFields.seed.textContent = currentSeed;
    statFields.plates.textContent = `${metrics.plate_count ?? 0}P`;
    statFields.land.textContent = formatPercent(metrics.land_ratio ?? 0);

    return true;
}
