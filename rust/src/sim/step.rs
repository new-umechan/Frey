#[path = "step_civilization.rs"]
mod civilization;
#[path = "step_climate.rs"]
mod climate;
#[path = "step_ecology.rs"]
mod ecology;
#[path = "step_feedback.rs"]
mod feedback;
#[path = "step_transition.rs"]
mod transition;

#[path = "step/geology.rs"]
mod geology;
#[path = "step/math.rs"]
mod math;
#[path = "step/river.rs"]
mod river;
#[path = "step/terrain.rs"]
mod terrain;

use civilization::run_civilization_step;
use climate::run_climate_step;
use ecology::run_ecology_step;
use feedback::apply_feedback_queue;
use geology::{run_geology_river_step, run_geology_river_step_profiled, run_geology_terrain_step};
use transition::update_era_transition;

use super::world::{EraKind, World};

pub(super) const MAX_HEIGHT_DELTA_PER_STEP: f32 = 0.020;
pub(super) const DEFAULT_DIFFUSION_WEIGHT: f32 = 0.06;
pub(super) const CONVERGENT_THRESHOLD: f32 = 0.010;
pub(super) const DIVERGENT_THRESHOLD: f32 = 0.010;
pub(super) const TRANSFORM_THRESHOLD: f32 = 0.014;
pub(super) const CRUST_RAIN_LAND: f32 = 0.12;
pub(super) const CRUST_RAIN_SEA: f32 = 0.04;
#[cfg(test)]
pub(super) const CHANNEL_TRANSFER_BASE: f32 = 0.18;
#[cfg(test)]
pub(super) const CHANNEL_TRANSFER_SLOPE_GAIN: f32 = 6.0;
#[cfg(test)]
pub(super) const CHANNEL_TRANSFER_MAX: f32 = 0.72;
#[cfg(test)]
pub(super) const FLUX_LOCAL_DECAY: f32 = 0.82;

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
    }
}

impl StepWorldBreakdownDetailed {
    pub fn accumulate(&mut self, other: &Self) {
        self.breakdown.accumulate(&other.breakdown);
        self.river.accumulate(&other.river);
    }
}

pub fn step_world(world: &mut World) {
    world.exec.budgets = world.exec.era.budgets();
    world.exec.real_years_per_tick = world.exec.era.real_years_per_tick();
    world.exec.runtime_tick_ms = world.exec.era.runtime_tick_ms();
    apply_feedback_queue(world);
    run_geology_terrain_step(world, world.exec.budgets.geology);
    run_climate_step(world, world.exec.budgets.climate);
    run_geology_river_step(world, world.exec.budgets.geology);
    run_ecology_step(world, world.exec.budgets.ecology);
    run_civilization_step(world, world.exec.budgets.civilization);
    update_era_transition(world);
    world.exec.tick = world.exec.tick.saturating_add(1);
}

pub fn step_world_profiled(world: &mut World) -> StepWorldBreakdown {
    step_world_profiled_detailed(world).breakdown
}

