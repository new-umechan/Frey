import { refreshCoreStatsFromMetrics } from "./core-builders.js";

export function fetchWorldStats(worldSimController, worldId) {
    return {
        metrics: worldSimController.get_metrics(worldId),
        plateStats: worldSimController.get_plate_stats(worldId),
    };
}

function updateStatFields({ statFields, level, currentSeed, plateStats, metrics }) {
    statFields.level.textContent = `${level}`;
    statFields.seed.textContent = currentSeed;
    statFields.plates.textContent = `${Number(plateStats?.plate_count) || 0}`;
    statFields.land.textContent = `${((Number(metrics.land_ratio) || 0) * 100).toFixed(1)}%`;
}

export function updateUiStatsFromWorldStats({ stats, currentSeed, statFields, level }) {
    updateStatFields({
        statFields,
        level,
        currentSeed,
        plateStats: stats.plateStats,
        metrics: stats.metrics,
    });
}

export function refreshWorldStatsFromController({
    worldSimController,
    worldId,
    world,
    currentSeed,
    statFields,
    level,
}) {
    if (!world.core) {
        return null;
    }
    const stats = fetchWorldStats(worldSimController, worldId);
    refreshCoreStatsFromMetrics(world.core, stats.metrics, stats.plateStats);
    updateUiStatsFromWorldStats({ stats, currentSeed, statFields, level });
    return stats;
}
