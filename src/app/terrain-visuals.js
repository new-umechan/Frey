import {
    build_render_positions as wasmBuildRenderPositions,
    build_vertex_colors as wasmBuildVertexColors,
} from "../wasm/frey_wasm.js";

export function buildVertexColors(
    heightData,
    plateId,
    riverFlux,
    lakeDepth,
    viewMode,
    debugEnabled = false,
    tectonicDebug = null,
) {
    const colors = wasmBuildVertexColors({
        height_data: heightData,
        plate_id: plateId,
        river_flux: riverFlux,
        lake_depth: lakeDepth ?? null,
        view_mode: viewMode,
        debug_enabled: debugEnabled,
        tectonic_debug: tectonicDebug ?? null,
    });
    return new Float32Array(colors);
}

export function buildRenderPositions(basePositions, heightData, surfaceMode = "globe") {
    const positions = wasmBuildRenderPositions({
        base_positions: basePositions,
        height_data: heightData,
        surface_mode: surfaceMode,
    });
    return new Float32Array(positions);
}
