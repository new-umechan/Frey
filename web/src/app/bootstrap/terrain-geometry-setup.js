import * as THREE from "three";
import { buildTerrainUvFromPositions } from "../../gfx/materials/river-mask.js";

export function setupTerrainGeometryAttributes({
    geometry,
    terrainMaterial,
    basePositions,
    currentViewMode,
    currentClimateMetric,
    debugEnabled,
}) {
    const vertexCount = basePositions.length / 3;
    const terrainUv = buildTerrainUvFromPositions(basePositions);
    geometry.setAttribute("terrainUv", new THREE.BufferAttribute(terrainUv, 2));
    geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainRiverFlux", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainMantleHeat", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainTemperature", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute(
        "terrainPrecipitation",
        new THREE.BufferAttribute(new Float32Array(vertexCount), 1),
    );
    geometry.setAttribute("terrainPlateId", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainLakeDepth", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugTrench", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugArc", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugBackarc", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute(
        "terrainDebugOceanOceanArc",
        new THREE.BufferAttribute(new Float32Array(vertexCount), 1),
    );

    terrainMaterial.setViewMode(currentViewMode);
    terrainMaterial.setClimateMetric(currentClimateMetric);
    terrainMaterial.setDebugEnabled(debugEnabled);
}
