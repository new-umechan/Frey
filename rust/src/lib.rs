mod common;
mod domains;
pub mod sim;
#[path = "generated/terrain_params_defaults.rs"]
mod terrain_params_defaults;
mod wasm;
pub use sim::world;

pub use crate::domains::types::{MeshOutput, TerrainOutput, TerrainParams};
pub use crate::sim::erosion::ErosionAutomatonState;
pub use crate::wasm::world_sim::WorldSimController;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    let output = domains::build_mesh(level).map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize mesh output: {err}")))
}

#[wasm_bindgen]
pub fn generate_terrain(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    let terrain_params = if params_js.is_undefined() || params_js.is_null() {
        TerrainParams::default()
    } else {
        serde_wasm_bindgen::from_value::<TerrainParams>(params_js)
            .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
    };

    let output = domains::build_terrain(&seed, terrain_params);
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")))
}

#[wasm_bindgen]
pub fn build_render_positions(input_js: JsValue) -> Result<JsValue, JsValue> {
    let positions = wasm::visuals::build_render_positions_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build render positions: {err}")))?;
    serde_wasm_bindgen::to_value(&positions)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize render positions: {err}")))
}
