// 横断層
pub mod exec;
pub mod world;
pub mod state;
pub mod geology_types;

// Tier 1（UPDATE_DAG 順）
pub mod geology;
pub mod climate;
pub mod hydrology;
pub mod ecology;
pub mod domesticates;
pub mod subsistence;
pub mod population;
pub mod settlement;
pub mod polity;
pub mod conflict;
pub use state::erosion;
pub(crate) use crate::common::geo;

pub use exec::{
    exec_world,
    exec_world_profiled,
    exec_world_profiled_detailed,
    ExecWorldBreakdown,
    ExecWorldBreakdownDetailed,
};

use crate::common::mesh::{flatten_positions, generate_icosphere};

use self::geology_types::{MeshOutput, GeologyOutput, GeologyParams};

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

pub(crate) fn step_erosion_automaton(
    state: &mut erosion::ErosionAutomatonState,
    budget_cells: u32,
) -> ErosionAutomatonBreakdown {
    geology::step_async_erosion_automaton(state, budget_cells)
}
