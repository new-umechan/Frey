#[path = "core.rs"]
mod core;
#[path = "generated/terrain_params_defaults.rs"]
mod terrain_params_defaults;
mod wasm_visuals;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
pub struct MeshOutput {
    positions: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TerrainParams {
    pub level: u32,
    pub harmonic_max_l: u32,
    pub spectral_alpha: f32,
    pub plate_count_min: u32,
    pub plate_count_max: u32,
    pub ocean_plate_ratio: f32,
    pub boundary_band: f32,
    pub boundary_convergent_base_gain: f32,
    pub boundary_divergent_base_gain: f32,
    pub boundary_transform_relief_gain: f32,
    pub trench_gain: f32,
    pub arc_gain: f32,
    pub collision_gain: f32,
    pub rift_gain: f32,
    pub boundary_trench_width: f32,
    pub boundary_arc_width: f32,
    pub boundary_collision_width: f32,
    pub boundary_rift_width: f32,
    pub boundary_obliquity_mix: f32,
    pub boundary_distance_falloff: f32,
    pub boundary_anisotropy: f32,
    pub river_rain_base: f32,
    pub river_accumulation_threshold: f32,
    pub erosion_iterations: u32,
    pub hydraulic_erosion_rate: f32,
    pub hydraulic_deposit_rate: f32,
    pub sediment_capacity_gain: f32,
    pub erosion_min_slope: f32,
    pub erosion_max_delta_per_iter: f32,
    pub coastal_deposit_rate: f32,
    pub shallow_sea_floor: f32,
    pub continent_competence_noise_gain: f32,
    pub continent_competence_large_scale: f32,
    pub continent_competence_mid_scale: f32,
    pub continent_competence_weight_gain: f32,
    pub continent_foldability_from_competence: f32,
    pub continent_erodibility_from_competence: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        terrain_params_defaults::build_default_terrain_params()
    }
}

#[derive(Serialize)]
pub struct TerrainOutput {
    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub plate_count: u32,
    pub land_ratio: f32,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
    pub lake_depth: Vec<f32>,
    pub vertex_weight: Vec<f32>,
    pub plate_is_ocean: Vec<u8>,
    pub plate_base_height: Vec<f32>,
    pub plate_base_weight: Vec<f32>,
    pub debug_trench_strength: Vec<f32>,
    pub debug_arc_strength: Vec<f32>,
    pub debug_backarc_strength: Vec<f32>,
    pub debug_ocean_ocean_arc_strength: Vec<f32>,
}

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    let output = core::build_mesh(level).map_err(|err| JsValue::from_str(&err))?;
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

    let output = core::build_terrain(&seed, terrain_params);
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")))
}

#[wasm_bindgen]
pub fn build_render_positions(input_js: JsValue) -> Result<JsValue, JsValue> {
    let positions = wasm_visuals::build_render_positions_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build render positions: {err}")))?;
    serde_wasm_bindgen::to_value(&positions)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize render positions: {err}")))
}

#[wasm_bindgen]
pub fn build_vertex_colors(input_js: JsValue) -> Result<JsValue, JsValue> {
    let colors = wasm_visuals::build_vertex_colors_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build vertex colors: {err}")))?;
    serde_wasm_bindgen::to_value(&colors)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize vertex colors: {err}")))
}
