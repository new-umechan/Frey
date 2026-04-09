#[path = "generated/climate_params_defaults.rs"]
mod climate_params_defaults;
mod common;
#[path = "generated/glaciology_params_defaults.rs"]
mod glaciology_params_defaults;
pub mod sim;
#[path = "generated/terrain_params_defaults.rs"]
mod terrain_params_defaults;
mod wasm_api;
pub use sim::world;

pub use crate::sim::erosion::ErosionAutomatonState;
pub use crate::sim::geology_types::{
    CrustType, GeologyInternal, GeologyOutput, GeologyParams, MeshOutput, PlateId, PlateRelation,
    StressTensor, SubductionPolarity,
};
pub use crate::wasm_api::world_sim::WorldSimController;
use wasm_bindgen::prelude::*;

pub fn generate_mesh_core(level: u32) -> Result<MeshOutput, String> {
    sim::build_mesh(level)
}

pub fn generate_geology_core(seed: &str, geology_params: GeologyParams) -> GeologyOutput {
    sim::build_geology(seed, geology_params)
}

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    let output = generate_mesh_core(level).map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize mesh output: {err}")))
}

#[wasm_bindgen]
pub fn generate_geology(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    let geology_params = if params_js.is_undefined() || params_js.is_null() {
        GeologyParams::default()
    } else {
        serde_wasm_bindgen::from_value::<GeologyParams>(params_js)
            .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
    };

    let output = generate_geology_core(&seed, geology_params);
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")))
}

#[wasm_bindgen]
pub fn build_render_positions(input_js: JsValue) -> Result<JsValue, JsValue> {
    let positions = wasm_api::visuals::build_render_positions_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build render positions: {err}")))?;
    serde_wasm_bindgen::to_value(&positions)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize render positions: {err}")))
}
