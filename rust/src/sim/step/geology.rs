use super::geology_river_budget;
use super::river::{run_river_step, RiverStepDetailBreakdown};
use super::terrain::run_terrain_step;
use crate::sim::world::World;

pub(super) fn run_geology_terrain_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    run_terrain_step(world);
}

pub(super) fn run_geology_river_step(world: &mut World, budget: u32) {
    let _ = run_geology_river_step_profiled(world, budget);
}

pub(super) fn run_geology_river_step_profiled(
    world: &mut World,
    budget: u32,
) -> RiverStepDetailBreakdown {
    if budget == 0 {
        return RiverStepDetailBreakdown::default();
    }
    run_river_step(world, geology_river_budget(world.exec.era, budget))
}
