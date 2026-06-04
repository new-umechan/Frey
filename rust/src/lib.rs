#[cfg(feature = "wasm_transport")]
pub mod application;
#[path = "generated/climate_params_defaults.rs"]
mod climate_params_defaults;
mod common;
pub mod core_api;
#[path = "generated/domesticates_params_defaults.rs"]
mod domesticates_params_defaults;
#[path = "generated/glaciology_params_defaults.rs"]
mod glaciology_params_defaults;
#[cfg(feature = "precompute_server")]
pub mod precompute_server;
pub mod sim;
#[path = "generated/terrain_params_defaults.rs"]
mod terrain_params_defaults;
pub mod transport;
#[cfg(feature = "wasm_transport")]
mod wasm_api;
pub use sim::world;

pub use crate::sim::erosion::ErosionAutomatonState;
pub use crate::sim::geology_types::{
    CrustType, GeologyInternal, GeologyOutput, GeologyParams, MeshOutput, PlateId, PlateRelation,
    StressTensor, SubductionPolarity,
};
#[cfg(feature = "wasm_transport")]
pub use crate::transport::wasm::WorldSimController;
#[cfg(feature = "wasm_transport")]
use wasm_bindgen::prelude::*;

pub fn generate_mesh_core(level: u32) -> Result<MeshOutput, String> {
    core_api::generate_mesh(level)
}

pub fn generate_geology_core(seed: &str, geology_params: GeologyParams) -> GeologyOutput {
    core_api::generate_geology(seed, geology_params)
}

#[cfg(feature = "wasm_transport")]
#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    transport::wasm::generate_mesh(level)
}

#[cfg(feature = "wasm_transport")]
#[wasm_bindgen]
pub fn generate_geology(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    transport::wasm::generate_geology(seed, params_js)
}

#[cfg(feature = "wasm_transport")]
#[wasm_bindgen]
pub fn build_render_positions(input_js: JsValue) -> Result<JsValue, JsValue> {
    transport::wasm::build_render_positions(input_js)
}
