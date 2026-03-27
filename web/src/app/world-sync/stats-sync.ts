import { type StatFields } from "../../ui/dom.js";

function formatCompact(value: number): string {
    if (!Number.isFinite(value)) {
        return "-";
    }
    if (value >= 1000000) {
        return `${(value / 1000000).toFixed(1)}M`;
    }
    if (value >= 1000) {
        return `${(value / 1000).toFixed(1)}k`;
    }
    return Math.floor(value).toString();
}

function formatPercent(ratio: number): string {
    if (!Number.isFinite(ratio)) {
        return "- %";
    }
    return `${(ratio * 100).toFixed(1)}%`;
}

export function refreshWorldStatsFromController(options: {
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

    const metrics = worldSimController.get_metrics(worldId);
    if (!metrics) {
        return false;
    }

    world.tick = Math.floor(metrics.tick ?? 0);
    statFields.level.textContent = `L${level}`;
    statFields.seed.textContent = currentSeed;
    statFields.plates.textContent = `${metrics.plate_count ?? 0}P`;
    statFields.land.textContent = formatPercent(metrics.land_ratio ?? 0);

    return true;
}
