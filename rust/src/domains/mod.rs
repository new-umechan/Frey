use crate::common::mesh::{flatten_positions, generate_icosphere};
use crate::MeshOutput;
use crate::{ErosionAutomatonState, TerrainOutput, TerrainParams};

mod terrain;

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

pub(crate) fn step_erosion_automaton(state: &mut ErosionAutomatonState, budget_cells: u32) {
    terrain::step_async_erosion_automaton(state, budget_cells);
}
