import {
    build_render_positions as wasmBuildRenderPositions,
} from "../../interface/wasm";

export function buildRenderPositions(basePositions, heightData, surfaceMode = "globe") {
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
