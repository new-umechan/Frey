use crate::sim::terrain_types::TerrainParams;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub last_rebuild_tick: u64,
    #[serde(default)]
    pub last_sink_full_rebuild_tick: u64,
    #[serde(default)]
    pub flux_scale_ema: f32,
    #[serde(default)]
    pub last_river_driver: f32,
    #[serde(default)]
    pub prev_river_next: Vec<i32>,
    #[serde(default)]
    pub flow_heading: Vec<[f32; 3]>,
    #[serde(default)]
    pub groundwater_storage: Vec<f32>,
    #[serde(default)]
    pub scratch_effective_runoff: Vec<f32>,
    #[serde(default)]
    pub scratch_changed_mark: Vec<u8>,
    #[serde(default)]
    pub scratch_flux_samples: Vec<f32>,
    pub recent_changed: Vec<u32>,
    #[serde(default)]
    pub sink_id: Vec<i32>,
    #[serde(default)]
    pub sink_route_next: Vec<i32>,
    #[serde(default)]
    pub sink_spill_cell: Vec<i32>,
    #[serde(default)]
    pub sink_spill_to: Vec<i32>,
    #[serde(default)]
    pub sink_capacity_total: Vec<f32>,
    #[serde(default)]
    pub sink_capacity_remaining: Vec<f32>,
    #[serde(default)]
    pub sink_storage_sediment: Vec<f32>,
    #[serde(default)]
    pub sink_spill_level: Vec<f32>,
    #[serde(default)]
    pub sink_overflow_active: Vec<u8>,
    #[serde(default)]
    pub sink_dirty: Vec<u8>,
    pub params: TerrainParams,
}
