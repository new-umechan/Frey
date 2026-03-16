const FLOAT32_FIELDS = new Set([
    "height",
    "river_flux",
    "mantle_heat",
    "temperature",
    "precipitation",
    "runoff",
    "ocean_temperature",
]);

const CORE_FIELD_LOADERS = {
    heightData: "height",
    plateId: "plate_id",
    riverFlux: "river_flux",
    riverNext: "river_next",
    mantleHeat: "mantle_heat",
    temperature: "temperature",
    precipitation: "precipitation",
};

const WORLD_CHANGESET = Object.freeze({
    height: false,
    river: false,
    mantleHeat: false,
    climate: false,
});

const DELTA_FIELD_KIND_BY_VIEW = Object.freeze({
    normal: ["height", "river_flux", "river_next"],
    plates: ["height", "river_flux", "river_next"],
    mantle: ["height", "river_flux", "river_next", "mantle_heat"],
});

const CLIMATE_FIELD_KIND_BY_METRIC = Object.freeze({
    temperature: "temperature",
    precipitation: "precipitation",
});

const CORE_KEY_BY_FIELD_KIND = Object.freeze({
    height: "heightData",
    river_flux: "riverFlux",
    river_next: "riverNext",
    mantle_heat: "mantleHeat",
    temperature: "temperature",
    precipitation: "precipitation",
});

function getFieldData(controller, worldId, fieldKind) {
    const response = controller.get_field(worldId, fieldKind, 1);
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
}

function fetchCoreFields(worldSimController, worldId) {
    return Object.fromEntries(
        Object.entries(CORE_FIELD_LOADERS).map(([targetKey, fieldKind]) => [
            targetKey,
            getFieldData(worldSimController, worldId, fieldKind),
        ]),
    );
}

export function getDeltaFieldKindsForView({ viewMode, climateMetric }) {
    if (viewMode === "climate") {
        const climateField = CLIMATE_FIELD_KIND_BY_METRIC[climateMetric] ?? "temperature";
        return ["height", "river_flux", "river_next", climateField];
    }
    return DELTA_FIELD_KIND_BY_VIEW[viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal;
}

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
    temperature,
    precipitation,
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
        temperature,
        precipitation,
        plateInfo,
        targetLandRatio: Number.isFinite(targetLandRatio) ? targetLandRatio : 0,
        ...createEmptyCoreBuffers(cellCount),
    };
    rebuildVertexWeight(core);
    return core;
}

function applyWorldMetrics({
    world,
    metrics,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
}) {
    world.tick = Math.max(0, Math.floor(metrics.tick ?? 0));
    world.era = typeof metrics.era === "string" ? metrics.era : createEraMetrics().key;
    const nextEraMetrics = buildEraMetricsFromRuntime(world.era, metrics);
    world.budgets = { ...nextEraMetrics.budgets };
    setEraScale(world.era, nextEraMetrics);
}

function updateStatFields({ statFields, level, currentSeed, plateStats, metrics }) {
    statFields.level.textContent = `${level}`;
    statFields.seed.textContent = currentSeed;
    statFields.plates.textContent = `${Number(plateStats?.plate_count) || 0}`;
    statFields.land.textContent = `${((Number(metrics.land_ratio) || 0) * 100).toFixed(1)}%`;
}

function refreshCoreStatsFromMetrics(core, metrics, plateStats) {
    core.plateInfo = buildPlateInfoFromStats(plateStats);
    core.targetLandRatio = Number.isFinite(metrics.land_ratio) ? metrics.land_ratio : 0;
    rebuildVertexWeight(core);
}

function fetchWorldStats(worldSimController, worldId) {
    return {
        metrics: worldSimController.get_metrics(worldId),
        plateStats: worldSimController.get_plate_stats(worldId),
    };
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
    updateStatFields({
        statFields,
        level,
        currentSeed,
        plateStats: stats.plateStats,
        metrics: stats.metrics,
    });
    return stats;
}

function applyNumericDelta(target, fieldDelta) {
    const ranges = Array.isArray(fieldDelta?.ranges) ? fieldDelta.ranges : [];
    const values = fieldDelta?.f32_data ?? fieldDelta?.i32_data ?? [];
    const canFastCopy = typeof target?.set === "function" && ArrayBuffer.isView(values);
    if (fieldDelta?.mode === "full") {
        const copyLength = Math.min(target.length, values.length);
        if (canFastCopy) {
            target.set(values.subarray(0, copyLength), 0);
            return copyLength > 0;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[i] = values[i];
        }
        return copyLength > 0;
    }

    let offset = 0;
    for (const range of ranges) {
        const start = Math.max(0, Math.floor(range?.start ?? 0));
        const end = Math.min(target.length, Math.floor(range?.end ?? 0));
        if (end <= start) {
            continue;
        }
        const rangeLength = end - start;
        const copyLength = Math.max(0, Math.min(rangeLength, values.length - offset));
        if (canFastCopy && copyLength > 0) {
            target.set(values.subarray(offset, offset + copyLength), start);
            offset += rangeLength;
            continue;
        }
        for (let i = 0; i < copyLength; i += 1) {
            target[start + i] = values[offset + i];
        }
        offset += rangeLength;
    }
    return ranges.length > 0;
}

