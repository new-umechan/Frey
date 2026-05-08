use smallvec::SmallVec;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::application::world_support::ensure_sink_buffers;
use crate::sim;
use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::world::{EraKind, World};
use crate::GeologyParams;

use crate::sim::exec::{geology_river_budget, CRUST_RAIN_LAND, CRUST_RAIN_SEA};

pub mod surface;

mod fallback;
mod fill_spill;
mod network;
mod profiling;
mod routing;
mod sync;

use fallback::run_river_fallback;
use fill_spill::{
    rebuild_fill_spill_state, refresh_fill_spill_storage_and_lakes, should_rebuild_fill_spill,
    sync_fill_spill_from_erosion, update_public_lake_flags, FillSpillRebuildMode,
};
use network::{
    align_flow_heading, apply_river_network_constraints, build_river_network,
    smooth_and_normalize_flux, RiverNetworkConstraintBuffers, RiverNetworkConstraintInput,
};
use profiling::{profile_elapsed_ms, profile_now};
use routing::{apply_baseflow_storage, build_runoff_for_routing, should_rebuild_network};
pub(crate) use sync::sync_erosion_height;
use sync::{erosion_state_matches_world, sync_erosion_rain};

pub(crate) use fill_spill::apply_fill_spill_sink_rule_to_erosion_cell;
pub(crate) use fill_spill::sync_fill_spill_to_erosion;

const NETWORK_BLEND_ALPHA: f32 = 0.38;
const FLUX_SCALE_EMA_ALPHA: f32 = 0.20;
const ACTIVE_OFF_THRESHOLD_SCALE: f32 = 0.65;

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
    pub sink_incremental_rebuild_ms: f64,
    pub sink_full_rebuild_ms: f64,
    pub sink_affected_ratio: f64,
    pub sink_validation_fail_count: u32,
}

pub(crate) fn run_hydrology_step(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    geology_budget: u32,
    geology_state: Option<&crate::sim::world::GeologyDynamicsState>,
) -> HydrologyStepDetailBreakdown {
    let mut detail = HydrologyStepDetailBreakdown::default();
    let budget = geology_river_budget(world.clock.epoch, geology_budget);
    if budget == 0 {
        return detail;
    }

    let phase_start = profile_now();
    let runoff = build_runoff_for_routing(world);
    detail.river_prepare_ms += profile_elapsed_ms(phase_start);
    let river_driver = routing::river_rebuild_driver_with_geology(world, geology_state);
    if !run_river_step_with_erosion_state(
        world,
        hydrology_state.as_mut(),
        budget,
        &runoff,
        river_driver,
        &mut detail,
    ) {
        let phase_start = profile_now();
        run_river_fallback(world, &runoff, hydrology_state.as_mut());
        world.state.hydrology.erosion_rate.fill(0.0);
        world.state.hydrology.deposition_rate.fill(0.0);
        detail.river_fallback_ms += profile_elapsed_ms(phase_start);
        detail.fallback_count = detail.fallback_count.saturating_add(1);
    }
    detail
}

pub fn apply_hydrology_state_view(
    world: &mut World,
    state: &ErosionAutomatonState,
) -> Result<(), String> {
    {
        let mut cells = world.cell_store_mut();
        cells.apply_hydrology_view(state)?;
    }
    world.refresh_terrain_state();
    let mesh_nbr_offsets = world.mesh().nbr_offsets.clone();
    let mesh_nbrs = world.mesh().nbrs.clone();
    rebuild_fill_spill_state(
        &mut world.state.hydrology,
        &world.state.geology.height,
        &mesh_nbr_offsets,
        &mesh_nbrs,
        &world.control.geology_params,
        Some(&state.water),
        Some(&state.sediment),
    );
    rebuild_mfd_from_primary(&mut world.state.hydrology);
    update_public_lake_flags(
        &mut world.state.hydrology,
        &world.state.geology.height,
        &world.control.geology_params,
    );
    Ok(())
}

