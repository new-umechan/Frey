import {
    build_render_positions as wasmBuildRenderPositions,
} from "../../transport/wasm/frey-wasm-module";

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
    const positions = wasmBuildRenderPositions({
        base_positions: basePositions,
        height_data: heightData,
        surface_mode: surfaceMode,
        view_mode: options.viewMode ?? "normal",
        cell_metric: options.cellMetric ?? "height",
        metric_data: options.metricData,
    });
    if (positions instanceof Float32Array) {
        return positions;
    }
    return Float32Array.from(positions ?? []);
}
