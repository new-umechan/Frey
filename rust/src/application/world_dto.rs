#![cfg(feature = "wasm_transport")]

use serde::{Deserialize, Serialize};

use crate::sim::geology_types::GeologyParams;
use verification_runtime::VerificationMode;

#[derive(Deserialize)]
pub(crate) struct InitWorldConfig {
    #[serde(default)]
    pub geology_params: Option<GeologyParams>,
    #[serde(default)]
    pub simulation_rate: Option<f32>,
    #[serde(default)]
    pub verification_mode: Option<VerificationMode>,
}

#[derive(Deserialize)]
pub(crate) struct WorldDeltaQuery {
    #[serde(default)]
    pub include_fields: Option<Vec<String>>,
}

#[derive(Serialize)]
pub(crate) struct InitWorldOutput {
    pub world_id: String,
    pub tick: f64,
    pub era: String,
    pub cell_count: u32,
}

#[derive(Serialize)]
pub(crate) struct FieldResponse {
    pub field_kind: String,
    pub stride: u32,
    pub cell_count: u32,
    pub sampled_count: u32,
    pub f32_data: Option<Vec<f32>>,
    pub u32_data: Option<Vec<u32>>,
    pub i32_data: Option<Vec<i32>>,
}

#[derive(Serialize)]
pub(crate) struct BudgetSummary {
    pub geology: u32,
    pub climate: u32,
    pub ecology: u32,
    pub civilization: u32,
}

#[derive(Serialize)]
pub(crate) struct MetricsResponse {
    pub world_id: String,
    pub tick: f64,
    pub era: String,
    pub simulation_rate: f32,
    pub real_years_per_tick: f32,
    pub runtime_tick_ms: u32,
    pub budgets: BudgetSummary,
    pub cell_count: u32,
    pub land_cells: u32,
    pub land_ratio: f32,
    pub mean_height: f32,
    pub height_std_dev: f32,
    pub mean_river_flux: f32,
    pub max_height: f32,
    pub min_height: f32,
    pub max_river_flux: f32,
    pub top10_river_flux_sum: f32,
    pub river_active_cells: u32,
    pub river_fragmentation_ratio: f32,
    pub river_ocean_reach_ratio: f32,
    pub river_mainstem_persistence: f32,
    pub river_flux_concentration: f32,
    pub continent_count: u32,
    pub largest_continent_cells: u32,
    pub global_sediment_export: f32,
    pub marine_sediment_mass: f32,
    pub solid_earth_mass_proxy: f32,
    pub solid_earth_mass_proxy_drift: f32,
    pub ocean_water_inventory: f32,
    pub ocean_water_inventory_drift: f32,
    pub ice_inventory: f32,
}

#[derive(Serialize)]
pub(crate) struct ScientificBenchmarkMetricsResponse {
    pub cell_count: u32,
    pub land_cells: u32,
    pub land_ratio: f32,
    pub mean_height: f32,
    pub height_std_dev: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub mean_river_flux: f32,
    pub max_river_flux: f32,
    pub top10_river_flux_sum: f32,
    pub river_active_cells: u32,
    pub river_fragmentation_ratio: f32,
    pub river_ocean_reach_ratio: f32,
    pub river_mainstem_persistence: f32,
    pub river_flux_concentration: f32,
    pub continent_count: u32,
    pub largest_continent_cells: u32,
    pub global_sediment_export: f32,
    pub marine_sediment_mass: f32,
    pub solid_earth_mass_proxy: f32,
    pub solid_earth_mass_proxy_drift: f32,
    pub ocean_water_inventory: f32,
    pub ocean_water_inventory_drift: f32,
    pub ice_inventory: f32,
}

#[derive(Serialize)]
pub(crate) struct ScientificBenchmarkSampleResponse {
    pub tick: f64,
    pub era: String,
    pub metrics: ScientificBenchmarkMetricsResponse,
}

#[derive(Serialize)]
pub(crate) struct ScientificBenchmarkSamplesResponse {
    pub world_id: String,
    pub sample_count: u32,
    pub samples: Vec<ScientificBenchmarkSampleResponse>,
}

#[derive(Serialize)]
pub(crate) struct PlateStat {
    pub plate_id: u32,
    pub cell_count: u32,
    pub mean_height: f32,
    pub land_ratio: f32,
    pub mean_river_flux: f32,
}

#[derive(Serialize)]
pub(crate) struct PlateStatsResponse {
    pub world_id: String,
    pub tick: f64,
    pub plate_count: u32,
    pub stats: Vec<PlateStat>,
}

#[derive(Serialize)]
pub(crate) struct HistoryTicksResponse {
    pub world_id: String,
    pub interval: u32,
    pub ticks: Vec<f64>,
}

#[derive(Serialize)]
pub(crate) struct RestoreWorldResult {
    pub world_id: String,
    pub tick: f64,
}

#[derive(Serialize)]
pub(crate) struct ForkWorldOutput {
    pub source_world_id: String,
    pub world_id: String,
    pub tick: f64,
}

#[derive(Serialize)]
pub(crate) struct DeltaRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Serialize)]
pub(crate) struct FieldDeltaResponse {
    pub field_kind: String,
    pub mode: String,
    pub ranges: Vec<DeltaRange>,
    pub dirty_bitmap: Option<Vec<u32>>,
    pub f32_data: Option<Vec<f32>>,
    pub u32_data: Option<Vec<u32>>,
    pub i32_data: Option<Vec<i32>>,
}