pub fn sync_hydrology_state_for_headless_runner(
    world: &mut World,
    state: &mut ErosionAutomatonState,
    params: &GeologyParams,
) {
    let expected = world.cell_store().len();
    if !erosion_state_matches_world(state, expected, expected, expected) {
        *state = crate::sim::build_hydrology_state_for_bench(world, params.clone());
        return;
    }
    state.tick = world.clock.tick;
    state.last_river_driver = 1.0;
    state.params = params.clone();
    state.recent_changed.clear();
    ensure_hydrology_mfd_for_headless_runner(&mut world.state.hydrology);
    ensure_sink_buffers(state, expected);
    sync_fill_spill_to_erosion(state, &world.state.hydrology);
}

fn ensure_hydrology_mfd_for_headless_runner(hydrology: &mut crate::sim::world::HydrologyState) {
    let expected = hydrology.river_next.len();
    if hydrology.river_downstream.len() != expected {
        rebuild_mfd_from_primary(hydrology);
    }
}

pub(crate) fn run_hydrology_flow_step(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
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
    if !run_river_flow_only_with_state(world, hydrology_state.as_mut(), &runoff, &mut detail) {
        let phase_start = profile_now();
        run_river_fallback(world, &runoff, hydrology_state.as_mut());
        world.state.hydrology.erosion_rate.fill(0.0);
        world.state.hydrology.deposition_rate.fill(0.0);
        detail.river_fallback_ms += profile_elapsed_ms(phase_start);
        detail.fallback_count = detail.fallback_count.saturating_add(1);
    }
    detail
}

pub(crate) fn run_hydrology_step_with_existing_state(
    world: &mut World,
    hydrology_state: &mut ErosionAutomatonState,
    geology_budget: u32,
    run_mfd: bool,
) -> HydrologyStepDetailBreakdown {
    let mut state = Some(hydrology_state.clone());
    let detail = if run_mfd {
        run_hydrology_step(world, &mut state, geology_budget, None)
    } else {
        run_hydrology_flow_step(world, &mut state, geology_budget)
    };
    if let Some(updated) = state {
        *hydrology_state = updated;
    }
    detail
}

