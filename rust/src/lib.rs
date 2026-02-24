mod core;

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
    pub l_max: u32,
    pub alpha: f32,
    pub num_plates_min: u32,
    pub num_plates_max: u32,
    pub ocean_plate_ratio: f32,
    pub boundary_band: f32,
    pub uplift_gain: f32,
    pub subduct_gain: f32,
    pub divergent_gain: f32,
    pub smooth_iter: u32,
    pub smooth_lambda: f32,
    pub river_rain_base: f32,
    pub river_accum_threshold: f32,
    pub erosion_iter: u32,
    pub hydraulic_erode_rate: f32,
    pub hydraulic_deposit_rate: f32,
    pub sediment_capacity_gain: f32,
    pub erosion_min_slope: f32,
    pub erosion_max_delta_per_iter: f32,
    pub coastal_deposit_rate: f32,
    pub shallow_sea_floor: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            level: 6,
            l_max: 4,
            alpha: 1.5,
            num_plates_min: 8,
            num_plates_max: 18,
            ocean_plate_ratio: 0.65,
            boundary_band: 0.08,
            uplift_gain: 0.45,
            subduct_gain: 0.35,
            divergent_gain: 0.20,
            smooth_iter: 6,
            smooth_lambda: 0.35,
            river_rain_base: 0.5,
            river_accum_threshold: 0.015,
            erosion_iter: 12,
            hydraulic_erode_rate: 0.020,
            hydraulic_deposit_rate: 0.35,
            sediment_capacity_gain: 0.90,
            erosion_min_slope: 0.002,
            erosion_max_delta_per_iter: 0.015,
            coastal_deposit_rate: 0.45,
            shallow_sea_floor: -0.08,
        }
    }
}

#[derive(Serialize)]
pub struct TerrainOutput {
    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
}

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    core::generate_mesh(level)
}

#[wasm_bindgen]
pub fn generate_terrain(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    core::generate_terrain(seed, params_js)
}
