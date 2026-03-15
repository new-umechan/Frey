export function getFieldData(controller, worldId, fieldKind) {
    const response = controller.get_field(worldId, fieldKind, 1);
    if (fieldKind === "height" || fieldKind === "river_flux" || fieldKind === "mantle_heat") {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
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

export function buildCoreFromController({
    heightData,
    plateId,
    riverFlux,
    riverNext,
    mantleHeat,
    plateInfo,
    targetLandRatio,
}) {
    const cellCount = heightData.length;
    const vertexWeight = new Float32Array(cellCount);
    for (let i = 0; i < cellCount; i += 1) {
        const pid = plateId[i];
        const weight = pid >= 0 && pid < plateInfo.baseWeight.length
            ? plateInfo.baseWeight[pid]
            : 0.5;
        vertexWeight[i] = weight;
    }

    return {
        heightData,
        plateId,
        riverFlux,
        riverNext,
        mantleHeat,
        lakeDepth: new Float32Array(cellCount),
        plateInfo,
        vertexWeight,
        tectonicDebug: {
            trench: new Float32Array(cellCount),
            arc: new Float32Array(cellCount),
            backarc: new Float32Array(cellCount),
            oceanOceanArc: new Float32Array(cellCount),
        },
        targetLandRatio: Number.isFinite(targetLandRatio) ? targetLandRatio : 0,
    };
}

export function syncWorldFromController({
    worldSimController,
    worldId,
    world,
    basePositions,
    currentSeed,
    currentSurfaceMode,
    terrainRenderer,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
    setCurrentTerrainData,
    statFields,
    level,
}) {
    const metrics = worldSimController.get_metrics(worldId);
    const plateStats = worldSimController.get_plate_stats(worldId);
    const heightData = getFieldData(worldSimController, worldId, "height");
    const riverFlux = getFieldData(worldSimController, worldId, "river_flux");
    const plateId = getFieldData(worldSimController, worldId, "plate_id");
    const riverNext = getFieldData(worldSimController, worldId, "river_next");
    const mantleHeat = getFieldData(worldSimController, worldId, "mantle_heat");

    const plateInfo = buildPlateInfoFromStats(plateStats);
    const core = buildCoreFromController({
        heightData,
        plateId,
        riverFlux,
        riverNext,
        mantleHeat,
        plateInfo,
        targetLandRatio: metrics.land_ratio,
    });

    world.tick = Math.max(0, Math.floor(metrics.tick ?? 0));
    world.era = typeof metrics.era === "string" ? metrics.era : createEraMetrics().key;
    const nextEraMetrics = buildEraMetricsFromRuntime(world.era, metrics);
    world.budgets = { ...nextEraMetrics.budgets };
    world.core = core;
    setCurrentTerrainData(core);

    setEraScale(world.era, nextEraMetrics);

    terrainRenderer.updateTerrainAttributes(core);
    terrainRenderer.updateRiverMaskTexture(core);
    terrainRenderer.updateGeometryPositions(core, currentSurfaceMode);

    statFields.vertices.textContent = `${basePositions.length / 3}`;
    statFields.level.textContent = `${level}`;
    statFields.seed.textContent = currentSeed;
    statFields.plates.textContent = `${Number(plateStats?.plate_count) || 0}`;
    statFields.land.textContent = `${((Number(metrics.land_ratio) || 0) * 100).toFixed(1)}%`;
}
