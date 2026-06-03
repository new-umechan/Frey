const METRIC_DISPLACEMENT_SCALE = 0.06;

function clampScalar(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

function metricDisplacementRange(metricKey: string): [number, number] | null {
    switch (metricKey) {
        case "temperature":
            return [-30.0, 45.0];
        case "precipitation":
            return [0.0, 4000.0];
        case "evapotranspiration":
            return [0.0, 2500.0];
        case "aridity":
            return [0.0, 4.0];
        case "runoff":
            return [0.0, 3000.0];
        case "river_flux":
            return [0.0, 1.0];
        default:
            return null;
    }
}

function buildMetricDisplacement(options: {
    vertexCount: number;
    viewMode?: string;
    cellMetric?: string;
    metricData?: Float32Array;
}): Float32Array | null {
    if (options.viewMode !== "metric") {
        return null;
    }
    const range = metricDisplacementRange(options.cellMetric ?? "height");
    if (!range || options.metricData?.length !== options.vertexCount) {
        return null;
    }
    const [min, max] = range;
    const span = Math.max(1e-6, max - min);
    const displacement = new Float32Array(options.vertexCount);
    for (let index = 0; index < options.vertexCount; index += 1) {
        const normalized = clampScalar((Number(options.metricData[index] ?? 0) - min) / span, 0.0, 1.0);
        displacement[index] = normalized * METRIC_DISPLACEMENT_SCALE;
    }
    return displacement;
}

export function buildRenderPositions(
    basePositions: Float32Array,
    heightData: Float32Array,
    surfaceMode = "globe",
    options: {
        viewMode?: string;
        cellMetric?: string;
        metricData?: Float32Array;
    } = {},
) {
    if (basePositions.length % 3 !== 0) {
        throw new Error("basePositions length must be divisible by 3");
    }
    const vertexCount = basePositions.length / 3;
    if (heightData.length !== vertexCount) {
        throw new Error("basePositions and heightData length mismatch");
    }

    const positions = new Float32Array(basePositions);
    const isMapMode = surfaceMode === "map";
    const metricDisplacement = buildMetricDisplacement({
        vertexCount,
        viewMode: options.viewMode,
        cellMetric: options.cellMetric,
        metricData: options.metricData,
    });

    for (let offset = 0; offset < positions.length; offset += 3) {
        const vertexIndex = offset / 3;
        const height = Number(heightData[vertexIndex] ?? 0);
        const x = positions[offset];
        const y = positions[offset + 1];
        const z = positions[offset + 2];
        const renderHeight = clampScalar(height, -0.12, 1.2);
        const radius = 1.0 + renderHeight * 0.08;

        if (isMapMode) {
            const len = Math.max(1e-6, Math.sqrt(x * x + y * y + z * z));
            const nx = x / len;
            const ny = y / len;
            const nz = z / len;
            const longitude = Math.atan2(nz, nx);
            const latitude = Math.asin(clampScalar(ny, -1.0, 1.0));
            positions[offset] = longitude / Math.PI;
            positions[offset + 1] = latitude / Math.PI;
            positions[offset + 2] = 0.0;
            continue;
        }

        const displacedRadius = radius + Number(metricDisplacement?.[vertexIndex] ?? 0);
        positions[offset] = x * displacedRadius;
        positions[offset + 1] = y * displacedRadius;
        positions[offset + 2] = z * displacedRadius;
    }

    return positions;
}
