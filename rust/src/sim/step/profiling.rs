use super::civilization::run_civilization_step;
use super::climate::run_climate_step;
use super::ecology::run_ecology_step;
use super::feedback::apply_feedback_queue;
use super::geology::{run_geology_river_step_profiled, run_geology_terrain_step};
use super::pipeline::prepare_step;
use super::transition::update_era_transition;

use crate::sim::world::World;

#[derive(Clone, Copy, Debug, Default)]
pub struct StepWorldBreakdown {
    pub step_feedback_ms: f64,
    pub step_geology_terrain_ms: f64,
    pub step_climate_ms: f64,
    pub step_geology_river_ms: f64,
    pub step_ecology_ms: f64,
    pub step_civilization_ms: f64,
    pub step_transition_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StepWorldRiverBreakdown {
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
pub struct StepWorldBreakdownDetailed {
    pub breakdown: StepWorldBreakdown,
    pub river: StepWorldRiverBreakdown,
}

#[cfg(target_arch = "wasm32")]
type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
fn profile_now() -> ProfileClock {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    js_sys::Date::now() - start
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

impl StepWorldBreakdown {
    fn capture_elapsed(elapsed_from: ProfileClock) -> f64 {
        profile_elapsed_ms(elapsed_from)
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.step_feedback_ms += other.step_feedback_ms;
        self.step_geology_terrain_ms += other.step_geology_terrain_ms;
        self.step_climate_ms += other.step_climate_ms;
        self.step_geology_river_ms += other.step_geology_river_ms;
        self.step_ecology_ms += other.step_ecology_ms;
        self.step_civilization_ms += other.step_civilization_ms;
        self.step_transition_ms += other.step_transition_ms;
    }
}

impl StepWorldRiverBreakdown {
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

impl StepWorldBreakdownDetailed {
    pub fn accumulate(&mut self, other: &Self) {
        self.breakdown.accumulate(&other.breakdown);
        self.river.accumulate(&other.river);
    }
}

pub fn step_world_profiled(world: &mut World) -> StepWorldBreakdown {
    step_world_profiled_detailed(world).breakdown
}

pub fn step_world_profiled_detailed(world: &mut World) -> StepWorldBreakdownDetailed {
    prepare_step(world);

    let mut breakdown = StepWorldBreakdown::default();
    let mut river_breakdown = StepWorldRiverBreakdown::default();

    let phase_start = profile_now();
    apply_feedback_queue(world);
    breakdown.step_feedback_ms = StepWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_geology_terrain_step(world, world.exec.budgets.geology);
    breakdown.step_geology_terrain_ms = StepWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_climate_step(world, world.exec.budgets.climate);
    breakdown.step_climate_ms = StepWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    let river_profile = run_geology_river_step_profiled(world, world.exec.budgets.geology);
    breakdown.step_geology_river_ms = StepWorldBreakdown::capture_elapsed(phase_start);
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
    run_ecology_step(world, world.exec.budgets.ecology);
    breakdown.step_ecology_ms = StepWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_civilization_step(world, world.exec.budgets.civilization);
    breakdown.step_civilization_ms = StepWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    update_era_transition(world);
    breakdown.step_transition_ms = StepWorldBreakdown::capture_elapsed(phase_start);

    world.exec.tick = world.exec.tick.saturating_add(1);
    StepWorldBreakdownDetailed {
        breakdown,
        river: river_breakdown,
    }
}
