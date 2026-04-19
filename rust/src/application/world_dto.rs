#![cfg(feature = "wasm_transport")]

use serde::{Deserialize, Serialize};

use crate::sim::geology_types::GeologyParams;

#[derive(Deserialize)]
pub(crate) struct InitWorldConfig {
    #[serde(default)]
    pub geology_params: Option<GeologyParams>,
    #[serde(default)]
    pub target_sea_ratio: Option<f32>,
    #[serde(default)]
    pub simulation_rate: Option<f32>,
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
