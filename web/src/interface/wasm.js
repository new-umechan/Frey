/**
 * @typedef {import("../../../generated/wasm/web/frey_wasm.js")} FreyWasm
 */

import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
} from "../../../generated/wasm/web/frey_wasm.js";

/**
 * @type {FreyWasm["default"]}
 */
export default initWasm;

export {
    /** @type {FreyWasm["WorldSimController"]} */
    WorldSimController,
    /** @type {FreyWasm["build_render_positions"]} */
    build_render_positions,
    /** @type {FreyWasm["generate_geology"]} */
    generate_geology,
    /** @type {FreyWasm["generate_mesh"]} */
    generate_mesh,
};
