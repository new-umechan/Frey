use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::geology_types::{GeologyParams, PlateId};
use crate::sim::hydrology::rebuild_mfd_from_primary;
use crate::sim::world;

use super::state::ManagedWorld;

const EROSION_RAIN_SCALE_MM: f32 = 1_200.0;

pub(super) fn build_erosion_state(
    world: &world::World,
    params: GeologyParams,
) -> ErosionAutomatonState {
    let cells = world.cell_store();
    let cell_count = cells.len();
    let mesh = world.mesh();
    ErosionAutomatonState {
        positions: mesh.positions.clone(),
        nbr_offsets: mesh.nbr_offsets.clone(),
        nbrs: mesh.nbrs.clone(),
        height: cells.height.to_vec(),
        water: vec![0.0; cell_count],
        sediment: vec![0.0; cell_count],
        armor: vec![0.0; cell_count],
        rain: cells
            .runoff
            .iter()
            .copied()
            .map(|value| (value.max(0.0) / EROSION_RAIN_SCALE_MM).clamp(0.0, 1.0))
            .collect(),
        river_flux: cells.river_flow.to_vec(),
        raw_river_flux: Vec::new(),
        river_next: cells.river_next.to_vec(),
        active_queue: (0..cell_count as u32).collect(),
        active_head: 0,
        in_queue: vec![1; cell_count],
        rain_cursor: 0,
        tick: world.clock.tick,
        last_rebuild_tick: world.clock.tick.saturating_sub(1),
        last_sink_full_rebuild_tick: world.clock.tick.saturating_sub(8),
        flux_scale_ema: 1.0,
        last_river_driver: 1.0,
        prev_river_next: cells.river_next.to_vec(),
        flow_heading: vec![[0.0, 0.0, 0.0]; cell_count],
        groundwater_storage: vec![0.0; cell_count],
        scratch_effective_runoff: vec![0.0; cell_count],
        scratch_changed_mark: vec![0; cell_count],
        scratch_flux_samples: Vec::with_capacity(cell_count / 2),
        recent_changed: Vec::new(),
        sink_id: vec![-1; cell_count],
        sink_route_next: vec![-1; cell_count],
        sink_spill_cell: Vec::new(),
        sink_spill_to: Vec::new(),
        sink_capacity_total: Vec::new(),
        sink_capacity_remaining: Vec::new(),
        sink_storage_sediment: Vec::new(),
        sink_spill_level: Vec::new(),
        sink_overflow_active: Vec::new(),
        sink_dirty: vec![1; cell_count],
        params,
    }
}

pub(super) fn sync_erosion_state(managed: &mut ManagedWorld) {
    sync_erosion_state_full(managed);
}

pub(super) fn sync_erosion_state_full(managed: &mut ManagedWorld) {
    let params = managed.geology_params.clone();
    let world = &mut managed.world;
    let hydrology_dynamics = &mut managed.hydrology_dynamics;
    let cells = world.cell_store();
    let expected = cells.len();
    let Some(state) = hydrology_dynamics.as_mut() else {
        *hydrology_dynamics = Some(build_erosion_state(world, params));
        return;
    };
    if !erosion_state_shape_matches(state, expected) {
        *hydrology_dynamics = Some(build_erosion_state(world, params));
        return;
    }
    state.height.clone_from_slice(cells.height);
    state.river_flux.clone_from_slice(cells.river_flow);
    state.prev_river_next.clone_from_slice(cells.river_next);
    state.river_next.clone_from_slice(cells.river_next);
    for (rain, runoff) in state.rain.iter_mut().zip(cells.runoff.iter().copied()) {
        *rain = (runoff.max(0.0) / EROSION_RAIN_SCALE_MM).clamp(0.0, 1.0);
    }
    state.tick = world.clock.tick;
    state.last_river_driver = 1.0;
    state.params = params;
    state.recent_changed.clear();
    ensure_hydrology_mfd(&mut world.state.hydrology);
    ensure_sink_buffers(state, expected);
}

pub(super) fn post_step_sync_light(managed: &mut ManagedWorld) {
    let params = managed.geology_params.clone();
    let world = &mut managed.world;
    let hydrology_dynamics = &mut managed.hydrology_dynamics;
    let expected = world.cell_store().len();
    let Some(state) = hydrology_dynamics.as_mut() else {
        *hydrology_dynamics = Some(build_erosion_state(world, params));
        return;
    };
    if !erosion_state_shape_matches(state, expected) {
        *hydrology_dynamics = Some(build_erosion_state(world, params));
        return;
    }
    state.tick = world.clock.tick;
    state.last_river_driver = 1.0;
    state.params = params;
    state.recent_changed.clear();
    ensure_hydrology_mfd(&mut world.state.hydrology);
    ensure_sink_buffers(state, expected);
}

fn ensure_hydrology_mfd(hydrology: &mut world::HydrologyState) {
    let expected = hydrology.river_next.len();
    if hydrology.river_downstream.len() != expected {
        rebuild_mfd_from_primary(hydrology);
    }
}

fn erosion_state_shape_matches(state: &ErosionAutomatonState, expected: usize) -> bool {
    state.height.len() == expected
        && state.river_flux.len() == expected
        && state.river_next.len() == expected
        && state.rain.len() == expected
        && state.prev_river_next.len() == expected
        && state.flow_heading.len() == expected
        && state.groundwater_storage.len() == expected
        && state.scratch_effective_runoff.len() == expected
        && state.scratch_changed_mark.len() == expected
}

fn ensure_sink_buffers(state: &mut ErosionAutomatonState, expected: usize) {
    if state.sink_id.len() != expected {
        state.sink_id = vec![-1; expected];
    }
    if state.sink_route_next.len() != expected {
        state.sink_route_next = vec![-1; expected];
    }
    if state.sink_dirty.len() != expected {
        state.sink_dirty = vec![1; expected];
    } else {
        state.sink_dirty.fill(1);
    }
    if state.flow_heading.len() != expected {
        state.flow_heading = vec![[0.0, 0.0, 0.0]; expected];
    }
    if state.groundwater_storage.len() != expected {
        state.groundwater_storage = vec![0.0; expected];
    }
    if state.scratch_effective_runoff.len() != expected {
        state.scratch_effective_runoff = vec![0.0; expected];
    }
    if state.scratch_changed_mark.len() != expected {
        state.scratch_changed_mark = vec![0; expected];
    }
    if state.scratch_flux_samples.capacity() < expected / 2 {
        state
            .scratch_flux_samples
            .reserve((expected / 2).saturating_sub(state.scratch_flux_samples.capacity()));
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

pub(super) fn sample_u32_from_plate_id(values: &[PlateId], stride: u32) -> Vec<u32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .map(|&v| v.as_u32())
        .collect()
}

pub(super) fn sample_i32(values: &[i32], stride: u32) -> Vec<i32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .copied()
        .collect()
}
