use super::geology::{
    apply_hydrology_erosion_to_geology, run_geology_step_with_state, run_hydrology_step_profiled,
    should_run_hydrology_mfd_for_geology,
};
use super::pipeline::{
    finalize_tick, prepare_step, run_climate_stage, run_ecology_stage, run_feedback_stage,
    run_glaciology_stage_with_hydrology, run_society_stage, run_transition_stage,
};

use crate::sim::world::{FeedbackQueue, World};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldBreakdown {
    pub exec_feedback_ms: f64,
    pub exec_geology_terrain_ms: f64,
    pub exec_climate_ms: f64,
    pub exec_glaciology_ms: f64,
    pub exec_hydrology_ms: f64,
    pub exec_ecology_ms: f64,
    pub exec_society_ms: f64,
    pub exec_transition_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldRiverBreakdown {
    pub step_geology_river_prepare_ms: f64,
    pub step_geology_river_automaton_ms: f64,
    pub step_geology_river_automaton_sink_ms: f64,
    pub step_geology_river_automaton_cell_ms: f64,
    pub step_geology_river_automaton_queue_ms: f64,
    pub step_geology_river_network_ms: f64,
    pub step_geology_river_sync_ms: f64,
    pub step_geology_river_fallback_ms: f64,
    pub river_network_rebuild_count: u32,
    pub river_fallback_count: u32,
    pub sink_rebuild_full_count: u32,
    pub sink_rebuild_partial_count: u32,
    pub sink_rebuild_skipped_count: u32,
    pub sink_rebuild_fallback_full_count: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldBreakdownDetailed {
    pub breakdown: ExecWorldBreakdown,
    pub river: ExecWorldRiverBreakdown,
}

#[cfg(target_arch = "wasm32")]
type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
fn profile_now() -> ProfileClock {
    let global = js_sys::global();
    let performance = js_sys::Reflect::get(&global, &JsValue::from_str("performance")).ok();
    if let Some(perf) = performance {
        if !perf.is_null() && !perf.is_undefined() {
            let now_fn = js_sys::Reflect::get(&perf, &JsValue::from_str("now")).ok();
            if let Some(now_fn) = now_fn {
                if let Some(now_fn) = now_fn.dyn_ref::<js_sys::Function>() {
                    if let Ok(value) = now_fn.call0(&perf) {
                        if let Some(ms) = value.as_f64() {
                            return ms;
                        }
                    }
                }
            }
        }
    }
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    profile_now() - start
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

impl ExecWorldBreakdown {
    fn capture_elapsed(elapsed_from: ProfileClock) -> f64 {
        profile_elapsed_ms(elapsed_from)
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.exec_feedback_ms += other.exec_feedback_ms;
        self.exec_geology_terrain_ms += other.exec_geology_terrain_ms;
        self.exec_climate_ms += other.exec_climate_ms;
        self.exec_glaciology_ms += other.exec_glaciology_ms;
        self.exec_hydrology_ms += other.exec_hydrology_ms;
        self.exec_ecology_ms += other.exec_ecology_ms;
        self.exec_society_ms += other.exec_society_ms;
        self.exec_transition_ms += other.exec_transition_ms;
    }
}

impl ExecWorldRiverBreakdown {
    pub fn accumulate(&mut self, other: &Self) {
        self.step_geology_river_prepare_ms += other.step_geology_river_prepare_ms;
        self.step_geology_river_automaton_ms += other.step_geology_river_automaton_ms;
        self.step_geology_river_automaton_sink_ms += other.step_geology_river_automaton_sink_ms;
        self.step_geology_river_automaton_cell_ms += other.step_geology_river_automaton_cell_ms;
        self.step_geology_river_automaton_queue_ms += other.step_geology_river_automaton_queue_ms;
        self.step_geology_river_network_ms += other.step_geology_river_network_ms;
        self.step_geology_river_sync_ms += other.step_geology_river_sync_ms;
        self.step_geology_river_fallback_ms += other.step_geology_river_fallback_ms;
        self.river_network_rebuild_count = self
            .river_network_rebuild_count
            .saturating_add(other.river_network_rebuild_count);
        self.river_fallback_count = self
            .river_fallback_count
            .saturating_add(other.river_fallback_count);
        self.sink_rebuild_full_count = self
            .sink_rebuild_full_count
            .saturating_add(other.sink_rebuild_full_count);
        self.sink_rebuild_partial_count = self
            .sink_rebuild_partial_count
            .saturating_add(other.sink_rebuild_partial_count);
        self.sink_rebuild_skipped_count = self
            .sink_rebuild_skipped_count
            .saturating_add(other.sink_rebuild_skipped_count);
        self.sink_rebuild_fallback_full_count = self
            .sink_rebuild_fallback_full_count
            .saturating_add(other.sink_rebuild_fallback_full_count);
    }
}

impl ExecWorldBreakdownDetailed {
    pub fn accumulate(&mut self, other: &Self) {
        self.breakdown.accumulate(&other.breakdown);
        self.river.accumulate(&other.river);
    }
}

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
    exec_world_profiled_detailed_with_feedback_and_hydrology(
        world,
        feedback,
        &mut hydrology_state,
    )
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
    prepare_step(world);

    let mut breakdown = ExecWorldBreakdown::default();
    let mut river_breakdown = ExecWorldRiverBreakdown::default();

    let phase_start = profile_now();
    run_feedback_stage(world, feedback);
    breakdown.exec_feedback_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_geology_step_with_state(world, geology_state, world.clock.budgets.geology);
    breakdown.exec_geology_terrain_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_climate_stage(world);
    breakdown.exec_climate_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_glaciology_stage_with_hydrology(world, hydrology_state);
    breakdown.exec_glaciology_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    let run_mfd = should_run_hydrology_mfd_for_geology(world, geology_state.as_ref());
    let river_profile =
        run_hydrology_step_profiled(world, hydrology_state, world.clock.budgets.geology, run_mfd);
    apply_hydrology_erosion_to_geology(world, geology_state, hydrology_state);
    breakdown.exec_hydrology_ms = ExecWorldBreakdown::capture_elapsed(phase_start);
    river_breakdown.step_geology_river_prepare_ms = river_profile.river_prepare_ms;
    river_breakdown.step_geology_river_automaton_ms = river_profile.river_automaton_ms;
    river_breakdown.step_geology_river_automaton_sink_ms = river_profile.river_automaton_sink_ms;
    river_breakdown.step_geology_river_automaton_cell_ms = river_profile.river_automaton_cell_ms;
    river_breakdown.step_geology_river_automaton_queue_ms = river_profile.river_automaton_queue_ms;
    river_breakdown.step_geology_river_network_ms = river_profile.river_network_ms;
    river_breakdown.step_geology_river_sync_ms = river_profile.river_sync_ms;
    river_breakdown.step_geology_river_fallback_ms = river_profile.river_fallback_ms;
    river_breakdown.river_network_rebuild_count = river_profile.network_rebuild_count;
    river_breakdown.river_fallback_count = river_profile.fallback_count;
    river_breakdown.sink_rebuild_full_count = river_profile.sink_rebuild_full_count;
    river_breakdown.sink_rebuild_partial_count = river_profile.sink_rebuild_partial_count;
    river_breakdown.sink_rebuild_skipped_count = river_profile.sink_rebuild_skipped_count;
    river_breakdown.sink_rebuild_fallback_full_count =
        river_profile.sink_rebuild_fallback_full_count;

    let phase_start = profile_now();
    run_ecology_stage(world);
    breakdown.exec_ecology_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_society_stage(world);
    breakdown.exec_society_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_transition_stage(world);
    breakdown.exec_transition_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    finalize_tick(world);
    ExecWorldBreakdownDetailed {
        breakdown,
        river: river_breakdown,
    }
}
