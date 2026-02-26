import * as THREE from "three";

function plateModeColor(plate, heightValue) {
    const hue = ((plate * 137.508) % 360) / 360;
    const saturation = 0.58;
    const lightness = heightValue > 0.0 ? 0.54 : 0.38;
    return new THREE.Color().setHSL(hue, saturation, lightness);
}

export function buildVertexColors(
    heightData,
    plateId,
    riverFlux,
    lakeDepth,
    viewMode,
    debugEnabled = false,
    tectonicDebug = null,
) {
    const colors = new Float32Array(heightData.length * 3);

    for (let v = 0; v < heightData.length; v += 1) {
        const h = heightData[v];
        const river = riverFlux[v];
        const lake = lakeDepth?.[v] ?? 0;
        let color;

        if (viewMode === "plates") {
            color = plateModeColor(plateId[v], h);
            if (h <= 0.0) {
                color.lerp(new THREE.Color("#0e2847"), 0.25);
            }
        } else if (h <= 0.0) {
            color = new THREE.Color("#12406a");
        } else {
            const t = Math.min(1.0, h);
            color = new THREE.Color(
                THREE.MathUtils.lerp(0.18, 0.62, t),
                THREE.MathUtils.lerp(0.42, 0.56, t),
                THREE.MathUtils.lerp(0.20, 0.48, t),
            );
            if (lake > 0.0 && h < 0.55) {
                const lakeDepthFactor = THREE.MathUtils.smoothstep(lake, 0.008, 0.050);
                const lakeWaterFactor = THREE.MathUtils.smoothstep(river, 0.012, 0.080);
                const lakeMix = 0.75 * lakeDepthFactor * lakeWaterFactor;
                if (lakeMix > 0.01) {
                    color.lerp(new THREE.Color("#2f82c7"), lakeMix);
                }
            }
            if (river > 0.10 && h < 0.45) {
                color.lerp(new THREE.Color("#4ca3dd"), Math.min(0.35, river * 0.45));
            }
        }

        if (debugEnabled && viewMode === "normal" && tectonicDebug) {
            const trench = tectonicDebug.trench?.[v] ?? 0;
            const arc = tectonicDebug.arc?.[v] ?? 0;
            const backarc = tectonicDebug.backarc?.[v] ?? 0;
            const oceanOceanArc = tectonicDebug.oceanOceanArc?.[v] ?? 0;

            if (trench > 0.01) {
                color.lerp(new THREE.Color("#ff355e"), Math.min(0.80, trench * 0.90));
            }
            if (backarc > 0.01) {
                color.lerp(new THREE.Color("#7b61ff"), Math.min(0.55, backarc * 0.60));
            }
            if (arc > 0.01) {
                color.lerp(new THREE.Color("#ffb000"), Math.min(0.85, arc * 0.95));
            }
            if (oceanOceanArc > 0.01) {
                color.lerp(new THREE.Color("#2aff7a"), Math.min(0.95, oceanOceanArc));
            }
        }

        const i = v * 3;
        colors[i] = color.r;
        colors[i + 1] = color.g;
        colors[i + 2] = color.b;
    }

    return colors;
}

export function buildRenderPositions(basePositions, heightData, surfaceMode = "globe") {
    const positions = new Float32Array(basePositions);
    const isMapMode = surfaceMode === "map";

    for (let i = 0; i < positions.length; i += 3) {
        const v = i / 3;
        const h = heightData[v];
        const x = positions[i];
        const y = positions[i + 1];
        const z = positions[i + 2];
        const renderHeight = h > 0.0 ? h : 0.0;
        const radius = 1.0 + renderHeight * 0.04;

        if (isMapMode) {
            const invLen = 1 / Math.max(1e-6, Math.hypot(x, y, z));
            const nx = x * invLen;
            const ny = y * invLen;
            const nz = z * invLen;
            const longitude = Math.atan2(nz, nx);
            const latitude = Math.asin(THREE.MathUtils.clamp(ny, -1, 1));

            positions[i] = longitude / Math.PI;
            positions[i + 1] = latitude / Math.PI;
            positions[i + 2] = 0;
            continue;
        }

        positions[i] = x * radius;
        positions[i + 1] = y * radius;
        positions[i + 2] = z * radius;
    }

    return positions;
}

export function summarizeTerrain(heightData, plateId) {
    const plateCount = new Set(plateId).size;
    const landCount = heightData.reduce((acc, h) => acc + (h > 0.0 ? 1 : 0), 0);
    const landRatio = landCount / Math.max(1, heightData.length);

    return {
        plateCount,
        landRatio,
    };
}
