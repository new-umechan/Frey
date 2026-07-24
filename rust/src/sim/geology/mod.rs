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
    let Some(plate_elapsed_years) = plate_dynamics_elapsed_years(
        world.clock.epoch,
        world.clock.tick,
        world.clock.real_years_per_tick,
    ) else {
        return;
    };
    dynamics::run_geology_dynamics_step_with_state(world, geology_state, plate_elapsed_years);
}

fn plate_dynamics_elapsed_years(
    epoch: crate::sim::world::EraKind,
    tick: u64,
    years_per_tick: f32,
) -> Option<f32> {
    let interval = epoch.plate_update_interval_ticks();
    let epoch_tick = tick.saturating_sub(epoch.start_tick());
    if (epoch_tick + 1) % interval != 0 {
        return None;
    }
    Some(years_per_tick * interval as f32)
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) -> crate::sim::ErosionAutomatonBreakdown {
    surface::step_async_erosion_automaton(state, budget_cells)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cadence_tests {
    use super::plate_dynamics_elapsed_years;
    use crate::sim::world::EraKind;

    #[test]
    fn environment_updates_after_each_five_myr_window() {
        assert_eq!(
            plate_dynamics_elapsed_years(
                EraKind::Environment,
                800,
                EraKind::Environment.real_years_per_tick(),
            ),
            None
        );
        assert_eq!(
            plate_dynamics_elapsed_years(
                EraKind::Environment,
                804,
                EraKind::Environment.real_years_per_tick(),
            ),
            Some(5_000_000.0)
        );
    }

    #[test]
    fn crust_keeps_its_existing_per_tick_cadence() {
        assert_eq!(
            plate_dynamics_elapsed_years(EraKind::Crust, 0, EraKind::Crust.real_years_per_tick(),),
            Some(5_000_000.0)
        );
    }
}
