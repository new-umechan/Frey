import {
    build_render_positions as wasmBuildRenderPositions,
} from "../wasm/frey_wasm.js";

export function buildRenderPositions(basePositions, heightData, surfaceMode = "globe") {
    const positions = wasmBuildRenderPositions({
        base_positions: basePositions,
        height_data: heightData,
        surface_mode: surfaceMode,
    });
    return new Float32Array(positions);
}
