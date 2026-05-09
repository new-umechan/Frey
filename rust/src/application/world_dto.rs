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
    #[serde(default)]
    pub timeline: Option<TimelineConfig>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct TimelineConfig {
    #[serde(default)]
    pub checkpoint_interval: Option<u64>,
    #[serde(default)]
    pub checkpoint_limit: Option<usize>,
    #[serde(default)]
    pub undo_log_limit: Option<usize>,
    #[serde(default)]
    pub undo_future_prune_grace_ticks: Option<u64>,
    #[serde(default)]
    pub max_estimated_bytes: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct ViewDeltaQuery {
    #[serde(default)]
    pub include_fields: Option<Vec<String>>,
}

#[allow(dead_code)]
pub(crate) type WorldDeltaQuery = ViewDeltaQuery;

#[derive(Serialize)]
pub(crate) struct InitWorldOutput {
    pub world_id: String,
    pub tick: f64,
    pub head_tick: f64,
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
    pub sea_level_offset: f32,
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
    pub sea_level_offset: f32,
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
pub(crate) struct CheckpointTicksResponse {
    pub world_id: String,
    pub interval: u32,
    pub ticks: Vec<f64>,
}

#[allow(dead_code)]
pub(crate) type HistoryTicksResponse = CheckpointTicksResponse;

#[derive(Serialize)]
pub(crate) struct SeekWorldResult {
    pub world_id: String,
    pub tick: f64,
    pub head_tick: f64,
}

#[allow(dead_code)]
pub(crate) type RestoreWorldResult = SeekWorldResult;

#[derive(Serialize)]
pub(crate) struct RewindWorldResult {
    pub world_id: String,
    pub tick: f64,
    pub head_tick: f64,
    pub rewound_ticks: u32,
}

#[derive(Serialize)]
pub(crate) struct DeltaRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Serialize)]
pub(crate) struct ViewDeltaFieldResponse {
    pub field_kind: String,
    pub mode: String,
    pub ranges: Vec<DeltaRange>,
    pub dirty_bitmap: Option<Vec<u32>>,
    pub f32_data: Option<Vec<f32>>,
    pub u32_data: Option<Vec<u32>>,
    pub i32_data: Option<Vec<i32>>,
}

#[allow(dead_code)]
pub(crate) type FieldDeltaResponse = ViewDeltaFieldResponse;

#[derive(Serialize)]
pub(crate) struct ViewDeltaResponse {
    pub world_id: String,
    pub tick: f64,
    pub head_tick: f64,
    pub era: String,
    pub real_years_per_tick: f32,
    pub runtime_tick_ms: u32,
    pub budgets: BudgetSummary,
    pub deltas: Vec<ViewDeltaFieldResponse>,
}

#[allow(dead_code)]
pub(crate) type WorldDeltaResponse = ViewDeltaResponse;

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
    pub head_tick: f64,
    pub tick_boundary: String,
}

#[derive(Serialize)]
pub(crate) struct TimelineAdvanceResult {
    pub world_id: String,
    pub tick: f64,
    pub head_tick: f64,
    pub advanced_ticks: u32,
}

#[derive(Serialize)]
pub(crate) struct TimelineStateResponse {
    pub world_id: String,
    pub current_tick: f64,
    pub head_tick: f64,
    pub checkpoint_interval: u32,
    pub checkpoint_limit: u32,
    pub checkpoint_count: u32,
    pub checkpoint_start_tick: Option<f64>,
    pub checkpoint_end_tick: Option<f64>,
    pub checkpoint_estimated_bytes: f64,
    pub undo_log_limit: u32,
    pub undo_future_prune_grace_ticks: f64,
    pub undo_log_count: u32,
    pub undo_log_start_tick: Option<f64>,
    pub undo_log_end_tick: Option<f64>,
    pub undo_log_estimated_bytes: f64,
    pub total_estimated_bytes: f64,
    pub max_estimated_bytes: Option<f64>,
    pub tick_boundary: String,
}
