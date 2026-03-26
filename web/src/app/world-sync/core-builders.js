function createEmptyCoreBuffers(cellCount) {
    return {
        lakeDepth: new Float32Array(cellCount),
        vertexWeight: new Float32Array(cellCount),
        tectonicDebug: {
            trench: new Float32Array(cellCount),
            arc: new Float32Array(cellCount),
            backarc: new Float32Array(cellCount),
            oceanOceanArc: new Float32Array(cellCount),
        },
    };
}

export function buildPlateInfoFromStats(plateStats) {
    const plateCount = Math.max(0, Number(plateStats?.plate_count) || 0);
    const stats = Array.isArray(plateStats?.stats) ? plateStats.stats : [];
    const isOcean = new Uint8Array(plateCount);
    const baseHeight = new Float32Array(plateCount);
    const baseWeight = new Float32Array(plateCount);

    let maxCellCount = 1;
    for (const stat of stats) {
        const cellCount = Math.max(0, Number(stat?.cell_count) || 0);
        if (cellCount > maxCellCount) {
            maxCellCount = cellCount;
        }
    }

    for (const stat of stats) {
        const plateId = Number(stat?.plate_id);
        if (!Number.isInteger(plateId) || plateId < 0 || plateId >= plateCount) {
            continue;
        }
        const meanHeight = Number(stat?.mean_height);
        const cellCount = Math.max(0, Number(stat?.cell_count) || 0);
        isOcean[plateId] = Number.isFinite(meanHeight) && meanHeight <= 0 ? 1 : 0;
        baseHeight[plateId] = Number.isFinite(meanHeight) ? meanHeight : 0;
        baseWeight[plateId] = Math.max(0.05, Math.min(1.0, cellCount / maxCellCount));
    }

    return {
        isOcean,
        baseHeight,
        baseWeight,
    };
}

function rebuildVertexWeight(core) {
    if (!core?.plateId || !core?.plateInfo) {
        return;
    }
    const { plateId, plateInfo } = core;
    const vertexWeight = core.vertexWeight ?? new Float32Array(plateId.length);
    for (let i = 0; i < plateId.length; i += 1) {
        const pid = plateId[i];
        vertexWeight[i] = pid >= 0 && pid < plateInfo.baseWeight.length
            ? plateInfo.baseWeight[pid]
            : 0.5;
    }
    core.vertexWeight = vertexWeight;
}

export function buildCoreFromController({
    heightData,
    plateId,
    riverFlux,
    riverNext,
    mantleHeat,
    erosionRate,
    depositionRate,
    temperature,
    precipitation,
    evapotranspiration,
    aridity,
    runoff,
    oceanTemperature,
    riverTransportCost,
    plateInfo,
    targetLandRatio,
}) {
    const cellCount = heightData.length;
    const core = {
        heightData,
        plateId,
        riverFlux,
        riverNext,
        mantleHeat,
        erosionRate,
        depositionRate,
        temperature,
        precipitation,
        evapotranspiration,
        aridity,
        runoff,
        oceanTemperature,
        riverTransportCost,
        plateInfo,
        targetLandRatio: Number.isFinite(targetLandRatio) ? targetLandRatio : 0,
        ...createEmptyCoreBuffers(cellCount),
    };
    rebuildVertexWeight(core);
    return core;
}

export function refreshCoreStatsFromMetrics(core, metrics, plateStats) {
    core.plateInfo = buildPlateInfoFromStats(plateStats);
    core.targetLandRatio = Number.isFinite(metrics.land_ratio) ? metrics.land_ratio : 0;
    rebuildVertexWeight(core);
}
