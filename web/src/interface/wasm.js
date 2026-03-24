import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
} from "../../../generated/wasm/web/frey_wasm.js";

export default initWasm;

export {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
};
