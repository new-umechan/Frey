use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::geology_types::GeologyInternal;
use crate::sim::world::{self, EraKind, World};
use crate::GeologyParams;

pub fn init_world_for_headless_runner(
    seed: &str,
    mesh_level: u32,
    geology_params: GeologyParams,
) -> Result<(World, ErosionAutomatonState), String> {
    if mesh_level > 8 {
        return Err("mesh_level must be between 0 and 8".to_string());
    }

    let (terrain, positions, nbr_offsets, nbrs) =
        crate::sim::build_geology_with_mesh(seed, geology_params.clone());
    if terrain.height.len() != positions.len() || terrain.plate_id.len() != positions.len() {
        return Err("terrain output does not match mesh vertex count".to_string());
    }

    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: terrain.lake_depth,
        plate_id: terrain.plate_id,
        erosion_rate: vec![0.0; positions.len()],
        deposition_rate: vec![0.0; positions.len()],
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); positions.len()],
        boundary_condition: vec![0.0; positions.len()],
    };
    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };

    let mut world = World::new(mesh, geology);
    world.state.hydrology.river_flow = terrain.river_flux;
    world.state.hydrology.river_next = terrain.river_next;
    crate::sim::hydrology::rebuild_mfd_from_primary(&mut world.state.hydrology);
    world.control.geology_params = geology_params.clone();
    world.control.erosion_thickness_coupling = geology_params.erosion_thickness_coupling;
    world.control.deposition_thickness_coupling = geology_params.deposition_thickness_coupling;
    world.clock.epoch = EraKind::Crust;

    let erosion_state = crate::sim::build_hydrology_state_for_bench(&world, geology_params);
    crate::sim::hydrology::apply_hydrology_state_view(&mut world, &erosion_state)?;
    Ok((world, erosion_state))
}
