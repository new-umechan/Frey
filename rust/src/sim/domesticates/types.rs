use serde::{Deserialize, Serialize};

use crate::sim::world::{CropBitmap, LivestockBitmap, N_CROPS, N_LIVESTOCK};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomesticatesParams {
    pub origin_count_limit: u32,
    pub origin_min_region_cells: u32,
    pub origin_seed_strength_crop: f32,
    pub origin_seed_strength_livestock: f32,
    pub max_dt: f32,
    pub moisture_precip_scale_mm: f32,
    pub moisture_aridity_scale: f32,
    pub origin_candidate_cutoff_ratio: f32,
    pub origin_top_candidate_ratio: f32,
    pub diffusion_memory_decay: f32,
    pub diffusion_memory_gain: f32,
    pub diffusion_memory_river_w: f32,
    pub diffusion_memory_height_w: f32,
    pub diffusion_memory_biome_w: f32,
}

impl Default for DomesticatesParams {
    fn default() -> Self {
        crate::domesticates_params_defaults::build_default_domesticates_params()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DomesticatesBenchDiagnostics {
    pub crop_niche: Vec<[f32; N_CROPS]>,
    pub livestock_niche: Vec<[f32; N_LIVESTOCK]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CropKind {
    Wheat = 0,
    Rice = 1,
    Maize = 2,
    Millet = 3,
    Potato = 4,
    Cassava = 5,
    Sorghum = 6,
    Yam = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LivestockKind {
    Cattle = 0,
    Horse = 1,
    Sheep = 2,
    Pig = 3,
    Camel = 4,
}

pub const ALL_CROPS: [CropKind; N_CROPS] = [
    CropKind::Wheat,
    CropKind::Rice,
    CropKind::Maize,
    CropKind::Millet,
    CropKind::Potato,
    CropKind::Cassava,
    CropKind::Sorghum,
    CropKind::Yam,
];

pub const ALL_LIVESTOCK: [LivestockKind; N_LIVESTOCK] = [
    LivestockKind::Cattle,
    LivestockKind::Horse,
    LivestockKind::Sheep,
    LivestockKind::Pig,
    LivestockKind::Camel,
];

#[inline]
pub fn crop_index(kind: CropKind) -> usize {
    kind as usize
}

#[inline]
pub fn livestock_index(kind: LivestockKind) -> usize {
    kind as usize
}

#[inline]
pub fn crop_mask(kind: CropKind) -> CropBitmap {
    1u8 << (kind as u8)
}

#[inline]
pub fn livestock_mask(kind: LivestockKind) -> LivestockBitmap {
    1u8 << (kind as u8)
}
