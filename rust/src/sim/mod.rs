pub mod state;
pub mod step;
pub mod terrain;
pub mod terrain_types;
pub mod world;
pub use state::erosion;
pub(crate) use crate::common::geo;

pub use step::{
    step_world,
    step_world_profiled,
    step_world_profiled_detailed,
    StepWorldBreakdown,
    StepWorldBreakdownDetailed,
};

use crate::common::mesh::{flatten_positions, generate_icosphere};

use self::terrain_types::{MeshOutput, TerrainOutput, TerrainParams};

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

pub(crate) fn build_terrain(seed: &str, terrain_params: TerrainParams) -> TerrainOutput {
    terrain::generate(seed, terrain_params)
}

pub(crate) fn step_erosion_automaton(
    state: &mut erosion::ErosionAutomatonState,
    budget_cells: u32,
) -> ErosionAutomatonBreakdown {
    terrain::step_async_erosion_automaton(state, budget_cells)
}
