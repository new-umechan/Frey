use super::civilization::run_civilization_step;
use super::climate::run_climate_step;
use super::ecology::run_ecology_step;
use super::feedback::apply_feedback_queue;
use super::geology::{run_geology_river_step, run_geology_terrain_step};
use super::transition::update_era_transition;

use crate::sim::world::World;

pub(super) fn prepare_step(world: &mut World) {
    world.exec.budgets = world.exec.era.budgets();
    world.exec.real_years_per_tick = world.exec.era.real_years_per_tick();
    world.exec.runtime_tick_ms = world.exec.era.runtime_tick_ms();
}

pub fn step_world(world: &mut World) {
    prepare_step(world);
    apply_feedback_queue(world);
    run_geology_terrain_step(world, world.exec.budgets.geology);
    run_climate_step(world, world.exec.budgets.climate);
    run_geology_river_step(world, world.exec.budgets.geology);
    run_ecology_step(world, world.exec.budgets.ecology);
    run_civilization_step(world, world.exec.budgets.civilization);
    update_era_transition(world);
    world.exec.tick = world.exec.tick.saturating_add(1);
}
