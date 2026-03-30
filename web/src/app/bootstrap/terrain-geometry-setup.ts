import * as THREE from "three";
import { buildTerrainUvFromPositions } from "../../gfx/materials/river-mask";

export function setupTerrainGeometryAttributes({
    geometry,
    terrainMaterial,
    basePositions,
    currentViewMode,
    currentCellMetric,
    debugEnabled,
}: {
    geometry: THREE.BufferGeometry;
    terrainMaterial: {
        setViewMode: (mode: string) => void;
        setCellMetric: (metric: string) => void;
        setDebugEnabled: (enabled: boolean) => void;
    };
    basePositions: Float32Array;
    currentViewMode: string;
    currentCellMetric: string;
    debugEnabled: boolean;
}) {
    const vertexCount = basePositions.length / 3;
    const terrainUv = buildTerrainUvFromPositions(basePositions);
    geometry.setAttribute("terrainUv", new THREE.BufferAttribute(terrainUv, 2));
    geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainMetric", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainLakeDepth", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugTrench", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugArc", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugBackarc", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute(
        "terrainDebugOceanOceanArc",
        new THREE.BufferAttribute(new Float32Array(vertexCount), 1),
    );

    terrainMaterial.setViewMode(currentViewMode);
    terrainMaterial.setCellMetric(currentCellMetric);
    terrainMaterial.setDebugEnabled(debugEnabled);
}
