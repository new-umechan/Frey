use crate::terrain_params_defaults;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct MeshOutput {
    pub(crate) positions: Vec<f32>,
    pub(crate) indices: Vec<u32>,
    pub(crate) cell_overlay_positions: Vec<f32>,
    pub(crate) cell_overlay_cell_ids: Vec<u32>,
    pub(crate) cell_overlay_lift: Vec<f32>,
}

/// 地殻タイプ。海洋地殻と大陸地殻を区別する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CrustType {
    #[default]
    Continental,
    Oceanic,
}

/// 応力テンソル。2D平面応力状態を表現する
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct StressTensor {
    pub xx: f32,
    pub yy: f32,
    pub xy: f32,
}

/// 地殻内部状態。GeologySystem内部で保持する永続状態
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GeologyInternal {
    #[serde(default)]
    pub crust_type: CrustType,
    #[serde(default)]
    pub age: f32,
    #[serde(default = "default_thickness")]
    pub thickness: f32,
    #[serde(default = "default_density")]
    pub density: f32,
    #[serde(default)]
    pub stress: StressTensor,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default = "default_rigidity")]
    pub rigidity: f32,
    #[serde(default)]
    pub arc_volcanism: f32,
    #[serde(default)]
    pub ridge_volcanism: f32,
    #[serde(default)]
    pub hotspot_volcanism: f32,
    #[serde(default)]
    pub backarc_volcanism: f32,
}

fn default_thickness() -> f32 {
    30.0
}

fn default_density() -> f32 {
    2700.0
}

fn default_rigidity() -> f32 {
    30e9
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct GeologyParams {
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
    pub rollback_gain: f32,
    pub rollback_suppression: f32,
    pub rollback_fraction_max: f32,
    pub rollback_threshold: f32,
    pub backarc_tension_gain: f32,
    pub dip_density_scale: f32,
    pub subduction_depth_gain: f32,
    pub convergence_memory_rate: f32,
    pub convergence_memory_spatial_smooth: f32,
    pub arc_volcanism_gain: f32,
    pub ridge_volcanism_gain: f32,
    pub hotspot_volcanism_gain: f32,
    pub backarc_volcanism_gain: f32,
    pub volcanic_uplift_gain: f32,
    pub volcanic_thickening_gain: f32,
    pub river_rain_base: f32,
    pub river_accumulation_threshold: f32,
    pub sink_local_rebuild_radius: u32,
    pub sink_overflow_hysteresis: f32,
    pub sink_min_capacity: f32,
    pub erosion_iterations: u32,
    pub hydraulic_erosion_rate: f32,
    pub hydraulic_deposit_rate: f32,
    pub sediment_capacity_gain: f32,
    pub erosion_min_slope: f32,
    pub erosion_max_delta_per_iter: f32,
    pub coastal_deposit_rate: f32,
    pub shallow_sea_floor: f32,
    pub river_inertia_gain: f32,
    pub river_curvature_penalty: f32,
    pub baseflow_infiltration_rate: f32,
    pub baseflow_release_rate: f32,
    pub baseflow_storage_cap: f32,
    pub continent_competence_noise_gain: f32,
    pub continent_competence_large_scale: f32,
    pub continent_competence_mid_scale: f32,
    pub continent_competence_weight_gain: f32,
    pub continent_foldability_from_competence: f32,
    pub continent_erodibility_from_competence: f32,
    pub mantle_density: f32,
    pub continental_crust_density: f32,
    pub oceanic_base_density: f32,
    pub age_density_gain: f32,
    pub erosion_thickness_coupling: f32,
    pub deposition_thickness_coupling: f32,
    #[serde(alias = "uplift_rate_gain")]
    pub tectonic_uplift_gain: f32,
    pub plate_motion_gain: f32,
    pub boundary_reclassify_interval: u32,
    pub river_rebuild_interval_min: u32,
    pub river_rebuild_interval_max: u32,
    pub river_activity_high_threshold: f32,
    pub river_activity_low_threshold: f32,
    #[serde(alias = "subsidence_rate_gain")]
    pub tectonic_subsidence_gain: f32,
    #[serde(alias = "marine_subsidence_gain")]
    pub thermal_subsidence_gain: f32,
    pub stress_relaxation_rate: f32,
    #[serde(alias = "isostasy_rate")]
    pub isostatic_adjustment_rate: f32,
    pub subduction_age_coupling: f32,
    pub subduction_initiation_threshold: f32,
    pub subduction_density_threshold: f32,
    pub mantle_heat_input: f32,
    pub mantle_heat_loss: f32,
    pub mantle_diffusion_rate: f32,
    pub plume_threshold: f32,
    pub plume_gain: f32,
    pub plume_heat_release_rate: f32,
    pub uplift_saturation_soft: f32,
    pub uplift_saturation_hard: f32,
    pub age_advection_gain: f32,
    pub nonlinear_diffusion_gain: f32,
    pub isostatic_relax_gain: f32,
    pub age_ref: f32,
}

impl Default for GeologyParams {
    fn default() -> Self {
        terrain_params_defaults::build_default_terrain_params()
    }
}

/// プレート ID。newtype パターンで型安全性を確保する
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Ord, PartialOrd,
)]
#[serde(transparent)]
pub struct PlateId(pub u32);

impl PlateId {
    pub fn as_u32(self) -> u32 {
        self.0
    }

    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// プレート間の関係。相対運動と境界の性質を記録する
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct PlateRelation {
    /// 相対速度ベクトル（3 次元）
    #[serde(default)]
    pub relative_velocity: [f32; 3],
    /// 収束の継続度合い（0..1）
    #[serde(default)]
    pub convergence_memory: f32,
    /// 沈み込み極性
    #[serde(default)]
    pub subduction_polarity: SubductionPolarity,
    /// 境界の傾斜角（ラジアン）
    #[serde(default)]
    pub dip_angle: f32,
    /// ロールバック割合（0..1）
    #[serde(default)]
    pub rollback_fraction: f32,
}

/// 沈み込み極性。どちらのプレートが沈み込んでいるか
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubductionPolarity {
    /// プレート A が下
    AUnderB,
    /// プレート B が下
    BUnderA,
    /// 沈み込みなし
    #[default]
    None,
}

#[derive(Serialize)]
pub struct GeologyOutput {
    pub height: Vec<f32>,
    pub plate_id: Vec<PlateId>,
    pub plate_count: u32,
    pub land_ratio: f32,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
    pub volcanism: Vec<f32>,
    pub vertex_buoyancy: Vec<f32>,
    pub lake_depth: Vec<f32>,
    pub vertex_weight: Vec<f32>,
    pub plate_is_ocean: Vec<u8>,
    pub plate_base_height: Vec<f32>,
    pub plate_base_weight: Vec<f32>,
    pub vertex_age_norm: Vec<f32>,
    pub debug_trench_strength: Vec<f32>,
    pub debug_arc_strength: Vec<f32>,
    pub debug_backarc_strength: Vec<f32>,
    pub debug_ocean_ocean_arc_strength: Vec<f32>,
}
