import * as THREE from "three";

function srgbHexToLinearRgb(hex: string): THREE.Vector3 {
    const color = new THREE.Color(hex);
    return new THREE.Vector3(color.r, color.g, color.b);
}

function viewModeToNumber(mode: string): number {
    return mode === "normal" ? 0 : 1;
}

function metricKeyToNumber(metricKey: string): number {
    if (metricKey.startsWith("crop_adoption_") || metricKey.startsWith("livestock_adoption_")) {
        return 14;
    }
    if (metricKey.startsWith("crop_available_") || metricKey.startsWith("livestock_available_")) {
        return 15;
    }
    const kindByKey: Record<string, number> = {
        height: 0,
        mantle_heat: 1,
        erosion_rate: 2,
        deposition_rate: 3,
        temperature: 4,
        precipitation: 5,
        evapotranspiration: 6,
        aridity: 7,
        ocean_temperature: 8,
        river_flux: 9,
        runoff: 10,
        river_transport_cost: 11,
        ice_pressure: 12,
        plate_id: 13,
        wind_direction: 16,
        biome: 17,
    };
    return kindByKey[metricKey] ?? 0;
}

export interface TerrainMaterialController {
    material: THREE.MeshStandardMaterial;
    setViewMode(mode: string): void;
    setCellMetric(metricKey: string): void;
    setSeaLevelOffset(offset: number): void;
    setRiverMaskTexture(texture: THREE.Texture): void;
    dispose(): void;
}

