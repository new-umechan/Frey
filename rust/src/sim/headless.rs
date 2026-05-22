use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::geology_types::GeologyInternal;
use crate::sim::precomputed::{
    canonical_cache_dir, load_snapshot, mirror_dir, restore_world_from_snapshot, stage_filename,
    AlphaSnapshotStage,
};
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
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); positions.len()],
        boundary_condition: vec![0.0; positions.len()],
        smoothing_limited_cells_ratio: 0.0,
        mean_smoothing_factor: 1.0,
        zero_mean_adjusted_cells_ratio: 0.0,
        zero_mean_mean_abs_correction: 0.0,
        zero_mean_std_delta: 0.0,
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

pub fn init_world_for_headless_runner_from_alpha_snapshot(
    seed: &str,
    mesh_level: u32,
    geology_params: GeologyParams,
    stage: AlphaSnapshotStage,
) -> Result<(World, ErosionAutomatonState), String> {
    if seed != "alpha" {
        return Err("alpha snapshot restore is only supported for seed=alpha".to_string());
    }

    let filename = stage_filename(stage);
    let candidates = [
        canonical_cache_dir().join(&filename),
        mirror_dir().join(&filename),
        std::path::PathBuf::from("dist/.dev-precomputed/alpha").join(&filename),
    ];
    let expected_stage = Some(stage);
    let mut attempted = Vec::new();
    for path in candidates.into_iter().filter(|path| path.exists()) {
        match init_world_for_headless_runner_from_snapshot_path(
            seed,
            mesh_level,
            geology_params.clone(),
            &path,
            expected_stage,
        ) {
            Ok(result) => return Ok(result),
            Err(err) => attempted.push(format!("{}: {}", path.display(), err)),
        }
    }
    if attempted.is_empty() {
        Err(format!("snapshot not found for stage={stage}"))
    } else {
        Err(format!(
            "all snapshot candidates failed for stage={stage}: {}",
            attempted.join(" | ")
        ))
    }
}

pub fn init_world_for_headless_runner_from_snapshot_path(
    seed: &str,
    mesh_level: u32,
    geology_params: GeologyParams,
    path: &std::path::Path,
    expected_stage: Option<AlphaSnapshotStage>,
) -> Result<(World, ErosionAutomatonState), String> {
    if mesh_level > 8 {
        return Err("mesh_level must be between 0 and 8".to_string());
    }

    let envelope = load_snapshot(path)?;
    if envelope.seed != seed {
        return Err(format!(
            "snapshot seed mismatch: expected={}, actual={}",
            seed, envelope.seed
        ));
    }
    if envelope.mesh_level != mesh_level {
        return Err(format!(
            "snapshot mesh level mismatch: expected={}, actual={}",
            mesh_level, envelope.mesh_level
        ));
    }
    if let Some(stage) = expected_stage {
        if envelope.stage != stage {
            return Err(format!(
                "snapshot stage mismatch: expected={}, actual={}",
                stage, envelope.stage
            ));
        }
    }

    let (positions, indices) = crate::common::mesh::generate_icosphere(mesh_level);
    let (nbr_offsets, nbrs) = crate::common::mesh::build_neighbors(positions.len(), &indices);
    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };
    let geology = envelope.world_core.cells.geology.clone();
    let world = World::new(mesh, geology);
    let (mut world, erosion_state) = restore_world_from_snapshot(world, &envelope)?;
    world.control.geology_params = geology_params.clone();
    world.control.erosion_thickness_coupling = geology_params.erosion_thickness_coupling;
    world.control.deposition_thickness_coupling = geology_params.deposition_thickness_coupling;
    Ok((world, erosion_state))
}
