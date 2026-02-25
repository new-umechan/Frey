use crate::MeshOutput;
use crate::{TerrainOutput, TerrainParams};

#[path = "core/geom.rs"]
mod geom;
#[path = "core/mesh.rs"]
mod mesh;
#[path = "core/rng.rs"]
mod rng;
#[path = "core/terrain.rs"]
mod terrain;

use self::mesh::{flatten_positions, generate_icosphere};

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

pub(crate) fn build_terrain(seed: &str, params: TerrainParams) -> TerrainOutput {
    terrain::generate(seed, params)
}
