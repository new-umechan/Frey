function buildOceanCellMask(heightData, plateId, plateInfo) {
    const mask = new Uint8Array(heightData.length);
    for (let i = 0; i < heightData.length; i += 1) {
        const pid = plateId[i];
        const oceanByPlate =
            Number.isInteger(pid) &&
            pid >= 0 &&
            pid < plateInfo.isOcean.length &&
            plateInfo.isOcean[pid] > 0;
        if (oceanByPlate || heightData[i] <= 0) {
            mask[i] = 1;
        }
    }
    return mask;
}

export function buildTerrainBundle({
    terrain,
    seed,
    terrainParams,
    initErosionAutomaton,
}) {
    const erosionAutomatonState = initErosionAutomaton(seed, terrainParams);
    const heightData = new Float32Array(terrain.height);
    const plateId = new Uint32Array(terrain.plate_id);
    const riverFlux = new Float32Array(terrain.river_flux);
    const riverNext = new Int32Array(terrain.river_next);
    const lakeDepth = new Float32Array(terrain.lake_depth ?? heightData.length);
    const plateInfo = {
        isOcean: new Uint8Array(terrain.plate_is_ocean),
        baseHeight: new Float32Array(terrain.plate_base_height),
        baseWeight: new Float32Array(terrain.plate_base_weight),
    };
    const vertexWeight = new Float32Array(terrain.vertex_weight);
    const tectonicDebug = {
        trench: new Float32Array(terrain.debug_trench_strength ?? heightData.length),
        arc: new Float32Array(terrain.debug_arc_strength ?? heightData.length),
        backarc: new Float32Array(terrain.debug_backarc_strength ?? heightData.length),
        oceanOceanArc: new Float32Array(
            terrain.debug_ocean_ocean_arc_strength ?? heightData.length,
        ),
    };
    const vertexAgeNorm = new Float32Array(terrain.vertex_age_norm ?? heightData.length);
    const vertexBuoyancy = new Float32Array(terrain.vertex_buoyancy ?? heightData.length);

    const core = {
        heightData,
        plateId,
        riverFlux,
        riverNext,
        lakeDepth,
        plateInfo,
        vertexWeight,
        tectonicDebug,
        targetLandRatio: Number.isFinite(terrain.land_ratio) ? terrain.land_ratio : 0.0,
    };

    const terrainDynamics = {
        oceanAgeNorm: vertexAgeNorm.length === heightData.length
            ? vertexAgeNorm
            : new Float32Array(heightData.length),
        targetBuoyancy: vertexBuoyancy.length === heightData.length
            ? vertexBuoyancy
            : new Float32Array(heightData.length),
        upliftMemory: new Float32Array(heightData.length),
        isOceanCell: buildOceanCellMask(heightData, plateId, plateInfo),
    };

    return {
        core,
        erosionAutomatonState,
        terrainDynamics,
        plateCount: Number.isFinite(terrain.plate_count) ? terrain.plate_count : 0,
        landRatio: Number.isFinite(terrain.land_ratio) ? terrain.land_ratio : 0,
    };
}
