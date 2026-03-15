import * as THREE from "three";

export function createTerrainRenderer({
    geometry,
    terrainMaterial,
    basePositions,
    buildRenderPositions,
    buildRiverMaskTexture,
}) {
    let currentRiverMaskTexture = null;

    function updateGeometryPositions(currentTerrainData, currentSurfaceMode) {
        if (!currentTerrainData) {
            return;
        }
        const positions = buildRenderPositions(
            basePositions,
            currentTerrainData.heightData,
            currentSurfaceMode,
        );
        geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
        geometry.computeVertexNormals();
        geometry.computeBoundingSphere();
    }

    function updateTerrainAttributes(currentTerrainData) {
        if (!currentTerrainData) {
            return;
        }
        geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(currentTerrainData.heightData, 1));
        geometry.setAttribute(
            "terrainRiverFlux",
            new THREE.BufferAttribute(currentTerrainData.riverFlux, 1),
        );
        geometry.setAttribute(
            "terrainMantleHeat",
            new THREE.BufferAttribute(currentTerrainData.mantleHeat, 1),
        );
        geometry.setAttribute(
            "terrainPlateId",
            new THREE.BufferAttribute(Float32Array.from(currentTerrainData.plateId), 1),
        );
        geometry.setAttribute("terrainLakeDepth", new THREE.BufferAttribute(currentTerrainData.lakeDepth, 1));
        geometry.setAttribute(
            "terrainDebugTrench",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.trench, 1),
        );
        geometry.setAttribute(
            "terrainDebugArc",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.arc, 1),
        );
        geometry.setAttribute(
            "terrainDebugBackarc",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.backarc, 1),
        );
        geometry.setAttribute(
            "terrainDebugOceanOceanArc",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.oceanOceanArc, 1),
        );
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

    function applyTerrainMaterialState(currentViewMode, debugEnabled) {
        terrainMaterial.setViewMode(currentViewMode);
        terrainMaterial.setDebugEnabled(debugEnabled);
    }

    return {
        updateGeometryPositions,
        updateTerrainAttributes,
        updateRiverMaskTexture,
        applyTerrainMaterialState,
    };
}
