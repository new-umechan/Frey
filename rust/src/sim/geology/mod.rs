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

pub(crate) fn update_geology(
    world: &mut crate::sim::world::World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    budget: u32,
) {
    if budget == 0 {
        return;
    }
    dynamics::run_geology_dynamics_step_with_state(world, geology_state);
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) -> crate::sim::ErosionAutomatonBreakdown {
    surface::step_async_erosion_automaton(state, budget_cells)
}

#[cfg(test)]
mod tests;
