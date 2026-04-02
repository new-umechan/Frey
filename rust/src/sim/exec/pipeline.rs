use super::feedback::apply_feedback_queue;
use super::geology::{
    apply_glaciology_forcing_to_geology, apply_hydrology_erosion_to_geology, run_geology_step,
    run_hydrology_step_unprofiled,
    should_run_hydrology_mfd,
};
use super::transition::update_era_transition;

use crate::sim::world::World;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecWorldPhase {
    #[default]
    Prepare,
    Feedback,
    Geology,
    Climate,
    Glaciology,
    Hydrology,
    Ecology,
    Society,
    Transition,
    Finalize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecWorldSliceResult {
    pub next_phase: ExecWorldPhase,
    pub ticks_completed: u32,
    pub work_units_consumed: u32,
}

pub(super) fn prepare_step(world: &mut World) {
    world.clock.budgets = world.clock.epoch.budgets();
    world.clock.real_years_per_tick = world.clock.epoch.real_years_per_tick();
    world.clock.runtime_tick_ms = world.clock.epoch.runtime_tick_ms();
}

pub(super) fn run_feedback_stage(world: &mut World) {
    apply_feedback_queue(world);
}

pub(super) fn run_geology_stage(world: &mut World) {
    run_geology_step(world, world.clock.budgets.geology);
}

pub(super) fn run_climate_stage(world: &mut World) {
    crate::sim::climate::run_climate_step(world, world.clock.budgets.climate);
}

pub(super) fn run_glaciology_stage(world: &mut World) {
    crate::sim::glaciology::run_glaciology_step(world, world.clock.budgets.climate);
    apply_glaciology_forcing_to_geology(world);
    world.refresh_terrain_state();
}

pub(super) fn run_hydrology_stage(world: &mut World) {
    let run_mfd = should_run_hydrology_mfd(world);
    run_hydrology_step_unprofiled(world, world.clock.budgets.geology, run_mfd);
    apply_hydrology_erosion_to_geology(world);
    world.refresh_terrain_state();
}

pub(super) fn run_ecology_stage(world: &mut World) {
    crate::sim::ecology::run_ecology_step(world, world.clock.budgets.ecology);
}

pub(super) fn run_society_stage(world: &mut World) {
    crate::sim::domesticates::update_domesticates(world, world.clock.budgets.ecology);
    crate::sim::subsistence::update_subsistence(world, world.clock.budgets.civilization);
    crate::sim::population::update_population(world, world.clock.budgets.civilization);
    crate::sim::settlement::update_settlement(world, world.clock.budgets.civilization);
    crate::sim::polity::update_polity(world, world.clock.budgets.civilization);
    crate::sim::conflict::update_conflict(world, world.clock.budgets.civilization);
}

pub(super) fn run_transition_stage(world: &mut World) {
    update_era_transition(world);
}

pub(super) fn finalize_tick(world: &mut World) {
    world.clock.tick = world.clock.tick.saturating_add(1);
}

pub fn exec_world_slice(
    world: &mut World,
    starting_phase: ExecWorldPhase,
    work_budget: u32,
) -> ExecWorldSliceResult {
    if work_budget == 0 {
        return ExecWorldSliceResult {
            next_phase: starting_phase,
            ticks_completed: 0,
            work_units_consumed: 0,
        };
    }

    let mut next_phase = starting_phase;
    let mut work_units_consumed: u32 = 0;
    let mut ticks_completed: u32 = 0;

    while work_units_consumed < work_budget {
        match next_phase {
            ExecWorldPhase::Prepare => {
                prepare_step(world);
                next_phase = ExecWorldPhase::Feedback;
            }
            ExecWorldPhase::Feedback => {
                run_feedback_stage(world);
                next_phase = ExecWorldPhase::Geology;
            }
            ExecWorldPhase::Geology => {
                run_geology_stage(world);
                next_phase = ExecWorldPhase::Climate;
            }
            ExecWorldPhase::Climate => {
                run_climate_stage(world);
                next_phase = ExecWorldPhase::Glaciology;
            }
            ExecWorldPhase::Glaciology => {
                run_glaciology_stage(world);
                next_phase = ExecWorldPhase::Hydrology;
            }
            ExecWorldPhase::Hydrology => {
                run_hydrology_stage(world);
                next_phase = ExecWorldPhase::Ecology;
            }
            ExecWorldPhase::Ecology => {
                run_ecology_stage(world);
                next_phase = ExecWorldPhase::Society;
            }
            ExecWorldPhase::Society => {
                run_society_stage(world);
                next_phase = ExecWorldPhase::Transition;
            }
            ExecWorldPhase::Transition => {
                run_transition_stage(world);
                next_phase = ExecWorldPhase::Finalize;
            }
            ExecWorldPhase::Finalize => {
                finalize_tick(world);
                next_phase = ExecWorldPhase::Prepare;
                ticks_completed = ticks_completed.saturating_add(1);
            }
        }
        work_units_consumed = work_units_consumed.saturating_add(1);
        if ticks_completed > 0 {
            break;
        }
    }

    ExecWorldSliceResult {
        next_phase,
        ticks_completed,
        work_units_consumed,
    }
}

pub fn exec_world(world: &mut World) {
    prepare_step(world);
    run_feedback_stage(world);
    run_geology_stage(world);
    run_climate_stage(world);
    run_glaciology_stage(world);
    run_hydrology_stage(world);
    run_ecology_stage(world);
    run_society_stage(world);
    run_transition_stage(world);
    finalize_tick(world);
}
