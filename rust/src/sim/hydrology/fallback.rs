use super::*;
use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::hydrology::downstream_from_csr;

pub(super) fn run_river_fallback(
    world: &mut World,
    runoff: &[f32],
    state: Option<&mut ErosionAutomatonState>,
) {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 || world.mesh().nbr_offsets.len() != cell_count + 1 {
        return;
    }

    let previous_flux = vec![0.0; cell_count];
    let params = &world.control.geology_params;

    let mut rebuilt = build_river_network(
            &world.mesh().positions,
            &world.mesh().nbr_offsets,
            &world.mesh().nbrs,
        &world.state.geology.height,
        runoff,
        params,
        None,
    );

    let mut flux_scale_ema = 1.0;
    let mut scratch_flux_samples = Vec::with_capacity(rebuilt.flux.len() / 2);
    smooth_and_normalize_flux(
        &mut rebuilt.flux,
        &previous_flux,
        &mut flux_scale_ema,
        &mut scratch_flux_samples,
    );
    let mut constraint_buffers = RiverNetworkConstraintBuffers {
        flux: &mut rebuilt.flux,
        primary_next: &mut rebuilt.primary_next,
        downstream_offsets: &mut rebuilt.downstream_offsets,
        downstream_cells: &mut rebuilt.downstream_cells,
        downstream_weights: &mut rebuilt.downstream_weights,
    };
    apply_river_network_constraints(
        RiverNetworkConstraintInput {
            height: &world.state.geology.height,
            previous_flux: &previous_flux,
            accumulation_threshold: params.river_accumulation_threshold,
        },
        &mut constraint_buffers,
    );

    world.state.hydrology.river_next = rebuilt.primary_next;
    world.state.hydrology.river_flow = rebuilt.flux;
    world.state.hydrology.river_downstream = downstream_from_csr(
        world.state.hydrology.river_next.len(),
        &rebuilt.downstream_offsets,
        &rebuilt.downstream_cells,
        &rebuilt.downstream_weights,
    );
    world.state.hydrology.is_lake.fill(false);
    if let Some(state) = state {
        if state.river_flux.len() == world.state.hydrology.river_flow.len() {
            sync_erosion_rain(state, runoff);
            state.prev_river_next.clone_from(&state.river_next);
            state
                .river_flux
                .clone_from(&world.state.hydrology.river_flow);
            state
                .river_next
                .clone_from(&world.state.hydrology.river_next);
            state.height.clone_from(&world.state.geology.height);
            state.last_rebuild_tick = world.clock.tick;
            state.flux_scale_ema = 1.0;
            state.last_river_driver = 1.0;
        }
    }
}
