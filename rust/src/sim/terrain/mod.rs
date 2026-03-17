use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::common::geom::{
    add3, chord_distance, clamp, dot3, length3, lerp, mul3, normalize3, project_to_tangent, sub3,
};
use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::common::rng::{rng_from_seed, DeterministicRng};
use crate::{TerrainOutput, TerrainParams};

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

mod pipeline;

pub(super) fn generate(seed: &str, params: TerrainParams) -> TerrainOutput {
    pipeline::generate(seed, params)
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) -> crate::sim::ErosionAutomatonBreakdown {
    surface::step_async_erosion_automaton(state, budget_cells)
}

#[cfg(test)]
mod tests;
