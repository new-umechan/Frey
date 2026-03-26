// 横断層
pub mod exec;
pub mod geology_types;
pub mod state;
pub mod world;

// Tier 1（UPDATE_DAG 順）
pub mod climate;
pub mod conflict;
pub mod domesticates;
pub mod ecology;
pub mod geology;
pub mod hydrology;
pub mod polity;
pub mod population;
pub mod settlement;
pub mod subsistence;
pub(crate) use crate::common::geo;
pub use state::erosion;

pub use exec::{
    exec_world, exec_world_profiled, exec_world_profiled_detailed, exec_world_slice,
    ExecWorldBreakdown, ExecWorldBreakdownDetailed, ExecWorldPhase, ExecWorldSliceResult,
};

use crate::common::mesh::{flatten_positions, generate_icosphere};

use self::geology_types::{GeologyOutput, GeologyParams, MeshOutput};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ErosionAutomatonBreakdown {
    pub sink_rebuild_ms: f64,
    pub cell_process_ms: f64,
    pub queue_update_ms: f64,
    pub sink_rebuild_full_count: u32,
    pub sink_rebuild_partial_count: u32,
    pub sink_rebuild_skipped_count: u32,
    pub sink_rebuild_fallback_full_count: u32,
}

pub(crate) fn build_mesh(level: u32) -> Result<MeshOutput, String> {
    if level > 8 {
        return Err("level must be between 0 and 8".to_string());
    }

    let (positions, indices) = generate_icosphere(level);
    let flattened_positions = flatten_positions(&positions);
    Ok(MeshOutput {
        positions: flattened_positions,
        indices,
    })
}

pub(crate) fn build_geology(seed: &str, geology_params: GeologyParams) -> GeologyOutput {
    geology::generate(seed, geology_params)
}

pub fn build_geology_with_mesh(
    seed: &str,
    geology_params: GeologyParams,
) -> (GeologyOutput, Vec<[f32; 3]>, Vec<u32>, Vec<u32>) {
    geology::generate_with_mesh(seed, geology_params)
}

pub fn run_climate_step_for_bench(world: &mut world::World, climate_budget: u32) {
    climate::surface::run_climate_step(world, climate_budget);
}

pub fn build_hydrology_state_for_bench(
    world: &world::World,
    params: GeologyParams,
) -> erosion::ErosionAutomatonState {
    const EROSION_RAIN_SCALE_MM: f32 = 1_200.0;

    let cell_count = world.state.geology.height.len();
    erosion::ErosionAutomatonState {
        positions: world.mesh.positions.clone(),
        nbr_offsets: world.mesh.nbr_offsets.clone(),
        nbrs: world.mesh.nbrs.clone(),
        height: world.state.geology.height.clone(),
        water: vec![0.0; cell_count],
        sediment: vec![0.0; cell_count],
        armor: vec![0.0; cell_count],
        rain: world
            .state
            .climate
            .runoff
            .iter()
            .copied()
            .map(|value| (value.max(0.0) / EROSION_RAIN_SCALE_MM).clamp(0.0, 1.0))
            .collect(),
        river_flux: world.state.hydrology.river_flow.clone(),
        river_next: world.state.hydrology.river_next.clone(),
        active_queue: (0..cell_count as u32).collect(),
        active_head: 0,
        in_queue: vec![1; cell_count],
        rain_cursor: 0,
        tick: world.clock.tick,
        last_rebuild_tick: world.clock.tick.saturating_sub(1),
        last_sink_full_rebuild_tick: world.clock.tick.saturating_sub(8),
        flux_scale_ema: 1.0,
        last_river_driver: 1.0,
        prev_river_next: world.state.hydrology.river_next.clone(),
        flow_heading: vec![[0.0, 0.0, 0.0]; cell_count],
        groundwater_storage: vec![0.0; cell_count],
        scratch_effective_runoff: vec![0.0; cell_count],
        scratch_changed_mark: vec![0; cell_count],
        scratch_flux_samples: Vec::with_capacity(cell_count / 2),
        recent_changed: Vec::new(),
        sink_id: vec![-1; cell_count],
        sink_route_next: vec![-1; cell_count],
        sink_spill_cell: Vec::new(),
        sink_spill_to: Vec::new(),
        sink_capacity_total: Vec::new(),
        sink_capacity_remaining: Vec::new(),
        sink_storage_sediment: Vec::new(),
        sink_spill_level: Vec::new(),
        sink_overflow_active: Vec::new(),
        sink_dirty: vec![1; cell_count],
        params,
    }
}

pub fn run_hydrology_step_for_bench(world: &mut world::World, geology_budget: u32, run_mfd: bool) {
    if run_mfd {
        hydrology::run_hydrology_step(world, geology_budget);
    } else {
        hydrology::run_hydrology_flow_step(world, geology_budget);
    }
}

pub fn run_ecology_step_for_bench(world: &mut world::World, ecology_budget: u32) {
    ecology::run_ecology_step(world, ecology_budget);
}

pub(crate) fn step_erosion_automaton(
    state: &mut erosion::ErosionAutomatonState,
    budget_cells: u32,
) -> ErosionAutomatonBreakdown {
    geology::step_async_erosion_automaton(state, budget_cells)
}
