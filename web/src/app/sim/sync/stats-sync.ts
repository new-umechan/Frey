import { type StatFields } from "../../../components/dom";

function formatPercent(ratio: number): string {
    if (!Number.isFinite(ratio)) {
        return "- %";
    }
    return `${(ratio * 100).toFixed(1)}%`;
}

export async function refreshWorldStatsFromController(options: {
    worldSimController: any;
    worldId: string | null;
    world: any;
    currentSeed: string;
    statFields: StatFields;
    level: number;
}) {
    const { worldSimController, worldId, world, currentSeed, statFields, level } = options;
    if (!worldId) {
        return false;
    }

    const metrics = await worldSimController.get_metrics(worldId);
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
