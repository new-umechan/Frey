use super::feedback::{apply_feedback_queue, apply_feedback_queue_for_module};
use super::geology::{
    apply_glaciology_forcing_to_geology, apply_hydrology_erosion_to_geology,
    run_geology_step_with_state, run_hydrology_step_unprofiled,
    should_run_hydrology_mfd_for_geology,
};
use super::modules::{
    declaration_for_phase, declared_phase_order, next_phase_after, phase_accepts_module_feedback,
    phase_completes_tick, validate_module_declarations, ModuleExecContext,
};
use super::transition::update_era_transition;

use crate::sim::world::{FeedbackQueue, World};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExecWorldPhase {
    #[default]
    Prepare,
    ExecFeedback,
    Geology,
    Climate,
    Glaciology,
    Hydrology,
    Ecology,
    Domesticates,
    Subsistence,
    Population,
    Settlement,
    Polity,
    Conflict,
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

pub(super) fn run_feedback_stage(world: &mut World, feedback: &mut FeedbackQueue) {
    apply_feedback_queue(world, feedback);
}

pub(super) fn run_geology_stage_with_geology(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
) {
    run_geology_step_with_state(world, geology_state, world.clock.budgets.geology);
}

pub(super) fn run_climate_stage(world: &mut World) {
    crate::sim::climate::run_climate_step(world, world.clock.budgets.climate);
}

pub(super) fn run_glaciology_stage_with_hydrology(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    crate::sim::glaciology::run_glaciology_step(world, world.clock.budgets.climate);
    apply_glaciology_forcing_to_geology(world, hydrology_state);
    world.refresh_terrain_state();
}

pub(super) fn run_hydrology_stage_with_hydrology(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    let run_mfd = should_run_hydrology_mfd_for_geology(world, geology_state.as_ref());
    run_hydrology_step_unprofiled(world, hydrology_state, world.clock.budgets.geology, run_mfd);
    apply_hydrology_erosion_to_geology(world, geology_state, hydrology_state);
    world.refresh_terrain_state();
}

pub(super) fn run_ecology_stage(world: &mut World) {
    crate::sim::ecology::run_ecology_step(world, world.clock.budgets.ecology);
}

pub(super) fn run_domesticates_stage(world: &mut World) {
    crate::sim::domesticates::update_domesticates(world, world.clock.budgets.ecology);
}

pub(super) fn run_subsistence_stage(world: &mut World) {
    crate::sim::subsistence::update_subsistence(world, world.clock.budgets.civilization);
}

pub(super) fn run_population_stage(world: &mut World) {
    crate::sim::population::update_population(world, world.clock.budgets.civilization);
}

pub(super) fn run_settlement_stage(world: &mut World) {
    crate::sim::settlement::update_settlement(world, world.clock.budgets.civilization);
}

pub(super) fn run_polity_stage(world: &mut World) {
    crate::sim::polity::update_polity(world, world.clock.budgets.civilization);
}

pub(super) fn run_conflict_stage(world: &mut World) {
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
    feedback: &mut FeedbackQueue,
    starting_phase: ExecWorldPhase,
    work_budget: u32,
) -> ExecWorldSliceResult {
    let mut hydrology_state = None;
    exec_world_slice_with_hydrology(
        world,
        feedback,
        &mut hydrology_state,
        starting_phase,
        work_budget,
    )
}

pub fn exec_world_slice_with_hydrology(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    starting_phase: ExecWorldPhase,
    work_budget: u32,
) -> ExecWorldSliceResult {
    world.with_geology_exec_state(|world, geology_state| {
        exec_world_slice_with_states(
            world,
            feedback,
            geology_state,
            hydrology_state,
            starting_phase,
            work_budget,
        )
    })
}

pub fn exec_world_slice_with_states(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    starting_phase: ExecWorldPhase,
    work_budget: u32,
) -> ExecWorldSliceResult {
    validate_module_declarations();
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
    let mut ctx = ModuleExecContext {
        feedback,
        geology_state,
        hydrology_state,
    };

    while work_units_consumed < work_budget {
        let phase = next_phase;
        let declaration = declaration_for_phase(phase);
        if phase_accepts_module_feedback(phase) {
            apply_feedback_queue_for_module(world, ctx.feedback, declaration.module_id);
        }
        (declaration.step)(world, &mut ctx);
        next_phase = next_phase_after(phase);
        if phase_completes_tick(phase) {
            ticks_completed = ticks_completed.saturating_add(1);
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
    let mut feedback = FeedbackQueue::new(world.cell_count());
    exec_world_with_feedback(world, &mut feedback);
}

pub fn exec_world_with_feedback(world: &mut World, feedback: &mut FeedbackQueue) {
    let mut hydrology_state: crate::sim::exec::HydrologyExecState = None;
    exec_world_with_feedback_and_hydrology(world, feedback, &mut hydrology_state);
}

pub fn exec_world_with_feedback_and_hydrology(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    world.with_geology_exec_state(|world, geology_state| {
        exec_world_with_feedback_and_states(world, feedback, geology_state, hydrology_state)
    });
}

pub fn exec_world_with_feedback_and_states(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    validate_module_declarations();
    let mut ctx = ModuleExecContext {
        feedback,
        geology_state,
        hydrology_state,
    };
    for phase in declared_phase_order() {
        let declaration = declaration_for_phase(phase);
        if phase_accepts_module_feedback(phase) {
            apply_feedback_queue_for_module(world, ctx.feedback, declaration.module_id);
        }
        (declaration.step)(world, &mut ctx);
    }
}
