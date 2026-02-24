import * as THREE from "three";

function plateModeColor(plate, heightValue) {
    const hue = ((plate * 137.508) % 360) / 360;
    const saturation = 0.58;
    const lightness = heightValue > 0.0 ? 0.54 : 0.38;
    return new THREE.Color().setHSL(hue, saturation, lightness);
}

export function buildVertexColors(heightData, plateId, riverFlux, viewMode) {
    const colors = new Float32Array(heightData.length * 3);

    for (let v = 0; v < heightData.length; v += 1) {
        const h = heightData[v];
        const river = riverFlux[v];
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
            if (river > 0.10 && h < 0.45) {
                color.lerp(new THREE.Color("#4ca3dd"), Math.min(0.35, river * 0.45));
            }
        }

        const i = v * 3;
        colors[i] = color.r;
        colors[i + 1] = color.g;
        colors[i + 2] = color.b;
    }

    return colors;
}

export function buildRenderPositions(basePositions, heightData) {
    const positions = new Float32Array(basePositions);

    for (let i = 0; i < positions.length; i += 3) {
        const v = i / 3;
        const h = heightData[v];
        const x = positions[i];
        const y = positions[i + 1];
        const z = positions[i + 2];
        const renderHeight = h > 0.0 ? h : 0.0;
        const radius = 1.0 + renderHeight * 0.04;

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

