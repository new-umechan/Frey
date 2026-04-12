import * as THREE from "three";
import { type TickPerfRecorder } from "../perf/recorder";
import { type CoreBuffers, type TypedArray } from "../sim/sync/types";
import { type WorldChangeset } from "../sim/sync/constants";
import { getCellMetricMeta } from "./cell-metric";

interface TerrainMaterialController {
    setRiverMaskTexture: (texture: THREE.Texture) => void;
    setViewMode: (mode: string) => void;
    setDebugEnabled: (enabled: boolean) => void;
    setCellMetric: (metric: string) => void;
}

export interface TerrainRendererOptions {
    geometry: THREE.BufferGeometry;
    terrainMaterial: TerrainMaterialController;
    basePositions: Float32Array;
    buildRenderPositions: (base: Float32Array, height: Float32Array, mode: string) => Float32Array;
    buildRiverMaskTexture: (base: Float32Array, next: Int32Array, flux: Float32Array) => THREE.Texture;
}

export interface TerrainRenderer {
    initializeTerrain: (currentTerrainData: CoreBuffers, currentSurfaceMode: string) => void;
    applyCoreChanges: (
        currentTerrainData: CoreBuffers,
        changes: WorldChangeset,
        currentSurfaceMode: string,
        tick: number,
        perfRecorder?: TickPerfRecorder | null,
    ) => void;
    updateGeometryPositions: (
        currentTerrainData: CoreBuffers,
        currentSurfaceMode: string,
        options?: { force?: boolean; heightChanged?: boolean; tick?: number },
    ) => void;
    applyTerrainMaterialState: (
        currentViewMode: string,
        debugEnabled: boolean,
        currentCellMetric: string,
    ) => void;
}

export function createTerrainRenderer(options: TerrainRendererOptions): TerrainRenderer {
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
    let metricOverlayBuffer: Float32Array | null = null;
    let currentMetricKey = "height";
    let lastSurfaceMode: string | null = null;
    let lastNormalRefreshTick = -1;

    const CORE_ATTRIBUTE_MAP: Partial<Record<keyof WorldChangeset, string[]>> = {
        height: ["terrainHeight", "terrainLakeDepth"],
        metric: ["terrainMetric", "terrainMetricOverlay"],
    };

    function resolveMetricArrayByDataKey(currentTerrainData: CoreBuffers, dataKey: string): TypedArray {
        const source = currentTerrainData[dataKey];
        if (source instanceof Float32Array || source instanceof Int32Array || source instanceof Uint32Array) {
            return source;
        }
        console.warn(`resolveMetricArrayByDataKey: dataKey "${dataKey}" not found in CoreBuffers, falling back to heightData`);
        return currentTerrainData.heightData as Float32Array;
    }

    function updateMetricAttribute(currentTerrainData: CoreBuffers) {
        const metricMeta = getCellMetricMeta(currentMetricKey);
        const source = resolveMetricArrayByDataKey(currentTerrainData, metricMeta.dataKey);
        if (!metricBuffer || metricBuffer.length !== source.length) {
            metricBuffer = new Float32Array(source.length);
        }
        metricBuffer.set(source as ArrayLike<number>);
        const metricAttr = ensureAttribute("terrainMetric", metricBuffer, 1);
        metricAttr.needsUpdate = true;

        const overlaySource = metricMeta.overlayDataKey
            ? resolveMetricArrayByDataKey(currentTerrainData, metricMeta.overlayDataKey)
            : null;
        if (overlaySource) {
            if (!metricOverlayBuffer || metricOverlayBuffer.length !== source.length) {
                metricOverlayBuffer = new Float32Array(source.length);
            }
            metricOverlayBuffer.set(overlaySource as ArrayLike<number>);
            const overlayAttr = ensureAttribute("terrainMetricOverlay", metricOverlayBuffer, 1);
            overlayAttr.needsUpdate = true;
        } else if (metricOverlayBuffer) {
            metricOverlayBuffer.fill(0.0);
            const overlayAttr = geometry.getAttribute("terrainMetricOverlay");
            if (overlayAttr) {
                overlayAttr.needsUpdate = true;
            }
        }
    }

    function ensureAttribute(name: string, array: ArrayLike<number>, itemSize: number): THREE.BufferAttribute {
        const current = geometry.getAttribute(name);
        if (current?.array === array) {
            return current as THREE.BufferAttribute;
        }
        const attribute = new THREE.BufferAttribute(array, itemSize);
        geometry.setAttribute(name, attribute);
        return attribute;
    }

    function ensureTerrainAttributes(currentTerrainData: CoreBuffers) {
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

    function markTerrainChanges(changes: WorldChangeset) {
        for (const [changeKey, attributeNames] of Object.entries(CORE_ATTRIBUTE_MAP) as [keyof WorldChangeset, string[]][]) {
            if (!changes[changeKey]) {
                continue;
            }
            for (const attributeName of attributeNames) {
                markAttributeNeedsUpdate(attributeName);
            }
        }
    }

    function updateGeometryPositions(currentTerrainData: CoreBuffers, currentSurfaceMode: string, options: { force?: boolean; heightChanged?: boolean; tick?: number } = {}) {
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
            currentTerrainData.heightData as Float32Array,
            currentSurfaceMode,
        );
        if (!positionBuffer || positionBuffer.length !== positions.length) {
            positionBuffer = new Float32Array(positions.length);
        }
        positionBuffer.set(positions);
        const positionAttribute = ensureAttribute("position", positionBuffer, 3);
        positionAttribute.needsUpdate = true;
        const currentTick = options.tick !== undefined && Number.isFinite(options.tick) ? options.tick : -1;
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

    function updateRiverMaskTexture(currentTerrainData: CoreBuffers) {
        if (!currentTerrainData) {
            return;
        }
        const nextTexture = buildRiverMaskTexture(
            basePositions,
            currentTerrainData.riverNext as Int32Array,
            currentTerrainData.riverFlux as Float32Array,
        );
        if (currentRiverMaskTexture) {
            currentRiverMaskTexture.dispose();
        }
        currentRiverMaskTexture = nextTexture;
        terrainMaterial.setRiverMaskTexture(nextTexture);
    }

    function initializeTerrain(currentTerrainData: CoreBuffers, currentSurfaceMode: string) {
        ensureTerrainAttributes(currentTerrainData);
        updateGeometryPositions(currentTerrainData, currentSurfaceMode, {
            force: true,
            heightChanged: true,
            tick: 0,
        });
        updateRiverMaskTexture(currentTerrainData);
    }

    function applyCoreChanges(
        currentTerrainData: CoreBuffers,
        changes: WorldChangeset,
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
