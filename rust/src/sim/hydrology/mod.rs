use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::sim;
use crate::sim::world::{EraKind, World};
use crate::GeologyParams;

use crate::sim::exec::{geology_river_budget, CRUST_RAIN_LAND, CRUST_RAIN_SEA};

pub mod surface;

mod fallback;
mod network;
mod profiling;
mod routing;
mod sync;

use fallback::run_river_fallback;
use network::{
    align_flow_heading, apply_river_network_constraints, build_river_network,
    smooth_and_normalize_flux,
};
use profiling::{profile_elapsed_ms, profile_now};
use routing::{
    apply_baseflow_storage, build_runoff_for_routing, river_rebuild_driver, should_rebuild_network,
};
use sync::{erosion_state_matches_world, sync_erosion_rain};

const NETWORK_BLEND_ALPHA: f32 = 0.38;
const FLUX_SCALE_EMA_ALPHA: f32 = 0.20;
const ACTIVE_OFF_THRESHOLD_SCALE: f32 = 0.65;
const RIVER_RUNOFF_SCALE_MM: f32 = 1_200.0;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HydrologyStepDetailBreakdown {
    pub river_prepare_ms: f64,
    pub river_automaton_ms: f64,
    pub river_automaton_sink_ms: f64,
    pub river_automaton_cell_ms: f64,
    pub river_automaton_queue_ms: f64,
    pub river_network_ms: f64,
    pub river_sync_ms: f64,
    pub river_fallback_ms: f64,
    pub network_rebuild_count: u32,
    pub fallback_count: u32,
    pub sink_rebuild_full_count: u32,
    pub sink_rebuild_partial_count: u32,
    pub sink_rebuild_skipped_count: u32,
    pub sink_rebuild_fallback_full_count: u32,
}

pub(crate) fn run_hydrology_step(
    world: &mut World,
    geology_budget: u32,
) -> HydrologyStepDetailBreakdown {
    let mut detail = HydrologyStepDetailBreakdown::default();
    let budget = geology_river_budget(world.clock.epoch, geology_budget);
    if budget == 0 {
        return detail;
    }

    let phase_start = profile_now();
    let runoff = build_runoff_for_routing(world);
    detail.river_prepare_ms += profile_elapsed_ms(phase_start);
    let river_driver = river_rebuild_driver(world);
    if !run_river_step_with_erosion_state(world, budget, &runoff, river_driver, &mut detail) {
        let phase_start = profile_now();
        run_river_fallback(world, &runoff);
        detail.river_fallback_ms += profile_elapsed_ms(phase_start);
        detail.fallback_count = detail.fallback_count.saturating_add(1);
    }
    detail
}

