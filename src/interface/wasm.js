import initWasm, {
    apply_land_ratio_floor,
    CrustTerrainAutomaton,
    WorldTimeController,
    build_render_positions,
    generate_mesh,
    init_erosion_automaton,
    step_layers_bundle,
    step_erosion_automaton,
} from "../wasm/frey_wasm.js";

export default initWasm;

export {
    apply_land_ratio_floor,
    CrustTerrainAutomaton,
    WorldTimeController,
    build_render_positions,
    generate_mesh,
    init_erosion_automaton,
    step_layers_bundle,
    step_erosion_automaton,
};