#[derive(Serialize)]
pub(crate) struct WorldDeltaResponse {
    pub world_id: String,
    pub tick: f64,
    pub era: String,
    pub real_years_per_tick: f32,
    pub runtime_tick_ms: u32,
    pub budgets: BudgetSummary,
    pub deltas: Vec<FieldDeltaResponse>,
}

#[derive(Serialize)]
pub(crate) struct StepWorldProfiledResponse {
    pub world_id: String,
    pub steps: u32,
    pub exec_feedback_ms: f64,
    pub exec_geology_terrain_ms: f64,
    pub exec_climate_ms: f64,
    pub exec_glaciology_ms: f64,
    pub exec_hydrology_ms: f64,
    pub exec_ecology_ms: f64,
    pub exec_society_ms: f64,
    pub exec_transition_ms: f64,
    pub step_sync_erosion_ms: f64,
    pub step_observe_world_change_ms: f64,
    pub step_history_snapshot_ms: f64,
}

#[derive(Serialize)]
pub(crate) struct StepWorldProfiledDetailResponse {
    pub world_id: String,
    pub steps: u32,
    pub exec_feedback_ms: f64,
    pub exec_geology_terrain_ms: f64,
    pub exec_climate_ms: f64,
    pub exec_glaciology_ms: f64,
    pub exec_hydrology_ms: f64,
    pub exec_ecology_ms: f64,
    pub exec_society_ms: f64,
    pub exec_transition_ms: f64,
    pub step_sync_erosion_ms: f64,
    pub step_observe_world_change_ms: f64,
    pub step_history_snapshot_ms: f64,
    pub step_geology_river_prepare_ms: f64,
    pub step_geology_river_automaton_ms: f64,
    pub step_geology_river_automaton_sink_ms: f64,
    pub step_geology_river_automaton_cell_ms: f64,
    pub step_geology_river_automaton_queue_ms: f64,
    pub step_geology_river_network_ms: f64,
    pub step_geology_river_sync_ms: f64,
    pub step_geology_river_fallback_ms: f64,
    pub river_network_rebuild_count: u32,
    pub river_fallback_count: u32,
    pub sink_rebuild_full_count: u32,
    pub sink_rebuild_partial_count: u32,
    pub sink_rebuild_skipped_count: u32,
    pub sink_rebuild_fallback_full_count: u32,
    pub step_geology_river_sink_incremental_rebuild_ms: f64,
    pub step_geology_river_sink_full_rebuild_ms: f64,
    pub sink_affected_ratio: f64,
    pub sink_validation_fail_count: u32,
}

#[derive(Serialize)]
pub(crate) struct ExecWorldSliceResponse {
    pub world_id: String,
    pub processed_ticks: u32,
    pub busy: bool,
    pub phase: String,
    pub tick: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CausalFeatureType {
    BorderSegment,
    RidgeOrMountainBand,
    TectonicCompressionOrPlateBoundary,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CausalRelationType {
    ConstraintAlignment,
    GeomorphicStructure,
    TectonicDriver,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceType {
    Morphology,
    PassabilityProxy,
    TectonicProxy,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UncertaintyStage {
    Low,
    Medium,
    High,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalLocationPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalMetricValue {
    pub metric_id: String,
    pub label: String,
    pub value: f32,
    pub unit: String,
    pub display_value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalFeatureDescriptor {
    pub feature_id: String,
    pub feature_type: CausalFeatureType,
    pub label: String,
    pub short_label: String,
    pub anchor: CausalLocationPoint,
    pub metrics: Vec<CausalMetricValue>,
    pub uncertainty_stage: UncertaintyStage,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalTraceSegment {
    pub trace_id: String,
    pub label: String,
    pub source_feature_id: String,
    pub target_feature_id: String,
    pub relation_type: CausalRelationType,
    pub path: Vec<CausalLocationPoint>,
    pub metrics: Vec<CausalMetricValue>,
    pub uncertainty_stage: UncertaintyStage,
    pub evidence_ids: Vec<String>,
    pub display_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalDisplayFeatureStyle {
    pub feature_id: String,
    pub color_hex: String,
    pub glow_intensity: f32,
    pub pulse_hz: f32,
    pub radius: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalDisplayTraceStyle {
    pub trace_id: String,
    pub color_hex: String,
    pub thickness: f32,
    pub flow_speed: f32,
    pub jitter_amplitude: f32,
    pub label_short: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalDisplayMapping {
    pub feature_styles: Vec<CausalDisplayFeatureStyle>,
    pub trace_styles: Vec<CausalDisplayTraceStyle>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalEvidenceEntry {
    pub evidence_id: String,
    pub trace_id: String,
    pub evidence_type: EvidenceType,
    pub summary: String,
    pub assumptions: Vec<String>,
    pub approximations: Vec<String>,
    pub uncertainty_reason: String,
    pub reference_model: String,
    pub reference_notes: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct CausalExplorationDemoResponse {
    pub demo_id: String,
    pub features: Vec<CausalFeatureDescriptor>,
    pub trace_segments: Vec<CausalTraceSegment>,
    pub metrics: Vec<CausalMetricValue>,
    pub display_mapping: CausalDisplayMapping,
    pub evidence: Vec<CausalEvidenceEntry>,
}
