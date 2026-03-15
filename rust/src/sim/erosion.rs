use crate::domains::types::TerrainParams;
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
    pub recent_changed: Vec<u32>,
    pub params: TerrainParams,
}
