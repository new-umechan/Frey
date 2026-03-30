import {
    build_render_positions as wasmBuildRenderPositions,
} from "../../interface/wasm";

export function buildRenderPositions(basePositions: Float32Array, heightData: Float32Array, surfaceMode = "globe") {
    const positions = wasmBuildRenderPositions({
        base_positions: basePositions,
        height_data: heightData,
        surface_mode: surfaceMode,
    });
    if (positions instanceof Float32Array) {
        return positions;
    }
    return Float32Array.from(positions ?? []);
}
