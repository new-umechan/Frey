use super::feedback::apply_feedback_queue;
use super::geology::{run_geology_step, run_hydrology_step_profiled};
use super::pipeline::prepare_step;
use super::transition::update_era_transition;

use crate::sim::world::World;

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecWorldBreakdown {
    pub exec_feedback_ms: f64,
    pub exec_geology_terrain_ms: f64,
    pub exec_climate_ms: f64,
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

impl ExecWorldBreakdown {
    fn capture_elapsed(elapsed_from: ProfileClock) -> f64 {
        profile_elapsed_ms(elapsed_from)
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.exec_feedback_ms += other.exec_feedback_ms;
        self.exec_geology_terrain_ms += other.exec_geology_terrain_ms;
        self.exec_climate_ms += other.exec_climate_ms;
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
    exec_world_profiled_detailed(world).breakdown
}

pub fn exec_world_profiled_detailed(world: &mut World) -> ExecWorldBreakdownDetailed {
    prepare_step(world);

    let mut breakdown = ExecWorldBreakdown::default();
    let mut river_breakdown = ExecWorldRiverBreakdown::default();

    let phase_start = profile_now();
    apply_feedback_queue(world);
    breakdown.exec_feedback_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    run_geology_step(world, world.exec.budgets.geology);
    breakdown.exec_geology_terrain_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    crate::sim::climate::run_climate_step(world, world.exec.budgets.climate);
    breakdown.exec_climate_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    let river_profile = run_hydrology_step_profiled(world, world.exec.budgets.geology);
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
    crate::sim::ecology::run_ecology_step(world, world.exec.budgets.ecology);
    breakdown.exec_ecology_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    crate::sim::domesticates::update_domesticates(world, world.exec.budgets.ecology);
    crate::sim::subsistence::update_subsistence(world, world.exec.budgets.civilization);
    crate::sim::population::update_population(world, world.exec.budgets.civilization);
    crate::sim::settlement::update_settlement(world, world.exec.budgets.civilization);
    crate::sim::polity::update_polity(world, world.exec.budgets.civilization);
    crate::sim::conflict::update_conflict(world, world.exec.budgets.civilization);
    breakdown.exec_society_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    let phase_start = profile_now();
    update_era_transition(world);
    breakdown.exec_transition_ms = ExecWorldBreakdown::capture_elapsed(phase_start);

    world.exec.tick = world.exec.tick.saturating_add(1);
    ExecWorldBreakdownDetailed {
        breakdown,
        river: river_breakdown,
    }
}
