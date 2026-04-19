import * as THREE from "three";
import {
    normalizeOverlayMetric,
    resolveOverlayMetricColor,
    supportsMetricOverlay,
} from "../../app/visualizers/metric-overlay-style";

const TERRAIN_HEIGHT_SCALE = 0.08;
const OVERLAY_BASE_OFFSET = 0.004;
const OVERLAY_MAX_DISPLACEMENT = 0.06;
const OVERLAY_OPACITY = 0.92;

export interface MetricCellOverlayMesh {
    positions: Float32Array;
    cellIds: Uint32Array;
    lift: Float32Array;
}

export interface MetricCellOverlayLayer {
    mesh: THREE.Mesh;
    supportsMetric: (metricKey: string) => boolean;
    setVisible: (visible: boolean) => void;
    update: (
        heightData: Float32Array,
        metricData: Float32Array,
        metricKey: string,
        dirtyCells?: Uint32Array | number[] | null,
    ) => void;
}

export function createMetricCellOverlayLayer(baseMesh: MetricCellOverlayMesh): MetricCellOverlayLayer {
    const geometry = new THREE.BufferGeometry();
    const positionBuffer = new Float32Array(baseMesh.positions);
    const colorBuffer = new Float32Array((baseMesh.positions.length / 3) * 3);

    geometry.setAttribute("position", new THREE.BufferAttribute(positionBuffer, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(colorBuffer, 3));

    const material = new THREE.MeshBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: OVERLAY_OPACITY,
        depthWrite: false,
        side: THREE.DoubleSide,
        polygonOffset: true,
        polygonOffsetFactor: -1,
        polygonOffsetUnits: -1,
    });

    const mesh = new THREE.Mesh(geometry, material);
    mesh.frustumCulled = false;
    mesh.visible = false;
    const vertexIndicesByCell = buildVertexIndicesByCell(baseMesh.cellIds);

    const color = new THREE.Color();

    const updateVertex = (vertexIndex: number, heightData: Float32Array, metricData: Float32Array, metricKey: string) => {
        const cellId = baseMesh.cellIds[vertexIndex];
        if (cellId >= heightData.length || cellId >= metricData.length) {
            return;
        }

        const src = vertexIndex * 3;
        const x = baseMesh.positions[src];
        const y = baseMesh.positions[src + 1];
        const z = baseMesh.positions[src + 2];

        const terrainHeight = clamp(heightData[cellId], -0.12, 1.2);
        const displacement = normalizeOverlayMetric(metricKey, metricData[cellId]) * OVERLAY_MAX_DISPLACEMENT;
        const lift = baseMesh.lift[vertexIndex];
        const radius = 1.0 + terrainHeight * TERRAIN_HEIGHT_SCALE + OVERLAY_BASE_OFFSET + displacement * lift;

        positionBuffer[src] = x * radius;
        positionBuffer[src + 1] = y * radius;
        positionBuffer[src + 2] = z * radius;

        resolveOverlayMetricColor(metricKey, metricData[cellId], color);
        colorBuffer[src] = color.r;
        colorBuffer[src + 1] = color.g;
        colorBuffer[src + 2] = color.b;
    };

    const update = (
        heightData: Float32Array,
        metricData: Float32Array,
        metricKey: string,
        dirtyCells: Uint32Array | number[] | null = null,
    ) => {
        if (!supportsMetricOverlay(metricKey)) {
            return;
        }
        if (metricData.length < 1 || heightData.length < 1) {
            return;
        }
        if (baseMesh.lift.length !== baseMesh.cellIds.length) {
            return;
        }

        let changed = false;
        if (dirtyCells === null || dirtyCells === undefined) {
            for (let i = 0; i < baseMesh.cellIds.length; i += 1) {
                updateVertex(i, heightData, metricData, metricKey);
            }
            changed = true;
        } else {
            for (let i = 0; i < dirtyCells.length; i += 1) {
                const cellId = Number(dirtyCells[i]);
                if (!Number.isFinite(cellId)) {
                    continue;
                }
                const vertexIndices = vertexIndicesByCell.get(cellId);
                if (!vertexIndices || vertexIndices.length < 1) {
                    continue;
                }
                for (let vertexOffset = 0; vertexOffset < vertexIndices.length; vertexOffset += 1) {
                    updateVertex(vertexIndices[vertexOffset], heightData, metricData, metricKey);
                }
                changed = true;
            }
        }

        if (!changed) {
            return;
        }
        geometry.getAttribute("position").needsUpdate = true;
        geometry.getAttribute("color").needsUpdate = true;
    };

    return {
        mesh,
        supportsMetric: supportsMetricOverlay,
        setVisible: (visible: boolean) => {
            mesh.visible = visible;
        },
        update,
    };
}

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

function buildVertexIndicesByCell(cellIds: Uint32Array): Map<number, number[]> {
    const cellToVertices = new Map<number, number[]>();
    for (let i = 0; i < cellIds.length; i += 1) {
        const cellId = cellIds[i];
        const current = cellToVertices.get(cellId);
        if (current) {
            current.push(i);
            continue;
        }
        cellToVertices.set(cellId, [i]);
    }
    return cellToVertices;
}
