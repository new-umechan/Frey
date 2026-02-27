import * as THREE from "three";

function srgbHexToLinearRgb(hex) {
    const color = new THREE.Color(hex);
    return new THREE.Vector3(color.r, color.g, color.b);
}

function viewModeToNumber(mode) {
    return mode === "plates" ? 1 : 0;
}

export function createTerrainMaterial() {
    const emptyRiverMask = new THREE.DataTexture(new Uint8Array([0, 0, 0, 255]), 1, 1, THREE.RGBAFormat);
    emptyRiverMask.wrapS = THREE.RepeatWrapping;
    emptyRiverMask.wrapT = THREE.ClampToEdgeWrapping;
    emptyRiverMask.magFilter = THREE.LinearFilter;
    emptyRiverMask.minFilter = THREE.LinearFilter;
    emptyRiverMask.needsUpdate = true;

    const uniforms = {
        uViewMode: { value: 0.0 },
        uDebugEnabled: { value: 0.0 },
        uRiverMask: { value: emptyRiverMask },
        uSeaColor: { value: srgbHexToLinearRgb("#12406a") },
        uSeaPlateMixColor: { value: srgbHexToLinearRgb("#0e2847") },
        uLakeColor: { value: srgbHexToLinearRgb("#2f82c7") },
        uRiverColor: { value: srgbHexToLinearRgb("#4ca3dd") },
        uDebugTrenchColor: { value: srgbHexToLinearRgb("#ff355e") },
        uDebugBackarcColor: { value: srgbHexToLinearRgb("#7b61ff") },
        uDebugArcColor: { value: srgbHexToLinearRgb("#ffb000") },
        uDebugOceanOceanArcColor: { value: srgbHexToLinearRgb("#2aff7a") },
    };

    const material = new THREE.MeshStandardMaterial({
        roughness: 0.95,
        metalness: 0.02,
        color: "#ffffff",
    });

    material.onBeforeCompile = (shader) => {
        Object.assign(shader.uniforms, uniforms);

        shader.vertexShader = shader.vertexShader
            .replace(
                "#include <common>",
                `#include <common>
attribute float terrainHeight;
attribute float terrainRiverFlux;
attribute float terrainPlateId;
attribute float terrainLakeDepth;
attribute float terrainDebugTrench;
attribute float terrainDebugArc;
attribute float terrainDebugBackarc;
attribute float terrainDebugOceanOceanArc;
attribute vec2 terrainUv;
varying float vTerrainHeight;
varying float vTerrainRiverFlux;
varying float vTerrainPlateId;
varying float vTerrainLakeDepth;
varying float vTerrainDebugTrench;
varying float vTerrainDebugArc;
varying float vTerrainDebugBackarc;
varying float vTerrainDebugOceanOceanArc;
varying vec2 vTerrainUv;`,
            )
            .replace(
                "#include <begin_vertex>",
                `#include <begin_vertex>
vTerrainHeight = terrainHeight;
vTerrainRiverFlux = terrainRiverFlux;
vTerrainPlateId = terrainPlateId;
vTerrainLakeDepth = terrainLakeDepth;
vTerrainDebugTrench = terrainDebugTrench;
vTerrainDebugArc = terrainDebugArc;
vTerrainDebugBackarc = terrainDebugBackarc;
vTerrainDebugOceanOceanArc = terrainDebugOceanOceanArc;
vTerrainUv = terrainUv;`,
            );

        shader.fragmentShader = shader.fragmentShader
            .replace(
                "#include <common>",
                `#include <common>
uniform float uViewMode;
uniform float uDebugEnabled;
uniform sampler2D uRiverMask;
uniform vec3 uSeaColor;
uniform vec3 uSeaPlateMixColor;
uniform vec3 uLakeColor;
uniform vec3 uRiverColor;
uniform vec3 uDebugTrenchColor;
uniform vec3 uDebugBackarcColor;
uniform vec3 uDebugArcColor;
uniform vec3 uDebugOceanOceanArcColor;
varying float vTerrainHeight;
varying float vTerrainRiverFlux;
varying float vTerrainPlateId;
varying float vTerrainLakeDepth;
varying float vTerrainDebugTrench;
varying float vTerrainDebugArc;
varying float vTerrainDebugBackarc;
varying float vTerrainDebugOceanOceanArc;
varying vec2 vTerrainUv;

float freyClamp(float v, float lo, float hi) {
    return clamp(v, lo, hi);
}

float freyLerp(float a, float b, float t) {
    return a + (b - a) * t;
}

float hueToRgb(float p, float q, float t) {
    if (t < 0.0) t += 1.0;
    if (t > 1.0) t -= 1.0;
    if (t < 1.0 / 6.0) return p + (q - p) * 6.0 * t;
    if (t < 1.0 / 2.0) return q;
    if (t < 2.0 / 3.0) return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    return p;
}

vec3 hslToRgb(float h, float s, float l) {
    float hue = mod(h, 1.0);
    float sat = clamp(s, 0.0, 1.0);
    float light = clamp(l, 0.0, 1.0);
    if (sat <= 1e-5) {
        return vec3(light);
    }
    float q = light < 0.5 ? light * (1.0 + sat) : light + sat - light * sat;
    float p = 2.0 * light - q;
    return vec3(
        hueToRgb(p, q, hue + 1.0 / 3.0),
        hueToRgb(p, q, hue),
        hueToRgb(p, q, hue - 1.0 / 3.0)
    );
}

vec3 freyPlateModeColor(float plateId, float h) {
    float hue = mod(plateId * 137.508, 360.0) / 360.0;
    float saturation = 0.58;
    float lightness = h > 0.0 ? 0.54 : 0.38;
    return hslToRgb(hue, saturation, lightness);
}

vec3 freyNormalModeColor(float h, float lakeDepth, float riverFlux, float riverMask) {
    if (h <= 0.0) {
        return uSeaColor;
    }

    float t = min(h, 1.0);
    vec3 c = vec3(
        freyLerp(0.18, 0.62, t),
        freyLerp(0.42, 0.56, t),
        freyLerp(0.20, 0.48, t)
    );

    float lakeBand = step(0.008, lakeDepth) * (1.0 - step(0.55, h));
    if (lakeBand > 0.5) {
        c = mix(c, uLakeColor, 0.65);
    }

    float riverBand = step(0.5, riverMask) * (1.0 - step(0.65, h));
    if (riverBand > 0.5) {
        c = mix(c, uRiverColor, 0.85);
    }

    return c;
}

vec3 freyApplyDebugOverlay(vec3 color) {
    if (uDebugEnabled < 0.5 || uViewMode > 0.5) {
        return color;
    }
    if (vTerrainDebugTrench > 0.01) {
        color = mix(color, uDebugTrenchColor, min(vTerrainDebugTrench * 0.90, 0.80));
    }
    if (vTerrainDebugBackarc > 0.01) {
        color = mix(color, uDebugBackarcColor, min(vTerrainDebugBackarc * 0.60, 0.55));
    }
    if (vTerrainDebugArc > 0.01) {
        color = mix(color, uDebugArcColor, min(vTerrainDebugArc * 0.95, 0.85));
    }
    if (vTerrainDebugOceanOceanArc > 0.01) {
        color = mix(
            color,
            uDebugOceanOceanArcColor,
            min(vTerrainDebugOceanOceanArc, 0.95)
        );
    }
    return color;
}`,
            )
            .replace(
                "#include <color_fragment>",
                `#include <color_fragment>
float riverMaskTex = texture2D(uRiverMask, vec2(fract(vTerrainUv.x), clamp(vTerrainUv.y, 0.0, 1.0))).r;
vec3 terrainColor;
if (uViewMode > 0.5) {
    terrainColor = freyPlateModeColor(vTerrainPlateId, vTerrainHeight);
    if (vTerrainHeight <= 0.0) {
        terrainColor = mix(terrainColor, uSeaPlateMixColor, 0.25);
    }
    if (riverMaskTex > 0.5 && vTerrainHeight > 0.0 && vTerrainHeight < 0.65) {
        terrainColor = mix(terrainColor, uRiverColor, 0.85);
    }
} else {
    terrainColor = freyNormalModeColor(
        vTerrainHeight,
        vTerrainLakeDepth,
        vTerrainRiverFlux,
        riverMaskTex
    );
}
terrainColor = freyApplyDebugOverlay(terrainColor);
diffuseColor.rgb = terrainColor;`,
            );
    };

    material.customProgramCacheKey = () => "frey-terrain-standard-v2";

    const controller = {
        material,
        setViewMode(mode) {
            uniforms.uViewMode.value = viewModeToNumber(mode);
        },
        setDebugEnabled(enabled) {
            uniforms.uDebugEnabled.value = enabled ? 1.0 : 0.0;
        },
        setRiverMaskTexture(texture) {
            uniforms.uRiverMask.value = texture;
        },
        dispose() {
            emptyRiverMask.dispose();
        },
    };

    return controller;
}