pub fn step_world_profiled_detailed(world: &mut World) -> StepWorldBreakdownDetailed {
    world.exec.budgets = world.exec.era.budgets();
    world.exec.real_years_per_tick = world.exec.era.real_years_per_tick();
    world.exec.runtime_tick_ms = world.exec.era.runtime_tick_ms();

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

pub(super) fn geology_river_budget(era: EraKind, geology_budget: u32) -> u32 {
    let scale = match era {
        EraKind::Crust => 1,
        EraKind::Environment => 4,
        EraKind::Life => 3,
        EraKind::Civilization => 2,
        EraKind::History => 1,
    };
    geology_budget.saturating_mul(scale).max(1)
}

pub(super) fn blend_alpha(budget: u32, base: f32) -> f32 {
    let b = budget.max(1) as f32;
    (1.0 - (1.0 - base).powf(b)).clamp(0.0, 1.0)
}

pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = math::length3(v);
    if len <= 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use crate::TerrainParams;

    use super::*;
    use crate::sim::world::{GeologyState, World, WorldMesh};

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                normalize3([0.0, 0.8, 0.6]),
                normalize3([0.7, 0.2, 0.6]),
                normalize3([0.4, -0.7, 0.6]),
                normalize3([-0.6, -0.1, 0.8]),
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let geology = GeologyState {
            height: vec![0.45, 0.15, -0.25, 0.05],
            plate_id: vec![0, 0, 1, 1],
            river_flux: vec![0.1, 0.2, 0.3, 0.1],
            river_next: vec![1, 2, -1, 2],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            boundary_condition: vec![0.0; 4],
        };
        World::new(mesh, geology)
    }

    #[test]
    fn step_world_advances_tick_and_sets_budget_to_one() {
        let mut world = build_test_world();
        world.exec.era = EraKind::History;
        step_world(&mut world);
        assert_eq!(world.exec.tick, 1);
        assert_eq!(world.exec.budgets.geology, 1);
        assert_eq!(world.exec.budgets.climate, 1);
        assert_eq!(world.exec.budgets.ecology, 1);
        assert_eq!(world.exec.budgets.civilization, 4);
    }

    #[test]
    fn river_fallback_routes_flux_downhill() {
        let mut world = build_test_world();
        world.exec.river_erosion_state = None;
        river::run_river_step(&mut world, 1);
        assert_eq!(world.state.geology.river_next.len(), 4);
        assert!(world
            .state
            .geology
            .river_flux
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0));
    }

    #[test]
    fn terrain_step_initializes_dynamics_and_updates_boundary_signal() {
        let mut world = build_test_world();
        let params = TerrainParams::default();
        world.exec.river_erosion_state = Some(crate::ErosionAutomatonState {
            positions: world.mesh.positions.clone(),
            nbr_offsets: world.mesh.nbr_offsets.clone(),
            nbrs: world.mesh.nbrs.clone(),
            height: world.state.geology.height.clone(),
            water: vec![0.0; 4],
            sediment: vec![0.0; 4],
            armor: vec![0.0; 4],
            rain: vec![0.1; 4],
            river_flux: world.state.geology.river_flux.clone(),
            river_next: world.state.geology.river_next.clone(),
            active_queue: vec![0, 1, 2, 3],
            active_head: 0,
            in_queue: vec![1; 4],
            rain_cursor: 0,
            tick: 0,
            last_rebuild_tick: 0,
            flux_scale_ema: 1.0,
            last_river_driver: 1.0,
            prev_river_next: world.state.geology.river_next.clone(),
            flow_heading: vec![[0.0, 0.0, 0.0]; 4],
            groundwater_storage: vec![0.0; 4],
            scratch_effective_runoff: vec![0.0; 4],
            scratch_changed_mark: vec![0; 4],
            scratch_flux_samples: Vec::with_capacity(2),
            recent_changed: Vec::new(),
            sink_id: vec![-1; 4],
            sink_route_next: vec![-1; 4],
            sink_spill_cell: Vec::new(),
            sink_spill_to: Vec::new(),
            sink_capacity_total: Vec::new(),
            sink_capacity_remaining: Vec::new(),
            sink_storage_sediment: Vec::new(),
            sink_spill_level: Vec::new(),
            sink_overflow_active: Vec::new(),
            sink_dirty: vec![1; 4],
            params,
        });

        terrain::run_terrain_step(&mut world);

        assert!(world.exec.terrain_dynamics.is_some());
        assert_eq!(world.state.geology.boundary_condition.len(), 4);
    }

    #[test]
    fn route_river_flux_emphasizes_upstream_accumulation() {
        let height = vec![0.6, 0.4, 0.2];
        let river_next = vec![1, 2, -1];
        let rain = vec![0.2, 0.2, 0.2];
        let flux = river::route_river_flux(&height, &river_next, &rain);
        assert_eq!(flux.len(), 3);
        assert!(flux[2] > flux[1]);
        assert_eq!(flux[0], 0.0);
    }

    #[test]
    fn river_fallback_applies_threshold_and_clears_ocean_next() {
        let mut world = build_test_world();
        world.exec.era = EraKind::Environment;
        world.exec.river_erosion_state = None;
        world.state.climate.runoff = vec![10.0; 4];

        river::run_river_step(&mut world, 1);

        assert_eq!(world.state.geology.river_next[2], -1);
        assert_eq!(world.state.geology.river_flux[0], 0.0);
        assert_eq!(world.state.geology.river_flux[1], 0.0);
        assert_eq!(world.state.geology.river_flux[3], 0.0);
    }
}
