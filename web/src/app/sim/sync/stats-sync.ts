import { type StatFields } from "../../../components/dom";
import { type WorldState } from "../../state/app-state";
import { type EngineClient, type MetricsResult } from "../../engine/engine-client";

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

    const metrics = await engineClient.get_metrics(worldId);
    if (!metrics) {
        return false;
    }

    world.tick = Math.floor(metrics.tick);
    world.engineView.tick = world.tick;
    world.engineView.era = String(metrics.era ?? world.era);
    world.engineView.budgets = {
        geology: Math.max(0, Math.floor(metrics.budgets.geology)),
        climate: Math.max(0, Math.floor(metrics.budgets.climate)),
        ecology: Math.max(0, Math.floor(metrics.budgets.ecology)),
        civilization: Math.max(0, Math.floor(metrics.budgets.civilization)),
    };
    statFields.level.textContent = `L${level}`;
    statFields.seed.textContent = currentSeed;
    statFields.plates.textContent = `${metrics.plate_count ?? 0}P`;
    statFields.land.textContent = formatPercent(metrics.land_ratio);

    return true;
}
