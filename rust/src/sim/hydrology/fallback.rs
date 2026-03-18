use super::*;

pub(super) fn run_river_fallback(world: &mut World, runoff: &[f32]) {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 || world.mesh.nbr_offsets.len() != cell_count + 1 {
        return;
    }

    let previous_flux = vec![0.0; cell_count];
    let default_params = GeologyParams::default();
    let params = world
        .exec
        .hydrology_dynamics
        .as_ref()
        .map(|state| &state.params)
        .unwrap_or(&default_params);

    let (mut flux, mut river_next, _) = build_river_network(
        &world.mesh.positions,
        &world.mesh.nbr_offsets,
        &world.mesh.nbrs,
        &world.state.geology.height,
        runoff,
        params,
        None,
    );

    let mut flux_scale_ema = 1.0;
    let mut scratch_flux_samples = Vec::with_capacity(flux.len() / 2);
    smooth_and_normalize_flux(
        &mut flux,
        &previous_flux,
        &mut flux_scale_ema,
        &mut scratch_flux_samples,
    );
    apply_river_network_constraints(
        &world.state.geology.height,
        &mut flux,
        &mut river_next,
        &previous_flux,
        params.river_accumulation_threshold,
    );

    world.state.hydrology.river_path = river_next;
    world.state.hydrology.river_flow = flux;
    if let Some(state) = world.exec.hydrology_dynamics.as_mut() {
        if state.river_flux.len() == world.state.hydrology.river_flow.len() {
            sync_erosion_rain(state, runoff);
            state.prev_river_next.clone_from(&state.river_next);
            state
                .river_flux
                .clone_from(&world.state.hydrology.river_flow);
            state
                .river_next
                .clone_from(&world.state.hydrology.river_path);
            state.height.clone_from(&world.state.geology.height);
            state.last_rebuild_tick = world.exec.tick;
            state.flux_scale_ema = 1.0;
            state.last_river_driver = 1.0;
        }
    }
}
