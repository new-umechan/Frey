use crate::sim::hydrology::{run_hydrology_step, HydrologyStepDetailBreakdown};
use crate::sim::world::World;

pub(super) fn run_geology_step(world: &mut World, budget: u32) {
    crate::sim::geology::update_geology(world, budget);
}

pub(super) fn run_hydrology_step_unprofiled(world: &mut World, budget: u32) {
    let _ = run_hydrology_step_profiled(world, budget);
}

pub(super) fn run_hydrology_step_profiled(
    world: &mut World,
    budget: u32,
) -> HydrologyStepDetailBreakdown {
    run_hydrology_step(world, budget)
}
