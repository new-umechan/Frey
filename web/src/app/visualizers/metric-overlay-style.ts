import * as THREE from "three";

const SUPPORTED_METRICS = new Set<string>([
    "temperature",
    "precipitation",
    "evapotranspiration",
    "aridity",
    "runoff",
    "river_flux",
]);

const COLOR_TEMP_C0 = new THREE.Color(0x0e2a58);
const COLOR_TEMP_C1 = new THREE.Color(0x3987cc);
const COLOR_TEMP_C2 = new THREE.Color(0xf2e6a8);
const COLOR_TEMP_C3 = new THREE.Color(0xe16d38);
const COLOR_TEMP_C4 = new THREE.Color(0x8b191e);

const COLOR_RAIN_C0 = new THREE.Color(0xb39a6e);
const COLOR_RAIN_C1 = new THREE.Color(0xdbca97);
const COLOR_RAIN_C2 = new THREE.Color(0x81b491);
const COLOR_RAIN_C3 = new THREE.Color(0x388da9);
const COLOR_RAIN_C4 = new THREE.Color(0x0e3f63);

const COLOR_DRY_WET = new THREE.Color(0x3f75c2);
const COLOR_DRY_DRY = new THREE.Color(0xbd5034);

const COLOR_RIVER_LOW = new THREE.Color(0xe8effa);
const COLOR_RIVER_HIGH = new THREE.Color(0x0d4f99);

const COLOR_FALLBACK = new THREE.Color(0xb7c3d6);

export function supportsMetricOverlay(metricKey: string): boolean {
    return SUPPORTED_METRICS.has(metricKey);
}

export function normalizeOverlayMetric(metricKey: string, value: number): number {
    if (!Number.isFinite(value)) {
        return 0;
    }
    const range = metricRange(metricKey);
    if (!range) {
        return 0;
    }
    const [min, max] = range;
    const span = Math.max(max - min, 1e-6);
    return clamp((value - min) / span, 0, 1);
}

export function resolveOverlayMetricColor(metricKey: string, value: number, out: THREE.Color): THREE.Color {
    if (!Number.isFinite(value)) {
        return out.copy(COLOR_FALLBACK);
    }
    switch (metricKey) {
        case "temperature":
            return blendFive(COLOR_TEMP_C0, COLOR_TEMP_C1, COLOR_TEMP_C2, COLOR_TEMP_C3, COLOR_TEMP_C4, normalizeOverlayMetric(metricKey, value), out);
        case "precipitation":
        case "evapotranspiration":
        case "runoff":
            return blendFive(COLOR_RAIN_C0, COLOR_RAIN_C1, COLOR_RAIN_C2, COLOR_RAIN_C3, COLOR_RAIN_C4, normalizeOverlayMetric(metricKey, value), out);
        case "aridity":
            return out.copy(COLOR_DRY_WET).lerp(COLOR_DRY_DRY, normalizeOverlayMetric(metricKey, value));
        case "river_flux":
            return out.copy(COLOR_RIVER_LOW).lerp(COLOR_RIVER_HIGH, normalizeOverlayMetric(metricKey, value));
        default:
            return out.copy(COLOR_FALLBACK);
    }
}

function metricRange(metricKey: string): [number, number] | null {
    switch (metricKey) {
        case "temperature":
            return [-30, 45];
        case "precipitation":
            return [0, 4000];
        case "evapotranspiration":
            return [0, 2500];
        case "aridity":
            return [0, 4];
        case "runoff":
            return [0, 3000];
        case "river_flux":
            return [0, 1];
        default:
            return null;
    }
}

function blendFive(
    c0: THREE.Color,
    c1: THREE.Color,
    c2: THREE.Color,
    c3: THREE.Color,
    c4: THREE.Color,
    t: number,
    out: THREE.Color,
): THREE.Color {
    if (t < 0.25) {
        return out.copy(c0).lerp(c1, t / 0.25);
    }
    if (t < 0.5) {
        return out.copy(c1).lerp(c2, (t - 0.25) / 0.25);
    }
    if (t < 0.75) {
        return out.copy(c2).lerp(c3, (t - 0.5) / 0.25);
    }
    return out.copy(c3).lerp(c4, (t - 0.75) / 0.25);
}

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}