export function createTerrainMaterial(): TerrainMaterialController {
    const emptyRiverMask = new THREE.DataTexture(new Uint8Array([0, 0, 0, 255]), 1, 1, THREE.RGBAFormat);
    emptyRiverMask.wrapS = THREE.RepeatWrapping;
    emptyRiverMask.wrapT = THREE.ClampToEdgeWrapping;
    emptyRiverMask.magFilter = THREE.LinearFilter;
    emptyRiverMask.minFilter = THREE.LinearFilter;
    emptyRiverMask.needsUpdate = true;

    const uniforms = {
        uViewMode: { value: 0.0 },
        uMetricKind: { value: 0.0 },
        uSeaLevelOffset: { value: 0.0 },
        uRiverMask: { value: emptyRiverMask as THREE.Texture },
        uSeaColor: { value: srgbHexToLinearRgb("#12406a") },
        uLakeColor: { value: srgbHexToLinearRgb("#2f82c7") },
        uRiverColor: { value: srgbHexToLinearRgb("#4ca3dd") },
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
attribute float terrainMetric;
attribute float terrainMetricOverlay;
attribute float terrainLakeDepth;
attribute vec2 terrainUv;
varying float vTerrainHeight;
varying float vTerrainMetric;
varying float vTerrainMetricOverlay;
varying float vTerrainLakeDepth;
varying vec2 vTerrainUv;`,
            )
            .replace(
                "#include <begin_vertex>",
                `#include <begin_vertex>
vTerrainHeight = terrainHeight;
vTerrainMetric = terrainMetric;
vTerrainMetricOverlay = terrainMetricOverlay;
vTerrainLakeDepth = terrainLakeDepth;
vTerrainUv = terrainUv;`,
            );

        shader.fragmentShader = shader.fragmentShader
            .replace(
                "#include <common>",
                `#include <common>
uniform float uViewMode;
uniform float uMetricKind;
uniform float uSeaLevelOffset;
uniform sampler2D uRiverMask;
uniform vec3 uSeaColor;
uniform vec3 uLakeColor;
uniform vec3 uRiverColor;
varying float vTerrainHeight;
varying float vTerrainMetric;
varying float vTerrainMetricOverlay;
varying float vTerrainLakeDepth;
varying vec2 vTerrainUv;

float freyLerp(float a, float b, float t) {
    return a + (b - a) * t;
}

vec3 paletteTemperature(float value) {
    float t = clamp((value + 25.0) / 60.0, 0.0, 1.0);
    vec3 c0 = vec3(0.056, 0.165, 0.345);
    vec3 c1 = vec3(0.223, 0.530, 0.800);
    vec3 c2 = vec3(0.949, 0.901, 0.659);
    vec3 c3 = vec3(0.882, 0.428, 0.220);
    vec3 c4 = vec3(0.545, 0.098, 0.117);
    if (t < 0.25) return mix(c0, c1, t / 0.25);
    if (t < 0.5) return mix(c1, c2, (t - 0.25) / 0.25);
    if (t < 0.75) return mix(c2, c3, (t - 0.5) / 0.25);
    return mix(c3, c4, (t - 0.75) / 0.25);
}

vec3 paletteRain(float value) {
    float t = clamp(value / 3000.0, 0.0, 1.0);
    vec3 c0 = vec3(0.701, 0.606, 0.432);
    vec3 c1 = vec3(0.859, 0.790, 0.593);
    vec3 c2 = vec3(0.507, 0.706, 0.570);
    vec3 c3 = vec3(0.219, 0.553, 0.665);
    vec3 c4 = vec3(0.054, 0.247, 0.388);
    if (t < 0.25) return mix(c0, c1, t / 0.25);
    if (t < 0.5) return mix(c1, c2, (t - 0.25) / 0.25);
    if (t < 0.75) return mix(c2, c3, (t - 0.5) / 0.25);
    return mix(c3, c4, (t - 0.75) / 0.25);
}

vec3 paletteMagma(float value) {
    float t = clamp(value, 0.0, 1.0);
    vec3 c0 = vec3(0.004, 0.004, 0.016);
    vec3 c1 = vec3(0.224, 0.047, 0.329);
    vec3 c2 = vec3(0.573, 0.149, 0.404);
    vec3 c3 = vec3(0.867, 0.326, 0.251);
    vec3 c4 = vec3(0.988, 0.998, 0.644);
    if (t < 0.25) return mix(c0, c1, t / 0.25);
    if (t < 0.5) return mix(c1, c2, (t - 0.25) / 0.25);
    if (t < 0.75) return mix(c2, c3, (t - 0.5) / 0.25);
    return mix(c3, c4, (t - 0.75) / 0.25);
}

vec3 paletteRiver(float value) {
    float t = clamp(value, 0.0, 1.0);
    return mix(vec3(0.91, 0.94, 0.98), vec3(0.05, 0.31, 0.60), t);
}

vec3 paletteDryness(float value) {
    float t = clamp(value / 4.0, 0.0, 1.0);
    return mix(vec3(0.25, 0.46, 0.76), vec3(0.74, 0.31, 0.20), t);
}

vec3 paletteCost(float value) {
    float t = clamp(value, 0.0, 1.0);
    return mix(vec3(0.86, 0.90, 0.91), vec3(0.13, 0.18, 0.22), t);
}

vec3 paletteIcePressure(float value) {
    float t = clamp(value, 0.0, 1.0);
    vec3 c0 = vec3(0.09, 0.23, 0.55);
    vec3 c1 = vec3(0.24, 0.58, 0.92);
    vec3 c2 = vec3(0.88, 0.93, 0.97);
    vec3 c3 = vec3(0.96, 0.69, 0.24);
    vec3 c4 = vec3(0.73, 0.18, 0.05);
    if (t < 0.25) return mix(c0, c1, t / 0.25);
    if (t < 0.5) return mix(c1, c2, (t - 0.25) / 0.25);
    if (t < 0.75) return mix(c2, c3, (t - 0.5) / 0.25);
    return mix(c3, c4, (t - 0.75) / 0.25);
}

vec3 palettePlateId(float value) {
    float id = floor(max(value, 0.0) + 0.5);
    float r = fract(sin(id * 12.9898 + 0.13) * 43758.5453);
    float g = fract(sin(id * 78.233 + 0.57) * 24634.6345);
    float b = fract(sin(id * 37.719 + 0.91) * 14375.9854);
    vec3 randomColor = vec3(r, g, b);
    vec3 anchorColor = mix(
        vec3(0.22, 0.38, 0.74),
        vec3(0.78, 0.46, 0.18),
        fract(id * 0.17)
    );
    return mix(randomColor, anchorColor, 0.42);
}

vec3 paletteAmber(float value) {
    float t = clamp(value / 0.02, 0.0, 1.0);
    return mix(vec3(0.98, 0.93, 0.82), vec3(0.86, 0.42, 0.07), t);
}

vec3 paletteTeal(float value) {
    float t = clamp(value / 0.02, 0.0, 1.0);
    return mix(vec3(0.84, 0.96, 0.94), vec3(0.06, 0.48, 0.45), t);
}

vec3 paletteAdoption(float value) {
    float t = clamp(value, 0.0, 1.0);
    vec3 c0 = vec3(0.96, 0.88, 0.86);
    vec3 c1 = vec3(0.90, 0.61, 0.53);
    vec3 c2 = vec3(0.78, 0.34, 0.25);
    vec3 c3 = vec3(0.62, 0.17, 0.12);
    if (t < 0.33) return mix(c0, c1, t / 0.33);
    if (t < 0.66) return mix(c1, c2, (t - 0.33) / 0.33);
    return mix(c2, c3, (t - 0.66) / 0.34);
}

vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

vec3 paletteWindDirection(float u, float v) {
    float angle = atan(v, u);
    float hue = (angle + 3.141592653589793) / 6.283185307179586;
    float speed = clamp(length(vec2(u, v)) / 12.0, 0.0, 1.0);
    return hsv2rgb(vec3(hue, 0.65, 0.55 + 0.35 * speed));
}

vec3 paletteBiome(float value) {
    float biome = floor(max(value, 0.0) + 0.5);
    if (biome < 0.5) return vec3(0.13, 0.48, 0.27); // TropicalForest
    if (biome < 1.5) return vec3(0.65, 0.65, 0.16); // Savanna
    if (biome < 2.5) return vec3(0.83, 0.60, 0.32); // Desert
    if (biome < 3.5) return vec3(0.47, 0.66, 0.31); // Grassland
    if (biome < 4.5) return vec3(0.25, 0.58, 0.33); // TemperateForest
    if (biome < 5.5) return vec3(0.16, 0.36, 0.32); // BorealForest
    if (biome < 6.5) return vec3(0.69, 0.76, 0.80); // Tundra
    if (biome < 7.5) return vec3(0.22, 0.49, 0.62); // Wetland
    return vec3(0.56, 0.58, 0.61); // Alpine
}

vec3 freyMetricModeColor(float kind) {
    if (kind < 0.5) return paletteRiver(clamp((vTerrainMetric + 1.0) * 0.5, 0.0, 1.0));
    if (kind < 1.5) return paletteMagma(vTerrainMetric);
    if (kind < 2.5) return paletteAmber(vTerrainMetric);
    if (kind < 3.5) return paletteTeal(vTerrainMetric);
    if (kind < 4.5) return paletteTemperature(vTerrainMetric);
    if (kind < 5.5) return paletteRain(vTerrainMetric);
    if (kind < 6.5) return paletteRain(vTerrainMetric);
    if (kind < 7.5) return paletteDryness(vTerrainMetric);
    if (kind < 8.5) return paletteTemperature(vTerrainMetric);
    if (kind < 9.5) return paletteRiver(vTerrainMetric);
    if (kind < 10.5) return paletteRain(vTerrainMetric);
    if (kind < 11.5) return paletteCost(vTerrainMetric);
    if (kind < 12.5) return paletteIcePressure(vTerrainMetric);
    if (kind < 13.5) return palettePlateId(vTerrainMetric);
    if (kind < 14.5) return paletteAdoption(vTerrainMetric);
    if (kind < 16.5) return mix(vec3(0.88, 0.93, 0.98), vec3(0.16, 0.44, 0.78), clamp(vTerrainMetric, 0.0, 1.0));
    if (kind < 17.5) return paletteWindDirection(vTerrainMetric, vTerrainMetricOverlay);
    return paletteBiome(vTerrainMetric);
}

vec3 freyNormalModeColor(float h, float lakeDepth, float riverMask) {
    if (h <= uSeaLevelOffset) {
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
}`,
            )
            .replace(
                "#include <color_fragment>",
                `#include <color_fragment>
float riverMaskTex = texture2D(uRiverMask, vec2(fract(vTerrainUv.x), clamp(vTerrainUv.y, 0.0, 1.0))).r;
vec3 terrainColor = uViewMode > 0.5
    ? freyMetricModeColor(uMetricKind)
    : freyNormalModeColor(vTerrainHeight, vTerrainLakeDepth, riverMaskTex);
if (uViewMode > 0.5 && uMetricKind >= 13.5 && uMetricKind < 14.5 && vTerrainMetricOverlay >= 0.5) {
    vec3 overlayColor = vec3(0.15, 0.42, 0.82);
    float hatch = step(0.72, fract((vTerrainUv.x + vTerrainUv.y) * 64.0));
    float overlayAlpha = mix(0.22, 0.46, hatch);
    terrainColor = mix(terrainColor, overlayColor, overlayAlpha);
}
diffuseColor.rgb = terrainColor;`,
            );
    };

    material.customProgramCacheKey = () => "frey-terrain-standard-v9";

    const controller: TerrainMaterialController = {
        material,
        setViewMode(mode) {
            uniforms.uViewMode.value = viewModeToNumber(mode);
        },
        setCellMetric(metricKey) {
            uniforms.uMetricKind.value = metricKeyToNumber(metricKey);
        },
        setSeaLevelOffset(offset) {
            uniforms.uSeaLevelOffset.value = Number.isFinite(offset) ? offset : 0.0;
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
