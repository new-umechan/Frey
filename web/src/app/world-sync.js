import { getCellMetricMeta } from "./cell-metric.js";

const FLOAT32_FIELDS = new Set([
    "height",
    "river_flux",
    "mantle_heat",
    "erosion_rate",
    "deposition_rate",
    "temperature",
    "precipitation",
    "evapotranspiration",
    "aridity",
    "runoff",
    "ocean_temperature",
    "river_transport_cost",
]);

const OPTIONAL_FIELD_KINDS = new Set([
    "erosion_rate",
    "deposition_rate",
    "evapotranspiration",
    "aridity",
    "river_transport_cost",
    "runoff",
    "ocean_temperature",
]);

const WORLD_CHANGESET = Object.freeze({
    height: false,
    river: false,
    mantleHeat: false,
    metric: false,
});

const DELTA_FIELD_KIND_BY_VIEW = Object.freeze({
    normal: ["height", "river_flux", "river_next"],
    metric: ["height", "river_flux", "river_next"],
});

const CORE_KEY_BY_FIELD_KIND = Object.freeze({
    height: "heightData",
    river_flux: "riverFlux",
    river_next: "riverNext",
    mantle_heat: "mantleHeat",
    erosion_rate: "erosionRate",
    deposition_rate: "depositionRate",
    temperature: "temperature",
    precipitation: "precipitation",
    evapotranspiration: "evapotranspiration",
    aridity: "aridity",
    runoff: "runoff",
    ocean_temperature: "oceanTemperature",
    river_transport_cost: "riverTransportCost",
});

const CHANGE_KIND_BY_FIELD_KIND = Object.freeze({
    height: "height",
    river_flux: "river",
    river_next: "river",
    mantle_heat: "mantleHeat",
    erosion_rate: "metric",
    deposition_rate: "metric",
    temperature: "metric",
    precipitation: "metric",
    evapotranspiration: "metric",
    aridity: "metric",
    runoff: "metric",
    ocean_temperature: "metric",
    river_transport_cost: "metric",
});

function createWorldChangeset() {
    return { ...WORLD_CHANGESET };
}

function markFieldChange(changes, fieldKind) {
    const changeKey = CHANGE_KIND_BY_FIELD_KIND[fieldKind];
    if (changeKey) {
        changes[changeKey] = true;
    }
}

function createFallbackFieldData(fieldKind, fallbackCellCount) {
    const count = Math.max(0, Math.floor(fallbackCellCount || 0));
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(count);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(count);
    }
    return new Int32Array(count);
}

function getFieldData(controller, worldId, fieldKind, fallbackCellCount = 0) {
    let response = null;
    try {
        response = controller.get_field(worldId, fieldKind, 1);
    } catch (error) {
        if (OPTIONAL_FIELD_KINDS.has(fieldKind)) {
            console.warn(`[world-sync] optional field fallback: ${fieldKind}`, error);
            return createFallbackFieldData(fieldKind, fallbackCellCount);
        }
        throw error;
    }
    if (FLOAT32_FIELDS.has(fieldKind)) {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
}

function fetchCoreFields(worldSimController, worldId) {
    const heightData = getFieldData(worldSimController, worldId, "height");
    const cellCount = heightData.length;
    return {
        heightData,
        plateId: getFieldData(worldSimController, worldId, "plate_id", cellCount),
        riverFlux: getFieldData(worldSimController, worldId, "river_flux", cellCount),
        riverNext: getFieldData(worldSimController, worldId, "river_next", cellCount),
        mantleHeat: getFieldData(worldSimController, worldId, "mantle_heat", cellCount),
        erosionRate: getFieldData(worldSimController, worldId, "erosion_rate", cellCount),
        depositionRate: getFieldData(worldSimController, worldId, "deposition_rate", cellCount),
        temperature: getFieldData(worldSimController, worldId, "temperature", cellCount),
        precipitation: getFieldData(worldSimController, worldId, "precipitation", cellCount),
        evapotranspiration: getFieldData(worldSimController, worldId, "evapotranspiration", cellCount),
        aridity: getFieldData(worldSimController, worldId, "aridity", cellCount),
        runoff: getFieldData(worldSimController, worldId, "runoff", cellCount),
        oceanTemperature: getFieldData(worldSimController, worldId, "ocean_temperature", cellCount),
        riverTransportCost: getFieldData(worldSimController, worldId, "river_transport_cost", cellCount),
    };
}

export function getDeltaFieldKindsForView({ viewMode, cellMetric }) {
    if (viewMode === "metric") {
        const meta = getCellMetricMeta(cellMetric);
        return ["height", "river_flux", "river_next", meta.fieldKind];
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

function updateUiStatsFromWorldStats({ stats, currentSeed, statFields, level }) {
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
    const changes = createWorldChangeset();
    for (const delta of worldDelta?.deltas ?? []) {
        const fieldKind = delta?.field_kind;
        const coreKey = CORE_KEY_BY_FIELD_KIND[fieldKind];
        if (!coreKey || !(coreKey in core)) {
            continue;
        }
        const didChange = applyNumericDelta(core[coreKey], delta);
        if (didChange) {
            markFieldChange(changes, fieldKind);
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
    markFieldChange(changes, fieldKind);
}

export function syncVisibleCoreFieldsFromController({
    worldSimController,
    worldId,
    core,
    fieldKinds,
}) {
    const changes = createWorldChangeset();
    const uniqueFieldKinds = Array.from(new Set(fieldKinds ?? []));
    for (const fieldKind of uniqueFieldKinds) {
        const values = getFieldData(
            worldSimController,
            worldId,
            fieldKind,
            core?.heightData?.length ?? 0,
        );
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
    updateUiStatsFromWorldStats({ stats, currentSeed, statFields, level });
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