fn run_river_step_with_erosion_state(
    world: &mut World,
    budget: u32,
    runoff: &[f32],
    river_driver: f32,
    detail: &mut HydrologyStepDetailBreakdown,
) -> bool {
    let tick = world.clock.tick;
    let mesh_positions = &world.mesh.positions;
    let mesh_nbr_offsets = &world.mesh.nbr_offsets;
    let mesh_nbrs = &world.mesh.nbrs;
    let geology = &mut world.state.geology;
    let hydrology = &mut world.state.hydrology;
    let expected_height = geology.height.len();
    let expected_flux = hydrology.river_flow.len();
    let expected_next = hydrology.river_downstream.len();

    let Some(state) = world.runtime.hydrology_dynamics.as_mut() else {
        return false;
    };
    if !erosion_state_matches_world(state, expected_height, expected_flux, expected_next) {
        return false;
    }

    let mut effective_runoff = std::mem::take(&mut state.scratch_effective_runoff);
    let phase_start = profile_now();
    apply_baseflow_storage(
        &mut state.groundwater_storage,
        &state.params,
        &geology.height,
        mesh_nbr_offsets,
        mesh_nbrs,
        runoff,
        &mut effective_runoff,
    );
    sync_erosion_rain(state, effective_runoff.as_slice());
    detail.river_prepare_ms += profile_elapsed_ms(phase_start);
    let cell_count = expected_height as u32;
    let budget_cells = (cell_count.saturating_mul(budget).max(1) / 12).max(32);
    let phase_start = profile_now();
    let automaton_breakdown = sim::step_erosion_automaton(state, budget_cells);
    detail.river_automaton_ms += profile_elapsed_ms(phase_start);
    detail.river_automaton_sink_ms += automaton_breakdown.sink_rebuild_ms;
    detail.river_automaton_cell_ms += automaton_breakdown.cell_process_ms;
    detail.river_automaton_queue_ms += automaton_breakdown.queue_update_ms;
    detail.sink_rebuild_full_count = detail
        .sink_rebuild_full_count
        .saturating_add(automaton_breakdown.sink_rebuild_full_count);
    detail.sink_rebuild_partial_count = detail
        .sink_rebuild_partial_count
        .saturating_add(automaton_breakdown.sink_rebuild_partial_count);
    detail.sink_rebuild_skipped_count = detail
        .sink_rebuild_skipped_count
        .saturating_add(automaton_breakdown.sink_rebuild_skipped_count);
    detail.sink_rebuild_fallback_full_count = detail
        .sink_rebuild_fallback_full_count
        .saturating_add(automaton_breakdown.sink_rebuild_fallback_full_count);

    state.last_river_driver = river_driver;
    if should_rebuild_network(tick, state, river_driver) {
        let phase_start = profile_now();
        let (mut rebuilt_flux, mut rebuilt_next, mut rebuilt_heading) = build_river_network(
            mesh_positions,
            mesh_nbr_offsets,
            mesh_nbrs,
            &state.height,
            effective_runoff.as_slice(),
            &state.params,
            Some(&*state),
        );
        smooth_and_normalize_flux(
            &mut rebuilt_flux,
            &state.river_flux,
            &mut state.flux_scale_ema,
            &mut state.scratch_flux_samples,
        );
        apply_river_network_constraints(
            &state.height,
            &mut rebuilt_flux,
            &mut rebuilt_next,
            &state.river_flux,
            state.params.river_accumulation_threshold,
        );
        align_flow_heading(mesh_positions, &mut rebuilt_heading, &rebuilt_next);
        state.prev_river_next.clone_from(&state.river_next);
        state.river_flux = rebuilt_flux;
        state.river_next = rebuilt_next;
        state.flow_heading = rebuilt_heading;
        state.last_rebuild_tick = tick;
        detail.river_network_ms += profile_elapsed_ms(phase_start);
        detail.network_rebuild_count = detail.network_rebuild_count.saturating_add(1);
    }
    state.scratch_effective_runoff = effective_runoff;

    let phase_start = profile_now();
    geology.height.clone_from(&state.height);
    hydrology.river_flow.clone_from(&state.river_flux);
    hydrology.river_downstream.clone_from(&state.river_next);
    hydrology.river_upstream.fill(-1);
    for (cell, &next) in hydrology.river_downstream.iter().enumerate() {
        if next >= 0 {
            let next_i = next as usize;
            if next_i < hydrology.river_upstream.len() {
                hydrology.river_upstream[next_i] = cell as i32;
            }
        }
    }
    world.state.geology.erosion_rate.fill(0.0);
    world.state.geology.deposition_rate.fill(0.0);
    for i in 0..hydrology.river_transport_cost.len() {
        hydrology.river_transport_cost[i] = 1.0 / (1.0 + hydrology.river_flow[i].sqrt());
    }
    detail.river_sync_ms += profile_elapsed_ms(phase_start);
    true
}

#[cfg(test)]
pub(crate) fn route_river_flux(height: &[f32], river_next: &[i32], runoff: &[f32]) -> Vec<f32> {
    network::route_river_flux(height, river_next, runoff)
}
