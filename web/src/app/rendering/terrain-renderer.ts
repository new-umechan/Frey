import * as THREE from "three";
import { type TickPerfRecorder } from "../perf/recorder";

export interface TerrainRendererOptions {
    geometry: THREE.BufferGeometry;
    terrainMaterial: any;
    basePositions: Float32Array;
    buildRenderPositions: (base: Float32Array, height: any, mode: string) => Float32Array;
    buildRiverMaskTexture: (base: Float32Array, next: any, flux: any) => THREE.Texture;
}

export function createTerrainRenderer(options: TerrainRendererOptions) {
    const {
        geometry,
        terrainMaterial,
        basePositions,
        buildRenderPositions,
        buildRiverMaskTexture,
    } = options;

    const NORMAL_REFRESH_INTERVAL_TICKS = 4;
    const BOUNDING_SPHERE_REFRESH_INTERVAL_TICKS = 8;

    let currentRiverMaskTexture: THREE.Texture | null = null;
    let positionBuffer: Float32Array | null = null;
    let metricBuffer: Float32Array | null = null;
    let currentMetricKey = "height";
    let lastSurfaceMode: string | null = null;
    let lastNormalRefreshTick = -1;

    const CORE_ATTRIBUTE_MAP: Record<string, string[]> = {
        height: ["terrainHeight"],
        metric: ["terrainMetric"],
    };

    function resolveMetricArray(currentTerrainData: any, metricKey: string): any {
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

    function updateMetricAttribute(currentTerrainData: any) {
        const source = resolveMetricArray(currentTerrainData, currentMetricKey);
        if (!metricBuffer || metricBuffer.length !== source.length) {
            metricBuffer = new Float32Array(source.length);
        }
        metricBuffer.set(source);
        const metricAttr = ensureAttribute("terrainMetric", metricBuffer, 1);
        metricAttr.needsUpdate = true;
    }

    function ensureAttribute(name: string, array: any, itemSize: number): THREE.BufferAttribute {
        const current = geometry.getAttribute(name);
        if (current?.array === array) {
            return current as THREE.BufferAttribute;
        }
        const attribute = new THREE.BufferAttribute(array, itemSize);
        geometry.setAttribute(name, attribute);
        return attribute;
    }

    function ensureTerrainAttributes(currentTerrainData: any) {
        ensureAttribute("terrainHeight", currentTerrainData.heightData, 1);
        updateMetricAttribute(currentTerrainData);
        if (currentTerrainData.lakeDepth) {
            ensureAttribute("terrainLakeDepth", currentTerrainData.lakeDepth, 1);
        }
        if (currentTerrainData.tectonicDebug) {
            ensureAttribute("terrainDebugTrench", currentTerrainData.tectonicDebug.trench, 1);
            ensureAttribute("terrainDebugArc", currentTerrainData.tectonicDebug.arc, 1);
            ensureAttribute("terrainDebugBackarc", currentTerrainData.tectonicDebug.backarc, 1);
            ensureAttribute(
                "terrainDebugOceanOceanArc",
                currentTerrainData.tectonicDebug.oceanOceanArc,
                1,
            );
        }
    }

    function markAttributeNeedsUpdate(name: string) {
        const attribute = geometry.getAttribute(name);
        if (attribute) {
            attribute.needsUpdate = true;
        }
    }

    function markTerrainChanges(changes: any) {
        for (const [changeKey, attributeNames] of Object.entries(CORE_ATTRIBUTE_MAP)) {
            if (!changes?.[changeKey]) {
                continue;
            }
            for (const attributeName of attributeNames) {
                markAttributeNeedsUpdate(attributeName);
            }
        }
    }

    function updateGeometryPositions(currentTerrainData: any, currentSurfaceMode: string, options: any = {}) {
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

    function updateRiverMaskTexture(currentTerrainData: any) {
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

    function initializeTerrain(currentTerrainData: any, currentSurfaceMode: string) {
        ensureTerrainAttributes(currentTerrainData);
        updateGeometryPositions(currentTerrainData, currentSurfaceMode, {
            force: true,
            heightChanged: true,
            tick: 0,
        });
        updateRiverMaskTexture(currentTerrainData);
    }

    function applyCoreChanges(
        currentTerrainData: any,
        changes: any,
        currentSurfaceMode: string,
        tick: number,
        perfRecorder: TickPerfRecorder | null = null,
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

    function applyTerrainMaterialState(currentViewMode: string, debugEnabled: boolean, currentCellMetric: string) {
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
