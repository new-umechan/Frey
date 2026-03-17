use super::feedback::apply_feedback_queue;
use super::geology::{run_geology_step, run_hydrology_step_unprofiled};
use super::transition::update_era_transition;

use crate::sim::world::World;

pub(super) fn prepare_step(world: &mut World) {
    world.exec.budgets = world.exec.era.budgets();
    world.exec.real_years_per_tick = world.exec.era.real_years_per_tick();
    world.exec.runtime_tick_ms = world.exec.era.runtime_tick_ms();
}

pub fn exec_world(world: &mut World) {
    prepare_step(world);
    apply_feedback_queue(world);
    run_geology_step(world, world.exec.budgets.geology);
    crate::sim::climate::run_climate_step(world, world.exec.budgets.climate);
    run_hydrology_step_unprofiled(world, world.exec.budgets.geology);
    crate::sim::ecology::run_ecology_step(world, world.exec.budgets.ecology);
    crate::sim::domesticates::update_domesticates(world, world.exec.budgets.ecology);
    crate::sim::subsistence::update_subsistence(world, world.exec.budgets.civilization);
    crate::sim::population::update_population(world, world.exec.budgets.civilization);
    crate::sim::settlement::update_settlement(world, world.exec.budgets.civilization);
    crate::sim::polity::update_polity(world, world.exec.budgets.civilization);
    crate::sim::conflict::update_conflict(world, world.exec.budgets.civilization);
    update_era_transition(world);
    world.exec.tick = world.exec.tick.saturating_add(1);
}
