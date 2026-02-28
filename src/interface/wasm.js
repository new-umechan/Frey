import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_mesh,
} from "../wasm/frey_wasm.js";

export default initWasm;

export {
    WorldSimController,
    build_render_positions,
    generate_mesh,
};
