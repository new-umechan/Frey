use std::collections::BTreeMap;

use crate::domains::types::TerrainParams;
use crate::sim::world;
use crate::sim::erosion::ErosionAutomatonState;

const EROSION_RAIN_SCALE_MM: f32 = 1_200.0;

pub(super) fn build_erosion_state(
    world: &world::World,
    params: TerrainParams,
) -> ErosionAutomatonState {
    let cell_count = world.state.geology.height.len();
    ErosionAutomatonState {
        positions: world.mesh.positions.clone(),
        nbr_offsets: world.mesh.nbr_offsets.clone(),
        nbrs: world.mesh.nbrs.clone(),
        height: world.state.geology.height.clone(),
        water: vec![0.0; cell_count],
        sediment: vec![0.0; cell_count],
        armor: vec![0.0; cell_count],
        rain: world
            .state
            .climate
            .runoff
            .iter()
            .copied()
            .map(|value| (value.max(0.0) / EROSION_RAIN_SCALE_MM).clamp(0.0, 1.0))
            .collect(),
        river_flux: world.state.geology.river_flux.clone(),
        river_next: world.state.geology.river_next.clone(),
        active_queue: (0..cell_count as u32).collect(),
        active_head: 0,
        in_queue: vec![1; cell_count],
        rain_cursor: 0,
        tick: world.exec.tick,
        recent_changed: Vec::new(),
        params,
    }
}

pub(super) fn sync_erosion_state(world: &mut world::World, params: &TerrainParams) {
    let state = build_erosion_state(world, params.clone());
    let _ = world.attach_river_erosion_state(state);
}

pub(super) fn trim_history(history: &mut BTreeMap<u64, world::World>, max_entries: usize) {
    while history.len() > max_entries {
        if let Some(oldest) = history.keys().next().copied() {
            history.remove(&oldest);
        } else {
            break;
        }
    }
}

pub(super) fn sampled_len(total_len: usize, stride: u32) -> u32 {
    if total_len == 0 {
        return 0;
    }
    let step = stride.max(1) as usize;
    total_len.div_ceil(step) as u32
}

pub(super) fn sample_f32(values: &[f32], stride: u32) -> Vec<f32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .copied()
        .collect()
}

pub(super) fn sample_u32_from_u16(values: &[u16], stride: u32) -> Vec<u32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .map(|&v| v as u32)
        .collect()
}

pub(super) fn sample_i32(values: &[i32], stride: u32) -> Vec<i32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .copied()
        .collect()
}

pub(super) fn apply_f32(values: &mut [f32], index: usize, value: f32) -> bool {
    if index >= values.len() || !value.is_finite() {
        return false;
    }
    values[index] = value;
    true
}

pub(super) fn apply_i32(values: &mut [i32], index: usize, value: i32) -> bool {
    if index >= values.len() {
        return false;
    }
    values[index] = value;
    true
}

pub(super) fn apply_u16(values: &mut [u16], index: usize, value: u16) -> bool {
    if index >= values.len() {
        return false;
    }
    values[index] = value;
    true
}
