use super::feedback::apply_feedback_queue;
use super::geology::{run_geology_step, run_hydrology_step_unprofiled};
use super::transition::update_era_transition;

use crate::sim::world::World;

pub(super) fn prepare_step(world: &mut World) {
    world.exec.budgets = world.exec.era.budgets();
    world.exec.real_years_per_tick = world.exec.era.real_years_per_tick();
    world.exec.runtime_tick_ms = world.exec.era.runtime_tick_ms();
}

pub(super) fn run_feedback_stage(world: &mut World) {
    apply_feedback_queue(world);
}

pub(super) fn run_geology_stage(world: &mut World) {
    run_geology_step(world, world.exec.budgets.geology);
}

pub(super) fn run_climate_stage(world: &mut World) {
    crate::sim::climate::run_climate_step(world, world.exec.budgets.climate);
}

pub(super) fn run_hydrology_stage(world: &mut World) {
    run_hydrology_step_unprofiled(world, world.exec.budgets.geology);
}

pub(super) fn run_ecology_stage(world: &mut World) {
    crate::sim::ecology::run_ecology_step(world, world.exec.budgets.ecology);
}

pub(super) fn run_society_stage(world: &mut World) {
    crate::sim::domesticates::update_domesticates(world, world.exec.budgets.ecology);
    crate::sim::subsistence::update_subsistence(world, world.exec.budgets.civilization);
    crate::sim::population::update_population(world, world.exec.budgets.civilization);
    crate::sim::settlement::update_settlement(world, world.exec.budgets.civilization);
    crate::sim::polity::update_polity(world, world.exec.budgets.civilization);
    crate::sim::conflict::update_conflict(world, world.exec.budgets.civilization);
}

pub(super) fn run_transition_stage(world: &mut World) {
    update_era_transition(world);
}

pub(super) fn finalize_tick(world: &mut World) {
    world.exec.tick = world.exec.tick.saturating_add(1);
}

pub fn exec_world(world: &mut World) {
    prepare_step(world);
    run_feedback_stage(world);
    run_geology_stage(world);
    run_climate_stage(world);
    run_hydrology_stage(world);
    run_ecology_stage(world);
    run_society_stage(world);
    run_transition_stage(world);
    finalize_tick(world);
}
