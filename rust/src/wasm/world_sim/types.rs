use serde::{Deserialize, Serialize};

use crate::domains::types::TerrainParams;

#[derive(Deserialize)]
pub(super) struct InitWorldConfig {
    #[serde(default)]
    pub terrain_params: Option<TerrainParams>,
    #[serde(default)]
    pub target_sea_ratio: Option<f32>,
    #[serde(default)]
    pub simulation_rate: Option<f32>,
}

#[derive(Deserialize)]
pub(super) struct InterventionOp {
    pub cell_id: u32,
    pub field: String,
    pub value: f64,
}

#[derive(Serialize)]
pub(super) struct InitWorldOutput {
    pub world_id: String,
    pub tick: f64,
    pub era: String,
    pub cell_count: u32,
}

#[derive(Serialize)]
pub(super) struct FieldResponse {
    pub field_kind: String,
    pub stride: u32,
    pub cell_count: u32,
    pub sampled_count: u32,
    pub f32_data: Option<Vec<f32>>,
    pub u32_data: Option<Vec<u32>>,
    pub i32_data: Option<Vec<i32>>,
}

#[derive(Serialize)]
pub(super) struct BudgetSummary {
    pub geology: u32,
    pub climate: u32,
    pub ecology: u32,
    pub civilization: u32,
}

#[derive(Serialize)]
pub(super) struct MetricsResponse {
    pub world_id: String,
    pub tick: f64,
    pub era: String,
    pub simulation_rate: f32,
    pub real_years_per_tick: f32,
    pub runtime_tick_ms: u32,
    pub budgets: BudgetSummary,
    pub cell_count: u32,
    pub land_ratio: f32,
    pub mean_height: f32,
    pub mean_river_flux: f32,
    pub max_height: f32,
    pub min_height: f32,
    pub max_river_flux: f32,
}

#[derive(Serialize)]
pub(super) struct PlateStat {
    pub plate_id: u32,
    pub cell_count: u32,
    pub mean_height: f32,
    pub land_ratio: f32,
    pub mean_river_flux: f32,
}

#[derive(Serialize)]
pub(super) struct PlateStatsResponse {
    pub world_id: String,
    pub tick: f64,
    pub plate_count: u32,
    pub stats: Vec<PlateStat>,
}

#[derive(Serialize)]
pub(super) struct InterventionResult {
    pub world_id: String,
    pub applied: u32,
    pub rejected: u32,
}

#[derive(Serialize)]
pub(super) struct ForkWorldResult {
    pub source_world_id: String,
    pub world_id: String,
    pub tick: f64,
}

#[derive(Serialize)]
pub(super) struct CheckpointResult {
    pub snapshot_id: String,
    pub world_id: String,
    pub tick: f64,
}

#[derive(Serialize)]
pub(super) struct LoadCheckpointResult {
    pub source_snapshot_id: String,
    pub world_id: String,
    pub tick: f64,
}
