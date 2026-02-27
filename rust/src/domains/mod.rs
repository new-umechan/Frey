use crate::common::mesh::{flatten_positions, generate_icosphere};
use crate::MeshOutput;
use crate::{ErosionAutomatonState, TerrainOutput, TerrainParams};

mod terrain;

pub(crate) use self::terrain::CrustTerrainUpdateState;

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

pub(crate) fn init_crust_terrain_update(
    seed: &str,
    mut terrain_params: TerrainParams,
) -> CrustTerrainUpdateState {
    terrain::sanitize_params(&mut terrain_params);
    terrain::init_crust_update_state(seed, terrain_params)
}

pub(crate) fn step_crust_terrain_update(state: &mut CrustTerrainUpdateState, budget_ticks: u32) {
    terrain::step_crust_update_budget(state, budget_ticks);
}

pub(crate) fn crust_terrain_update_is_done(state: &CrustTerrainUpdateState) -> bool {
    terrain::crust_update_is_done(state)
}

pub(crate) fn crust_terrain_update_phase_name(state: &CrustTerrainUpdateState) -> &'static str {
    terrain::crust_update_phase_name(state)
}

pub(crate) fn finish_crust_terrain_update(state: CrustTerrainUpdateState) -> TerrainOutput {
    terrain::finalize_crust_update_state(state)
}

pub(crate) fn build_erosion_automaton(
    seed: &str,
    terrain_params: TerrainParams,
) -> ErosionAutomatonState {
    terrain::init_async_erosion_automaton(seed, terrain_params)
}

pub(crate) fn step_erosion_automaton(state: &mut ErosionAutomatonState, budget_cells: u32) {
    terrain::step_async_erosion_automaton(state, budget_cells);
}
