use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlaciologyParams {
    pub accum_temp_threshold_c: f32,
    pub ablation_temp_threshold_c: f32,
    pub accumulation_gain: f32,
    pub accumulation_temp_sensitivity: f32,
    pub ablation_gain: f32,
    pub thickness_response_rate: f32,
    pub melt_runoff_gain: f32,
    pub erosion_gain: f32,
    pub glacial_erosion_coupling: f32,
    pub sea_level_coupling: f32,
    pub ice_load_to_bedrock_coupling: f32,
    pub isostatic_adjustment_rate: f32,
}

impl Default for GlaciologyParams {
    fn default() -> Self {
        crate::glaciology_params_defaults::build_default_glaciology_params()
    }
}
