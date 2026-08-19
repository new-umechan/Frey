use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::common::geom::{
    add3, chord_distance, clamp, dot3, length3, lerp, mul3, normalize3, project_to_tangent, sub3,
};
use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::common::rng::{rng_from_seed, rng_from_seed_label, DeterministicRng};
use crate::{GeologyOutput, GeologyParams};

mod types;
use types::*;

mod noise;
use noise::*;

mod plates;
use plates::*;

mod boundaries;
use boundaries::*;

mod surface;
use surface::*;

pub mod dynamics;
mod pipeline;

pub(super) fn generate(seed: &str, params: GeologyParams) -> GeologyOutput {
    pipeline::generate(seed, params)
}

pub(super) fn generate_with_mesh(
    seed: &str,
    params: GeologyParams,
) -> (GeologyOutput, Vec<[f32; 3]>, Vec<u32>, Vec<u32>) {
    pipeline::generate_with_mesh(seed, params)
}

pub(super) fn diagnose_plate_emergence(
    seed: &str,
    params: GeologyParams,
) -> crate::sim::geology_types::PlateEmergenceDiagnostics {
    diagnose_plate_emergence_with_override(seed, params, None)
}

pub(super) fn diagnose_plate_emergence_with_override(
    seed: &str,
    params: GeologyParams,
    min_region_override: Option<usize>,
) -> crate::sim::geology_types::PlateEmergenceDiagnostics {
    let (positions, indices) = generate_icosphere(params.level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let spherical = compute_spherical_coords(&positions);
    let mut pre_plate_rng = rng_from_seed_label(seed, "damage-first-pre-plate");
    let mut pre_plate_phi = evaluate_phi(
        &spherical,
        params.harmonic_max_l,
        params.spectral_alpha,
        &mut pre_plate_rng,
    );
    normalize_zscore(&mut pre_plate_phi);
    plates::diagnose_plate_emergence_with_mesh(
        seed,
        &positions,
        &nbr_offsets,
        &nbrs,
        &pre_plate_phi,
        &params,
        min_region_override,
        &mut pre_plate_rng,
    )
}

pub(crate) fn update_geology(
    world: &mut crate::sim::world::World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    budget: u32,
) {
    if budget == 0 {
        return;
    }
    dynamics::run_geology_dynamics_step_with_state(
        world,
        geology_state,
        world.clock.real_years_per_tick,
    );
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) -> crate::sim::ErosionAutomatonBreakdown {
    surface::step_async_erosion_automaton(state, budget_cells)
}

#[cfg(test)]
mod tests;
