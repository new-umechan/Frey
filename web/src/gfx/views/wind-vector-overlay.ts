import * as THREE from "three";

const TERRAIN_HEIGHT_SCALE = 0.08;
const BASE_OFFSET = 0.01;
const TARGET_ARROW_COUNT = 1800;
const MIN_SPEED = 1e-4;

const UP = new THREE.Vector3(0, 1, 0);
const FALLBACK_AXIS = new THREE.Vector3(1, 0, 0);
const normal = new THREE.Vector3();
const east = new THREE.Vector3();
const north = new THREE.Vector3();
const tangent = new THREE.Vector3();
const position = new THREE.Vector3();
const quaternion = new THREE.Quaternion();
const scale = new THREE.Vector3();
const matrix = new THREE.Matrix4();
const color = new THREE.Color();

export interface WindVectorOverlayLayer {
    mesh: THREE.InstancedMesh;
    setVisible: (visible: boolean) => void;
    update: (heightData: Float32Array, windU: Float32Array, windV: Float32Array) => void;
}

function buildSampledCellIds(cellCount: number): Uint32Array {
    if (cellCount < 1) {
        return new Uint32Array(0);
    }
    const stride = Math.max(1, Math.ceil(cellCount / TARGET_ARROW_COUNT));
    const sampled: number[] = [];
    for (let i = 0; i < cellCount; i += stride) {
        sampled.push(i);
    }
    return Uint32Array.from(sampled);
}

function resolveTangentBasis(surfaceNormal: THREE.Vector3) {
    east.crossVectors(UP, surfaceNormal);
    if (east.lengthSq() < 1e-8) {
        east.crossVectors(FALLBACK_AXIS, surfaceNormal);
    }
    east.normalize();
    north.crossVectors(surfaceNormal, east).normalize();
}

export function createWindVectorOverlayLayer(basePositions: Float32Array): WindVectorOverlayLayer {
    const cellCount = Math.floor(basePositions.length / 3);
    const sampledCellIds = buildSampledCellIds(cellCount);

    const geometry = new THREE.ConeGeometry(0.0024, 0.016, 5);
    const material = new THREE.MeshBasicMaterial({
        color: "#5c89b2",
        transparent: true,
        opacity: 0.9,
        depthWrite: false,
        side: THREE.DoubleSide,
        vertexColors: true,
    });
    const mesh = new THREE.InstancedMesh(geometry, material, sampledCellIds.length);
    mesh.count = sampledCellIds.length;
    mesh.frustumCulled = false;
    mesh.visible = false;

    const update = (heightData: Float32Array, windU: Float32Array, windV: Float32Array) => {
        if (sampledCellIds.length < 1) {
            return;
        }
        for (let i = 0; i < sampledCellIds.length; i += 1) {
            const cellId = sampledCellIds[i];
            if (cellId >= heightData.length || cellId >= windU.length || cellId >= windV.length) {
                scale.setScalar(0);
                matrix.compose(position.set(0, 0, 0), quaternion.identity(), scale);
                mesh.setMatrixAt(i, matrix);
                continue;
            }
            const src = cellId * 3;
            normal.set(
                basePositions[src],
                basePositions[src + 1],
                basePositions[src + 2],
            ).normalize();
            resolveTangentBasis(normal);

            const u = windU[cellId];
            const v = windV[cellId];
            const speed = Math.hypot(u, v);
            if (!Number.isFinite(speed) || speed < MIN_SPEED) {
                scale.setScalar(0);
                matrix.compose(position.set(0, 0, 0), quaternion.identity(), scale);
                mesh.setMatrixAt(i, matrix);
                continue;
            }
            tangent.copy(east).multiplyScalar(u).addScaledVector(north, v);
            if (tangent.lengthSq() < 1e-8) {
                tangent.copy(east);
            } else {
                tangent.normalize();
            }

            const terrainHeight = Math.max(-0.12, Math.min(1.2, heightData[cellId]));
            const radius = 1.0 + terrainHeight * TERRAIN_HEIGHT_SCALE + BASE_OFFSET;
            position.copy(normal).multiplyScalar(radius);
            quaternion.setFromUnitVectors(UP, tangent);

            const normSpeed = Math.max(0, Math.min(1, speed / 12.0));
            const shaftLength = 0.008 + normSpeed * 0.014;
            const shaftWidth = 0.002 + normSpeed * 0.0014;
            scale.set(shaftWidth, shaftLength, shaftWidth);
            matrix.compose(position, quaternion, scale);
            mesh.setMatrixAt(i, matrix);

            color.setHSL(0.58 - normSpeed * 0.17, 0.55, 0.47 + normSpeed * 0.18);
            mesh.setColorAt(i, color);
        }
        mesh.instanceMatrix.needsUpdate = true;
        if (mesh.instanceColor) {
            mesh.instanceColor.needsUpdate = true;
        }
    };

    return {
        mesh,
        setVisible: (visible: boolean) => {
            mesh.visible = visible;
        },
        update,
    };
}
