use crate::sim;
use crate::sim::geology_types::{GeologyOutput, GeologyParams, MeshOutput};

pub fn generate_mesh(level: u32) -> Result<MeshOutput, String> {
    sim::build_mesh(level)
}

pub fn generate_geology(seed: &str, geology_params: GeologyParams) -> GeologyOutput {
    sim::build_geology(seed, geology_params)
}
