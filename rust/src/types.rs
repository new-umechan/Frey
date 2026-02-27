use crate::terrain_params_defaults;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct MeshOutput {
    pub(crate) positions: Vec<f32>,
    pub(crate) indices: Vec<u32>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
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

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ErosionAutomatonState {
    pub positions: Vec<[f32; 3]>,
    pub nbr_offsets: Vec<u32>,
    pub nbrs: Vec<u32>,
    pub height: Vec<f32>,
    pub water: Vec<f32>,
    pub sediment: Vec<f32>,
    pub armor: Vec<f32>,
    pub rain: Vec<f32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
    pub active_queue: Vec<u32>,
    pub active_head: usize,
    pub in_queue: Vec<u8>,
    pub rain_cursor: usize,
    pub tick: u64,
    pub recent_changed: Vec<u32>,
    pub params: TerrainParams,
}