fn run_river_step_with_erosion_state(
    world: &mut World,
    state: Option<&mut ErosionAutomatonState>,
    budget: u32,
    runoff: &[f32],
    river_driver: f32,
    detail: &mut HydrologyStepDetailBreakdown,
) -> bool {
    let tick = world.clock.tick;
    let geology = &mut world.state.geology;
    let hydrology = &mut world.state.hydrology;
    let expected_height = geology.height.len();
    let expected_flux = hydrology.river_flow.len();
    let expected_next = hydrology.river_next.len();

    let Some(state) = state else {
        return false;
    };
    if !erosion_state_matches_world(state, expected_height, expected_flux, expected_next) {
        return false;
    }
    if state.height.len() == geology.height.len() {
        state.height.copy_from_slice(&geology.height);
    } else {
        state.height.clone_from(&geology.height);
    }

    sync_fill_spill_to_erosion(state, hydrology);

    let mut effective_runoff = std::mem::take(&mut state.scratch_effective_runoff);
    let phase_start = profile_now();
    apply_baseflow_storage(
        &mut state.groundwater_storage,
        &state.params,
        &geology.height,
        &state.nbr_offsets,
        &state.nbrs,
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
    sync_fill_spill_from_erosion(hydrology, &state.height, &state.params, state);
    match should_rebuild_fill_spill(hydrology, state) {
        FillSpillRebuildMode::Full { validation_failed } => {
            let phase_start = profile_now();
            rebuild_fill_spill_state(
                hydrology,
                &state.height,
                &state.nbr_offsets,
                &state.nbrs,
                &state.params,
                Some(&state.water),
                Some(&state.sediment),
            );
            state.last_sink_full_rebuild_tick = state.tick;
            let elapsed = profile_elapsed_ms(phase_start);
            detail.river_automaton_sink_ms += elapsed;
            detail.sink_full_rebuild_ms += elapsed;
            detail.sink_rebuild_full_count = detail.sink_rebuild_full_count.saturating_add(1);
            if validation_failed {
                detail.sink_validation_fail_count =
                    detail.sink_validation_fail_count.saturating_add(1);
            }
        }
        FillSpillRebuildMode::Incremental => {
            let phase_start = profile_now();
            let affected = fill_spill::rebuild_fill_spill_state_incremental(
                hydrology,
                &state.height,
                &state.nbr_offsets,
                &state.nbrs,
                &state.params,
                &state.recent_changed,
                state.params.sink_incremental_neighbor_hops,
            );
            let elapsed = profile_elapsed_ms(phase_start);
            detail.river_automaton_sink_ms += elapsed;
            detail.sink_incremental_rebuild_ms += elapsed;
            detail.sink_rebuild_partial_count = detail.sink_rebuild_partial_count.saturating_add(1);
            if !state.height.is_empty() {
                detail.sink_affected_ratio += (affected as f64) / (state.height.len() as f64);
            }
            refresh_fill_spill_storage_and_lakes(
                hydrology,
                &state.height,
                Some(&state.water),
                Some(&state.sediment),
                &state.params,
            );
        }
        FillSpillRebuildMode::Skip => {
            refresh_fill_spill_storage_and_lakes(
                hydrology,
                &state.height,
                Some(&state.water),
                Some(&state.sediment),
                &state.params,
            );
            detail.sink_rebuild_skipped_count = detail.sink_rebuild_skipped_count.saturating_add(1);
        }
    }

    state.last_river_driver = river_driver;
    if should_rebuild_network(tick, state, river_driver) {
        let phase_start = profile_now();
        let mut rebuilt = build_river_network(
            &state.positions,
            &state.nbr_offsets,
            &state.nbrs,
            &state.height,
            effective_runoff.as_slice(),
            &state.params,
            Some(&*state),
        );
        // 正規化前の flux を保持（river_flow 用）
        let raw_flux = rebuilt.flux.clone();

        smooth_and_normalize_flux(
            &mut rebuilt.flux,
            &state.river_flux,
            &mut state.flux_scale_ema,
            &mut state.scratch_flux_samples,
        );
        let mut constraint_buffers = RiverNetworkConstraintBuffers {
            flux: &mut rebuilt.flux,
            primary_next: &mut rebuilt.primary_next,
            downstream_offsets: &mut rebuilt.downstream_offsets,
            downstream_cells: &mut rebuilt.downstream_cells,
            downstream_weights: &mut rebuilt.downstream_weights,
        };
        apply_river_network_constraints(
            RiverNetworkConstraintInput {
                height: &state.height,
                previous_flux: &state.river_flux,
                accumulation_threshold: state.params.river_accumulation_threshold,
            },
            &mut constraint_buffers,
        );
        sanitize_primary_next_no_cycle(&mut rebuilt.primary_next);
        align_flow_heading(
            &state.positions,
            &mut rebuilt.heading,
            &rebuilt.primary_next,
        );
        state.prev_river_next.clone_from(&state.river_next);
        state.river_flux = rebuilt.flux; // 正規化済み（内部処理用）
        state.river_next = rebuilt.primary_next;
        state.flow_heading = rebuilt.heading;
        state.last_rebuild_tick = tick;
        detail.river_network_ms += profile_elapsed_ms(phase_start);
        detail.network_rebuild_count = detail.network_rebuild_count.saturating_add(1);

        hydrology.river_downstream.clone_from(&downstream_from_csr(
            hydrology.river_next.len(),
            &rebuilt.downstream_offsets,
            &rebuilt.downstream_cells,
            &rebuilt.downstream_weights,
        ));

        // raw_flux を state に保存（river_flow 用）
        state.raw_river_flux = raw_flux;
    } else {
        if hydrology.river_downstream.len() != hydrology.river_next.len() {
            rebuild_mfd_from_primary(hydrology);
        }
    }
    state.scratch_effective_runoff = effective_runoff;

    let phase_start = profile_now();
    update_erosion_and_deposition_rates(geology, hydrology, &state.height);
    // raw_river_flux（正規化前）を river_flow として使用
    hydrology.river_flow.clone_from(&state.raw_river_flux);
    hydrology.river_next.clone_from(&state.river_next);
    if hydrology.river_downstream.len() != hydrology.river_next.len() {
        rebuild_mfd_from_primary(hydrology);
    }
    update_public_lake_flags(hydrology, &state.height, &state.params);
    for i in 0..hydrology.river_transport_cost.len() {
        hydrology.river_transport_cost[i] = 1.0 / (1.0 + hydrology.river_flow[i].sqrt());
    }
    update_surface_water_access(hydrology);
    detail.river_sync_ms += profile_elapsed_ms(phase_start);
    true
}

fn run_river_flow_only_with_state(
    world: &mut World,
    state: Option<&mut ErosionAutomatonState>,
    runoff: &[f32],
    detail: &mut HydrologyStepDetailBreakdown,
) -> bool {
    let geology = &mut world.state.geology;
    let hydrology = &mut world.state.hydrology;
    let expected_height = geology.height.len();
    let expected_flux = hydrology.river_flow.len();
    let expected_next = hydrology.river_next.len();

    let Some(state) = state else {
        return false;
    };
    if !erosion_state_matches_world(state, expected_height, expected_flux, expected_next) {
        return false;
    }
    state.height.clone_from(&geology.height);

    match should_rebuild_fill_spill(hydrology, state) {
        FillSpillRebuildMode::Full { validation_failed } => {
            let phase_start = profile_now();
            rebuild_fill_spill_state(
                hydrology,
                &state.height,
                &state.nbr_offsets,
                &state.nbrs,
                &state.params,
                Some(&state.water),
                Some(&state.sediment),
            );
            state.last_sink_full_rebuild_tick = state.tick;
            detail.sink_full_rebuild_ms += profile_elapsed_ms(phase_start);
            detail.sink_rebuild_full_count = detail.sink_rebuild_full_count.saturating_add(1);
            if validation_failed {
                detail.sink_validation_fail_count =
                    detail.sink_validation_fail_count.saturating_add(1);
            }
        }
        FillSpillRebuildMode::Incremental => {
            let phase_start = profile_now();
            let affected = fill_spill::rebuild_fill_spill_state_incremental(
                hydrology,
                &state.height,
                &state.nbr_offsets,
                &state.nbrs,
                &state.params,
                &state.recent_changed,
                state.params.sink_incremental_neighbor_hops,
            );
            detail.sink_incremental_rebuild_ms += profile_elapsed_ms(phase_start);
            detail.sink_rebuild_partial_count = detail.sink_rebuild_partial_count.saturating_add(1);
            if !state.height.is_empty() {
                detail.sink_affected_ratio += (affected as f64) / (state.height.len() as f64);
            }
            refresh_fill_spill_storage_and_lakes(
                hydrology,
                &state.height,
                Some(&state.water),
                Some(&state.sediment),
                &state.params,
            );
        }
        FillSpillRebuildMode::Skip => {
            refresh_fill_spill_storage_and_lakes(
                hydrology,
                &state.height,
                Some(&state.water),
                Some(&state.sediment),
                &state.params,
            );
            detail.sink_rebuild_skipped_count = detail.sink_rebuild_skipped_count.saturating_add(1);
        }
    }
    sync_fill_spill_to_erosion(state, hydrology);

    let mut effective_runoff = std::mem::take(&mut state.scratch_effective_runoff);
    let phase_start = profile_now();
    apply_baseflow_storage(
        &mut state.groundwater_storage,
        &state.params,
        &geology.height,
        &state.nbr_offsets,
        &state.nbrs,
        runoff,
        &mut effective_runoff,
    );
    sync_erosion_rain(state, effective_runoff.as_slice());
    detail.river_prepare_ms += profile_elapsed_ms(phase_start);

    let phase_start = profile_now();
    sanitize_primary_next_no_cycle(&mut state.river_next);
    if hydrology.river_downstream.len() != hydrology.river_next.len() {
        rebuild_mfd_from_primary(hydrology);
    }
    let raw_flux = flow_flux_on_downstream_network(
        &geology.height,
        hydrology.river_downstream.as_slice(),
        &effective_runoff,
    );
    let mut normalized_flux = raw_flux.clone();
    smooth_and_normalize_flux(
        &mut normalized_flux,
        &state.river_flux,
        &mut state.flux_scale_ema,
        &mut state.scratch_flux_samples,
    );
    detail.river_network_ms += profile_elapsed_ms(phase_start);

    state.prev_river_next.clone_from(&state.river_next);
    state.raw_river_flux = raw_flux;
    state.river_flux = normalized_flux;
    state.scratch_effective_runoff = effective_runoff;
    state.last_river_driver = 0.0;
    sync_fill_spill_from_erosion(hydrology, &state.height, &state.params, state);
    refresh_fill_spill_storage_and_lakes(
        hydrology,
        &state.height,
        Some(&state.water),
        Some(&state.sediment),
        &state.params,
    );

    let phase_start = profile_now();
    hydrology.river_flow.clone_from(&state.raw_river_flux);
    hydrology.river_next.clone_from(&state.river_next);
    if hydrology.river_downstream.len() != hydrology.river_next.len() {
        rebuild_mfd_from_primary(hydrology);
    }
    update_public_lake_flags(hydrology, &state.height, &state.params);
    hydrology.erosion_rate.fill(0.0);
    hydrology.deposition_rate.fill(0.0);
    for i in 0..hydrology.river_transport_cost.len() {
        hydrology.river_transport_cost[i] = 1.0 / (1.0 + hydrology.river_flow[i].sqrt());
    }
    update_surface_water_access(hydrology);
    detail.river_sync_ms += profile_elapsed_ms(phase_start);
    true
}

fn flow_flux_on_downstream_network(
    height: &[f32],
    river_downstream: &[SmallVec<[(u32, f32); 4]>],
    runoff: &[f32],
) -> Vec<f32> {
    let count = height.len().min(river_downstream.len()).min(runoff.len());
    let mut order = (0..count).collect::<Vec<_>>();
    order.sort_unstable_by(|a, b| {
        height[*b]
            .partial_cmp(&height[*a])
            .unwrap_or(Ordering::Equal)
    });

    let mut flux = vec![0.0; count];
    for i in 0..count {
        if height[i] > 0.0 {
            flux[i] = runoff[i].max(0.0);
        }
    }
    for &i in &order {
        if height[i] <= 0.0 {
            flux[i] = 0.0;
            continue;
        }
        for &(target, weight) in &river_downstream[i] {
            if weight <= 0.0 {
                continue;
            }
            let n = target as usize;
            if n < count {
                flux[n] += flux[i] * weight;
            }
        }
    }
    flux
}

fn update_erosion_and_deposition_rates(
    geology: &crate::sim::world::GeologyState,
    hydrology: &mut crate::sim::world::HydrologyState,
    next_height: &[f32],
) {
    let count = geology
        .height
        .len()
        .min(hydrology.erosion_rate.len())
        .min(hydrology.deposition_rate.len())
        .min(next_height.len());
    hydrology.erosion_rate.fill(0.0);
    hydrology.deposition_rate.fill(0.0);
    for (i, &next_h) in next_height.iter().enumerate().take(count) {
        let delta = next_h - geology.height[i];
        if delta >= 0.0 {
            hydrology.deposition_rate[i] = delta;
        } else {
            hydrology.erosion_rate[i] = -delta;
        }
    }
}

fn update_surface_water_access(hydrology: &mut crate::sim::world::HydrologyState) {
    let max_flow = hydrology
        .river_flow
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    let count = hydrology
        .surface_water_access
        .len()
        .min(hydrology.river_flow.len())
        .min(hydrology.is_lake.len());
    for i in 0..count {
        let flow = (hydrology.river_flow[i] / max_flow).clamp(0.0, 1.0);
        let lake_bonus = if hydrology.is_lake[i] { 0.2 } else { 0.0 };
        hydrology.surface_water_access[i] = (flow + lake_bonus).clamp(0.0, 1.0);
    }
}

pub(crate) fn rebuild_mfd_from_primary(hydrology: &mut crate::sim::world::HydrologyState) {
    let cell_count = hydrology.river_next.len();
    hydrology.river_downstream = vec![SmallVec::new(); cell_count];
    for (cell, &next) in hydrology.river_next.iter().enumerate() {
        if next >= 0 {
            hydrology.river_downstream[cell].push((next as u32, 1.0));
        }
    }
}

pub(crate) fn downstream_from_csr(
    cell_count: usize,
    offsets: &[u32],
    cells: &[u32],
    weights: &[f32],
) -> Vec<SmallVec<[(u32, f32); 4]>> {
    let mut result = vec![SmallVec::new(); cell_count];
    if offsets.len() != cell_count + 1 || cells.len() != weights.len() {
        return result;
    }
    for i in 0..cell_count {
        let start = offsets[i] as usize;
        let end = offsets[i + 1] as usize;
        if start >= end || end > cells.len() {
            continue;
        }
        for idx in start..end {
            result[i].push((cells[idx], weights[idx]));
        }
    }
    result
}

pub(super) fn sanitize_primary_next_no_cycle(river_next: &mut [i32]) {
    let count = river_next.len();
    if count == 0 {
        return;
    }

    for next in river_next.iter_mut() {
        if *next >= 0 && (*next as usize) >= count {
            *next = -1;
        }
    }

    let mut visit_state = vec![0u8; count];
    let mut path = Vec::<usize>::with_capacity(32);
    for start in 0..count {
        if visit_state[start] != 0 {
            continue;
        }
        let mut node = start as i32;
        while node >= 0 {
            let idx = node as usize;
            if idx >= count {
                break;
            }
            match visit_state[idx] {
                0 => {
                    visit_state[idx] = 1;
                    path.push(idx);
                    node = river_next[idx];
                }
                1 => {
                    river_next[idx] = -1;
                    break;
                }
                _ => break,
            }
        }
        for &idx in &path {
            visit_state[idx] = 2;
        }
        path.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::{GeologyState, World, WorldMesh};
    use crate::PlateId;

    fn build_test_world() -> World {
        World::new(
            WorldMesh {
                positions: vec![
                    [0.0, 0.0, 1.0],
                    [0.2, 0.0, 0.98],
                    [0.0, 0.2, 0.98],
                    [-0.2, 0.0, 0.98],
                ],
                nbr_offsets: vec![0, 3, 5, 7, 8],
                nbrs: vec![1, 2, 3, 0, 2, 0, 1, 0],
            },
            GeologyState {
                height: vec![1.2, 0.9, 0.6, -0.2],
                lake_depth: vec![0.0; 4],
                plate_id: vec![PlateId(0); 4],
                volcanism: vec![0.0; 4],
                vertex_buoyancy: vec![0.0; 4],
                geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
                boundary_condition: vec![0.0; 4],
                smoothing_limited_cells_ratio: 0.0,
                mean_smoothing_factor: 1.0,
                zero_mean_adjusted_cells_ratio: 0.0,
                zero_mean_mean_abs_correction: 0.0,
                zero_mean_std_delta: 0.0,
            },
        )
    }

    #[test]
    fn sanitize_primary_next_breaks_cycle_into_sink() {
        let mut river_next = vec![1, 2, 0, -1];

        sanitize_primary_next_no_cycle(&mut river_next);

        assert_eq!(river_next, vec![-1, 2, 0, -1]);
    }

    #[test]
    fn fallback_keeps_downstream_and_primary_consistent() {
        let mut world = build_test_world();
        let runoff = vec![1.0, 0.8, 0.6, 0.0];

        fallback::run_river_fallback(&mut world, &runoff, None);

        assert_eq!(
            world.state.hydrology.river_downstream.len(),
            world.state.hydrology.river_next.len()
        );
        for (cell, downstream) in world.state.hydrology.river_downstream.iter().enumerate() {
            assert!(
                downstream.len() <= 4,
                "cell {cell} exceeds max branch count"
            );
            let next = world.state.hydrology.river_next[cell];
            if next < 0 {
                assert!(downstream.is_empty(), "cell {cell} should terminate");
                continue;
            }
            let max_weight_target = downstream
                .iter()
                .copied()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
                .map(|(target, _)| target as i32)
                .unwrap_or(-1);
            assert_eq!(max_weight_target, next, "cell {cell} target mismatch");
        }
    }
}
