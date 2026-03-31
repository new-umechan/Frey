use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClimateParams {
    pub lapse_rate_c_per_km: f32,
    pub height_to_meters: f32,
    pub precip_min_mm: f32,
    pub precip_max_mm: f32,
    pub hadley_anomaly_gain: f32,
    pub distance_scale_km: f32,
    pub continentality_gain: f32,
    pub moisture_convergence_gain: f32,
    pub convergence_min_mm: f32,
    pub convergence_max_mm: f32,
    pub convergence_blend: f32,
    pub orographic_uplift_gain_mm: f32,
    pub orographic_rise_scale_m: f32,
    pub orographic_trace_steps: u32,
    pub orographic_trace_alignment_min: f32,
    pub orographic_step_decay: f32,
    pub rain_shadow_gain: f32,
    pub rain_shadow_scale_m: f32,
    pub rain_shadow_distance_km: f32,
    pub downwind_depletion_gain: f32,
    pub downwind_depletion_max: f32,
    pub downwind_depletion_steps: u32,
    pub downwind_depletion_passes: u32,
    pub downwind_depletion_decay: f32,
    pub downwind_alignment_min: f32,
    pub precip_cap_from_moisture: f32,
    pub cold_coast_gain: f32,
    pub cold_relax_hotspot_weight: f32,
    pub hotspot_precip_gain_mm: f32,
    pub hotspot_coast_distance_km: f32,
    pub hotspot_fetch_weight: f32,
    pub hotspot_convergence_weight: f32,
    pub core_substeps: u32,
    pub core_temperature_diffusion_gain: f32,
    pub core_moisture_transport_gain: f32,
    pub core_condense_excess_gain: f32,
    pub core_orographic_condense_gain: f32,
    pub core_ocean_evaporation_gain: f32,
    pub core_land_recycle_gain: f32,
    pub core_land_bucket_capacity_mm: f32,
    pub core_land_drainage_gain: f32,
    pub core_land_relaxation_years: f32,
    pub core_humidity_floor_mm: f32,
    pub core_humidity_ref_mm: f32,
    pub core_humidity_cc_rate_per_c: f32,
}

impl Default for ClimateParams {
    fn default() -> Self {
        crate::climate_params_defaults::build_default_climate_params()
    }
}
