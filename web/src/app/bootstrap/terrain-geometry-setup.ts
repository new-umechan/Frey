import * as THREE from "three";
import { buildTerrainUvFromPositions } from "../../gfx/materials/river-mask";

export function setupTerrainGeometryAttributes({
    geometry,
    terrainMaterial,
    basePositions,
    currentViewMode,
    currentCellMetric,
}: {
    geometry: THREE.BufferGeometry;
    terrainMaterial: {
        setViewMode: (mode: string) => void;
        setCellMetric: (metric: string) => void;
    };
    basePositions: Float32Array;
    currentViewMode: string;
    currentCellMetric: string;
}) {
    const vertexCount = basePositions.length / 3;
    const terrainUv = buildTerrainUvFromPositions(basePositions);
    geometry.setAttribute("terrainUv", new THREE.BufferAttribute(terrainUv, 2));
    geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainMetric", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainMetricOverlay", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainLakeDepth", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));

    terrainMaterial.setViewMode(currentViewMode);
    terrainMaterial.setCellMetric(currentCellMetric);
}
