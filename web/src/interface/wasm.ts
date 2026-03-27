import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
    type InitInput,
    type InitOutput,
} from "../../../generated/wasm/web/frey_wasm";

export default initWasm as (input?: InitInput | Promise<InitInput>) => Promise<InitOutput>;

export {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
};
