use super::geology::{
    apply_hydrology_erosion_to_geology, run_hydrology_step_profiled,
    should_run_hydrology_mfd_for_geology,
};
use super::modules::{
    declaration_for_phase, declared_phase_order, phase_accepts_module_feedback,
    phase_execution_kind, phase_profile_category, ExecutionKind, ModuleExecContext,
    ProfileCategory,
};
use super::profiling::{
    profile_now, ExecWorldBreakdown, ExecWorldBreakdownDetailed, ExecWorldRiverBreakdown,
};
use super::profiling_river::apply_hydrology_profile;

use crate::sim::world::{FeedbackQueue, World};

pub fn exec_world_profiled(world: &mut World) -> ExecWorldBreakdown {
    let mut feedback = FeedbackQueue::new(world.cell_count());
    exec_world_profiled_detailed_with_feedback(world, &mut feedback).breakdown
}

pub fn exec_world_profiled_detailed(world: &mut World) -> ExecWorldBreakdownDetailed {
    let mut feedback = FeedbackQueue::new(world.cell_count());
    exec_world_profiled_detailed_with_feedback(world, &mut feedback)
}

pub fn exec_world_profiled_detailed_with_feedback(
    world: &mut World,
    feedback: &mut FeedbackQueue,
) -> ExecWorldBreakdownDetailed {
    let mut hydrology_state: crate::sim::exec::HydrologyExecState = None;
    exec_world_profiled_detailed_with_feedback_and_hydrology(world, feedback, &mut hydrology_state)
}

pub fn exec_world_profiled_detailed_with_feedback_and_hydrology(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) -> ExecWorldBreakdownDetailed {
    world.with_geology_exec_state(|world, geology_state| {
        exec_world_profiled_detailed_with_feedback_and_states(
            world,
            feedback,
            geology_state,
            hydrology_state,
        )
    })
}

pub fn exec_world_profiled_detailed_with_feedback_and_states(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) -> ExecWorldBreakdownDetailed {
    let mut breakdown = ExecWorldBreakdown::default();
    let mut river_breakdown = ExecWorldRiverBreakdown::default();
    let mut ctx = ModuleExecContext {
        feedback,
        geology_state,
        hydrology_state,
    };

    for phase in declared_phase_order() {
        let declaration = declaration_for_phase(phase);
        if phase_accepts_module_feedback(phase) {
            super::feedback::apply_feedback_queue_for_module(
                world,
                ctx.feedback,
                declaration.module_id,
            );
        }

        let phase_start = profile_now();
        match phase_execution_kind(phase) {
            ExecutionKind::HydrologyCoupled => {
                let run_mfd =
                    should_run_hydrology_mfd_for_geology(world, ctx.geology_state.as_ref());
                let river_profile = run_hydrology_step_profiled(
                    world,
                    ctx.hydrology_state,
                    ctx.geology_state.as_ref(),
                    world.clock.budgets.geology,
                    run_mfd,
                );
                apply_hydrology_erosion_to_geology(world, ctx.geology_state, ctx.hydrology_state);
                apply_hydrology_profile(&mut river_breakdown, river_profile);
                accumulate_phase_elapsed(
                    &mut breakdown,
                    phase_profile_category(phase),
                    ExecWorldBreakdown::capture_elapsed(phase_start),
                );
            }
            ExecutionKind::Plain => {
                (declaration.step)(world, &mut ctx);
                accumulate_phase_elapsed(
                    &mut breakdown,
                    phase_profile_category(phase),
                    ExecWorldBreakdown::capture_elapsed(phase_start),
                );
            }
        }
    }

    ExecWorldBreakdownDetailed {
        breakdown,
        river: river_breakdown,
    }
}

fn accumulate_phase_elapsed(
    breakdown: &mut ExecWorldBreakdown,
    category: ProfileCategory,
    elapsed_ms: f64,
) {
    match category {
        ProfileCategory::None => {}
        ProfileCategory::Feedback => breakdown.exec_feedback_ms += elapsed_ms,
        ProfileCategory::GeologyTerrain => breakdown.exec_geology_terrain_ms += elapsed_ms,
        ProfileCategory::Climate => breakdown.exec_climate_ms += elapsed_ms,
        ProfileCategory::Glaciology => breakdown.exec_glaciology_ms += elapsed_ms,
        ProfileCategory::Hydrology => breakdown.exec_hydrology_ms += elapsed_ms,
        ProfileCategory::Ecology => breakdown.exec_ecology_ms += elapsed_ms,
        ProfileCategory::Society => breakdown.exec_society_ms += elapsed_ms,
        ProfileCategory::Transition => breakdown.exec_transition_ms += elapsed_ms,
    }
}
