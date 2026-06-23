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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TectonicRegime {
    StagnantLid,
    #[default]
    MobileLid,
    ShatteredLid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlateEmergenceFallbackKind {
    #[default]
    None,
    StagnantLidProtoPlates,
    ShatteredLidProtoBlocks,
    LegacyPowerVoronoi,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct InitialPlateKinematics {
    pub angular_axis: [f32; 3],
    pub angular_speed: f32,
    #[serde(default)]
    pub activity: f32,
    #[serde(default)]
    pub plume_divergence_bias: [f32; 3],
    #[serde(default)]
    pub downwelling_convergence_bias: [f32; 3],
    #[serde(default)]
    pub subduction_tendency: f32,
    #[serde(default)]
    pub craton_resistance: f32,
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
    pub pre_plate_steps: u32,
    pub pre_plate_damage_rate: f32,
    pub pre_plate_healing_decay: f32,
    pub pre_plate_boundary_ratio_min: f32,
    pub pre_plate_boundary_ratio_max: f32,
    pub pre_plate_min_region_fraction: f32,
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
    pub sink_full_rebuild_interval_ticks: u32,
    pub sink_full_rebuild_changed_ratio: f32,
    pub sink_incremental_neighbor_hops: u32,
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
    pub tectonic_uplift_gain: f32,
    pub plate_motion_gain: f32,
    pub boundary_reclassify_interval: u32,
    pub river_rebuild_interval_min: u32,
    pub river_rebuild_interval_max: u32,
    pub river_activity_high_threshold: f32,
    pub river_activity_low_threshold: f32,
    pub tectonic_subsidence_gain: f32,
    pub thermal_subsidence_gain: f32,
    pub stress_relaxation_rate: f32,
    pub isostatic_adjustment_rate: f32,
    pub subduction_age_coupling: f32,
    pub subduction_initiation_threshold: f32,
    pub subduction_density_threshold: f32,
    pub hypsometry_land_p50: f32,
    pub hypsometry_land_p90: f32,
    pub hypsometry_ocean_p50: f32,
    pub hypsometry_ocean_p90: f32,
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
    pub plate_emergence_regime: TectonicRegime,
    pub plate_emergence_fallback: PlateEmergenceFallbackKind,
    pub initial_plate_kinematics: Vec<InitialPlateKinematics>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlateEmergenceThresholdDiagnostic {
    pub boundary_ratio: f32,
    pub valid_count: u32,
    pub largest_ratio: f32,
    pub tiny_fragment_ratio: f32,
    pub single_cell_plate_count: u32,
    pub min_plate_cells: u32,
    pub final_plate_count: u32,
    pub multi_component_plate_count: u32,
    pub max_plate_component_count: u32,
    pub mean_detached_fragment_ratio: f32,
    pub max_plate_area_ratio: f32,
    pub second_plate_area_ratio: f32,
    pub effective_plate_count: f32,
    pub mean_plate_boundary_complexity: f32,
    pub max_plate_boundary_complexity: f32,
    pub regime: TectonicRegime,
    pub regime_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlateEmergenceIterationDiagnostic {
    pub step: u32,
    pub mean_abs_damage_delta: f32,
    pub max_damage_delta: f32,
    pub selected_boundary_ratio: f32,
    pub selected_valid_count: u32,
    pub selected_largest_ratio: f32,
    pub selected_tiny_fragment_ratio: f32,
    pub selected_single_cell_plate_count: u32,
    pub selected_min_plate_cells: u32,
    pub selected_final_plate_count: u32,
    pub selected_multi_component_plate_count: u32,
    pub selected_max_plate_component_count: u32,
    pub selected_mean_detached_fragment_ratio: f32,
    pub selected_max_plate_area_ratio: f32,
    pub selected_second_plate_area_ratio: f32,
    pub selected_effective_plate_count: f32,
    pub selected_mean_plate_boundary_complexity: f32,
    pub selected_max_plate_boundary_complexity: f32,
    pub selected_regime: TectonicRegime,
    pub selected_regime_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlateEmergenceDiagnostics {
    pub seed: String,
    pub level: u32,
    pub min_region: u32,
    pub base_step_budget: u32,
    pub max_step_budget: u32,
    pub settled_steps: u32,
    pub selected_boundary_ratio: f32,
    pub selected_valid_count: u32,
    pub selected_largest_ratio: f32,
    pub selected_tiny_fragment_ratio: f32,
    pub selected_single_cell_plate_count: u32,
    pub selected_min_plate_cells: u32,
    pub selected_final_plate_count: u32,
    pub selected_multi_component_plate_count: u32,
    pub selected_max_plate_component_count: u32,
    pub selected_mean_detached_fragment_ratio: f32,
    pub selected_max_plate_area_ratio: f32,
    pub selected_second_plate_area_ratio: f32,
    pub selected_effective_plate_count: f32,
    pub selected_mean_plate_boundary_complexity: f32,
    pub selected_max_plate_boundary_complexity: f32,
    pub selected_regime: TectonicRegime,
    pub selected_regime_score: f32,
    pub evolution_iterations: Vec<PlateEmergenceIterationDiagnostic>,
    pub threshold_candidates: Vec<PlateEmergenceThresholdDiagnostic>,
}