function applyWorldDeltaToCore(core, worldDelta) {
    const changes = { ...WORLD_CHANGESET };
    for (const delta of worldDelta?.deltas ?? []) {
        switch (delta?.field_kind) {
        case "height":
            changes.height = applyNumericDelta(core.heightData, delta);
            break;
        case "river_flux":
            changes.river = applyNumericDelta(core.riverFlux, delta) || changes.river;
            break;
        case "river_next":
            changes.river = applyNumericDelta(core.riverNext, delta) || changes.river;
            break;
        case "mantle_heat":
            changes.mantleHeat = applyNumericDelta(core.mantleHeat, delta);
            break;
        case "temperature":
            changes.climate = applyNumericDelta(core.temperature, delta) || changes.climate;
            break;
        case "precipitation":
            changes.climate = applyNumericDelta(core.precipitation, delta) || changes.climate;
            break;
        default:
            break;
        }
    }
    return changes;
}

function applyFieldSnapshotToCore(core, fieldKind, values, changes) {
    const coreKey = CORE_KEY_BY_FIELD_KIND[fieldKind];
    if (!coreKey || !(coreKey in core)) {
        return;
    }
    core[coreKey] = values;
    if (fieldKind === "height") {
        changes.height = true;
        return;
    }
    if (fieldKind === "river_flux" || fieldKind === "river_next") {
        changes.river = true;
        return;
    }
    if (fieldKind === "mantle_heat") {
        changes.mantleHeat = true;
        return;
    }
    if (fieldKind === "temperature" || fieldKind === "precipitation") {
        changes.climate = true;
    }
}

export function syncVisibleCoreFieldsFromController({
    worldSimController,
    worldId,
    core,
    fieldKinds,
}) {
    const changes = { ...WORLD_CHANGESET };
    const uniqueFieldKinds = Array.from(new Set(fieldKinds ?? []));
    for (const fieldKind of uniqueFieldKinds) {
        const values = getFieldData(worldSimController, worldId, fieldKind);
        applyFieldSnapshotToCore(core, fieldKind, values, changes);
    }
    return changes;
}

function syncWorldState({
    world,
    metrics,
    core,
    currentSurfaceMode,
    terrainRenderer,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
    initializeTerrain = false,
    changes = WORLD_CHANGESET,
    perfRecorder = null,
}) {
    world.core = core;
    applyWorldMetrics({
        world,
        metrics,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
    });
    if (initializeTerrain) {
        terrainRenderer.initializeTerrain(core, currentSurfaceMode);
    } else {
        terrainRenderer.applyCoreChanges(
            core,
            changes,
            currentSurfaceMode,
            world.tick,
            perfRecorder,
        );
    }
    return {
        changes,
        metrics,
    };
}

function maybeRefreshStats({ refreshStats, refreshWorldStats }) {
    if (!refreshStats || typeof refreshWorldStats !== "function") {
        return { statsRefreshed: false, stats: null };
    }
    return {
        statsRefreshed: true,
        stats: refreshWorldStats(),
    };
}

export function syncWorldFromController({
    worldSimController,
    worldId,
    world,
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
    const stats = fetchWorldStats(worldSimController, worldId);
    const core = buildCoreFromController({
        ...fetchCoreFields(worldSimController, worldId),
        plateInfo: buildPlateInfoFromStats(stats.plateStats),
        targetLandRatio: stats.metrics.land_ratio,
    });
    setCurrentTerrainData(core);
    const result = syncWorldState({
        world,
        metrics: stats.metrics,
        core,
        currentSurfaceMode,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        initializeTerrain: true,
    });
    updateStatFields({
        statFields,
        level,
        currentSeed,
        plateStats: stats.plateStats,
        metrics: stats.metrics,
    });
    return {
        ...result,
        statsRefreshed: true,
        stats,
    };
}

export function syncWorldDeltaFromController({
    worldSimController,
    worldId,
    world,
    currentSurfaceMode,
    terrainRenderer,
    createEraMetrics,
    buildEraMetricsFromRuntime,
    setEraScale,
    refreshStats,
    refreshWorldStats,
    deltaFieldKinds,
    perfRecorder = null,
}) {
    if (!world.core) {
        return {
            changes: null,
            statsRefreshed: false,
            metrics: null,
            stats: null,
        };
    }

    const deltaTask = () => {
        const worldDelta = worldSimController.get_world_delta(
            worldId,
            Array.isArray(deltaFieldKinds) && deltaFieldKinds.length > 0
                ? { include_fields: deltaFieldKinds }
                : undefined,
        );
        const changes = applyWorldDeltaToCore(world.core, worldDelta);
        return { worldDelta, changes };
    };
    const {
        worldDelta,
        changes,
    } = perfRecorder ? perfRecorder.measure("delta_sync", deltaTask) : deltaTask();
    const result = syncWorldState({
        world,
        metrics: worldDelta,
        core: world.core,
        currentSurfaceMode,
        terrainRenderer,
        createEraMetrics,
        buildEraMetricsFromRuntime,
        setEraScale,
        changes,
        perfRecorder,
    });
    const refreshed = maybeRefreshStats({ refreshStats, refreshWorldStats });
    return {
        ...result,
        ...refreshed,
    };
}
