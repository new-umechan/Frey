use std::collections::BTreeMap;

use crate::domains::types::TerrainParams;
use crate::sim::world;

pub(super) const DEFAULT_HISTORY_LIMIT: usize = 512;

#[derive(Clone)]
pub(super) struct ManagedWorld {
    pub world: world::World,
    pub simulation_rate: f32,
    pub terrain_params: TerrainParams,
    pub history: BTreeMap<u64, world::World>,
}

#[derive(Clone)]
pub(super) struct SnapshotEntry {
    pub tick: u64,
    pub world: world::World,
}
