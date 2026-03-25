import * as THREE from "three";

export function createTerrainRenderer({
    geometry,
    terrainMaterial,
    basePositions,
    buildRenderPositions,
    buildRiverMaskTexture,
}) {
    const NORMAL_REFRESH_INTERVAL_TICKS = 4;
    const BOUNDING_SPHERE_REFRESH_INTERVAL_TICKS = 8;

    let currentRiverMaskTexture = null;
    let positionBuffer = null;
    let metricBuffer = null;
    let currentMetricKey = "height";
    let lastSurfaceMode = null;
    let lastNormalRefreshTick = -1;

    const CORE_ATTRIBUTE_MAP = {
        height: ["terrainHeight"],
        metric: ["terrainMetric"],
    };

    function resolveMetricArray(currentTerrainData, metricKey) {
        switch (metricKey) {
        case "mantle_heat":
            return currentTerrainData.mantleHeat;
        case "erosion_rate":
            return currentTerrainData.erosionRate;
        case "deposition_rate":
            return currentTerrainData.depositionRate;
        case "temperature":
            return currentTerrainData.temperature;
        case "precipitation":
            return currentTerrainData.precipitation;
        case "evapotranspiration":
            return currentTerrainData.evapotranspiration;
        case "aridity":
            return currentTerrainData.aridity;
        case "ocean_temperature":
            return currentTerrainData.oceanTemperature;
        case "river_flux":
            return currentTerrainData.riverFlux;
        case "runoff":
            return currentTerrainData.runoff;
        case "river_transport_cost":
            return currentTerrainData.riverTransportCost;
        case "height":
        default:
            return currentTerrainData.heightData;
        }
    }

    function updateMetricAttribute(currentTerrainData) {
        const source = resolveMetricArray(currentTerrainData, currentMetricKey);
        if (!metricBuffer || metricBuffer.length !== source.length) {
            metricBuffer = new Float32Array(source.length);
        }
        metricBuffer.set(source);
        const metricAttr = ensureAttribute("terrainMetric", metricBuffer, 1);
        metricAttr.needsUpdate = true;
    }

    function ensureAttribute(name, array, itemSize) {
        const current = geometry.getAttribute(name);
        if (current?.array === array) {
            return current;
        }
        const attribute = new THREE.BufferAttribute(array, itemSize);
        geometry.setAttribute(name, attribute);
        return attribute;
    }

    function ensureTerrainAttributes(currentTerrainData) {
        ensureAttribute("terrainHeight", currentTerrainData.heightData, 1);
        updateMetricAttribute(currentTerrainData);
        ensureAttribute("terrainLakeDepth", currentTerrainData.lakeDepth, 1);
        ensureAttribute("terrainDebugTrench", currentTerrainData.tectonicDebug.trench, 1);
        ensureAttribute("terrainDebugArc", currentTerrainData.tectonicDebug.arc, 1);
        ensureAttribute("terrainDebugBackarc", currentTerrainData.tectonicDebug.backarc, 1);
        ensureAttribute(
            "terrainDebugOceanOceanArc",
            currentTerrainData.tectonicDebug.oceanOceanArc,
            1,
        );
    }

    function markAttributeNeedsUpdate(name) {
        const attribute = geometry.getAttribute(name);
        if (attribute) {
            attribute.needsUpdate = true;
        }
    }

    function markTerrainChanges(changes) {
        for (const [changeKey, attributeNames] of Object.entries(CORE_ATTRIBUTE_MAP)) {
            if (!changes?.[changeKey]) {
                continue;
            }
            for (const attributeName of attributeNames) {
                markAttributeNeedsUpdate(attributeName);
            }
        }
    }

    function updateGeometryPositions(currentTerrainData, currentSurfaceMode, options = {}) {
        if (!currentTerrainData) {
            return;
        }
        const surfaceModeChanged = lastSurfaceMode !== currentSurfaceMode;
        const shouldUpdate = options.force || options.heightChanged || surfaceModeChanged;
        if (!shouldUpdate) {
            return;
        }
        const positions = buildRenderPositions(
            basePositions,
            currentTerrainData.heightData,
            currentSurfaceMode,
        );
        if (!positionBuffer || positionBuffer.length !== positions.length) {
            positionBuffer = new Float32Array(positions.length);
        }
        positionBuffer.set(positions);
        const positionAttribute = ensureAttribute("position", positionBuffer, 3);
        positionAttribute.needsUpdate = true;
        const currentTick = Number.isFinite(options.tick) ? options.tick : -1;
        const shouldRefreshNormals = options.force
            || surfaceModeChanged
            || (options.heightChanged && (currentTick - lastNormalRefreshTick >= NORMAL_REFRESH_INTERVAL_TICKS));
        if (shouldRefreshNormals) {
            geometry.computeVertexNormals();
            lastNormalRefreshTick = currentTick;
        }
        const shouldRefreshBoundingSphere = options.force
            || surfaceModeChanged
            || currentTick < 0
            || currentTick % BOUNDING_SPHERE_REFRESH_INTERVAL_TICKS === 0;
        if (shouldRefreshBoundingSphere) {
            geometry.computeBoundingSphere();
        }
        lastSurfaceMode = currentSurfaceMode;
    }

    function updateRiverMaskTexture(currentTerrainData) {
        if (!currentTerrainData) {
            return;
        }
        const nextTexture = buildRiverMaskTexture(
            basePositions,
            currentTerrainData.riverNext,
            currentTerrainData.riverFlux,
        );
        if (currentRiverMaskTexture) {
            currentRiverMaskTexture.dispose();
        }
        currentRiverMaskTexture = nextTexture;
        terrainMaterial.setRiverMaskTexture(nextTexture);
    }

    function initializeTerrain(currentTerrainData, currentSurfaceMode) {
        ensureTerrainAttributes(currentTerrainData);
        updateGeometryPositions(currentTerrainData, currentSurfaceMode, {
            force: true,
            heightChanged: true,
            tick: 0,
        });
        updateRiverMaskTexture(currentTerrainData);
    }

    function applyCoreChanges(
        currentTerrainData,
        changes,
        currentSurfaceMode,
        tick,
        perfRecorder = null,
    ) {
        if (!currentTerrainData) {
            return;
        }
        ensureTerrainAttributes(currentTerrainData);
        markTerrainChanges(changes);
        if (changes?.river) {
            if (perfRecorder) {
                perfRecorder.measure("river_mask_update", () => {
                    updateRiverMaskTexture(currentTerrainData);
                });
            } else {
                updateRiverMaskTexture(currentTerrainData);
            }
        }
        if (perfRecorder) {
            perfRecorder.measure("geometry_update", () => {
                updateGeometryPositions(currentTerrainData, currentSurfaceMode, {
                    heightChanged: changes?.height,
                    tick,
                });
            });
            return;
        }
        updateGeometryPositions(currentTerrainData, currentSurfaceMode, {
            heightChanged: changes?.height,
            tick,
        });
    }

    function applyTerrainMaterialState(currentViewMode, debugEnabled, currentCellMetric) {
        currentMetricKey = currentCellMetric;
        terrainMaterial.setViewMode(currentViewMode);
        terrainMaterial.setDebugEnabled(debugEnabled);
        terrainMaterial.setCellMetric(currentCellMetric);
        const currentMetricAttribute = geometry.getAttribute("terrainMetric");
        if (currentMetricAttribute) {
            currentMetricAttribute.needsUpdate = true;
        }
    }

    return {
        initializeTerrain,
        applyCoreChanges,
        updateGeometryPositions,
        applyTerrainMaterialState,
    };
}
