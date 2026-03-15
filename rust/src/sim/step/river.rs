use std::cmp::Ordering;

use crate::domains;
use crate::sim::world::{EraKind, World};

use super::{
    CHANNEL_TRANSFER_BASE,
    CHANNEL_TRANSFER_MAX,
    CHANNEL_TRANSFER_SLOPE_GAIN,
    CRUST_RAIN_LAND,
    CRUST_RAIN_SEA,
    FLUX_LOCAL_DECAY,
    RUNOFF_ALTITUDE_GAIN,
    RUNOFF_BASE,
    RUNOFF_OCEAN_FACTOR,
    RUNOFF_RAIN_GAIN,
};

pub(super) fn run_river_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    if let Some(state) = world.exec.river_erosion_state.as_mut() {
        if state.height.len() == world.state.geology.height.len()
            && state.river_flux.len() == world.state.geology.river_flux.len()
            && state.river_next.len() == world.state.geology.river_next.len()
        {
            let cell_count = world.state.geology.height.len() as u32;
            let budget_cells = (cell_count.saturating_mul(budget).max(1) / 12).max(32);
            domains::step_erosion_automaton(state, budget_cells);
            world.state.geology.height.clone_from(&state.height);
            world.state.geology.erosion_rate.fill(0.0);
            world.state.geology.deposition_rate.fill(0.0);
        }
    }

    run_river_fallback(world);
}

fn run_river_fallback(world: &mut World) {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 || world.mesh.nbr_offsets.len() != cell_count + 1 {
        return;
    }

    let mut river_next = vec![-1_i32; cell_count];
    for (i, river_next_i) in river_next.iter_mut().enumerate() {
        let start = world.mesh.nbr_offsets[i] as usize;
        let end = world.mesh.nbr_offsets[i + 1] as usize;
        let mut best_downstream = None::<(usize, f32)>;
        for &n_u32 in &world.mesh.nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            let drop = world.state.geology.height[i] - world.state.geology.height[n];
            if drop <= 1e-5 {
                continue;
            }
            match best_downstream {
                Some((_, best_drop)) if drop <= best_drop => {}
                _ => best_downstream = Some((n, drop)),
            }
        }
        if let Some((n, _)) = best_downstream {
            *river_next_i = n as i32;
        }
    }

    let rain = build_rain_for_fallback(world);
    let mut flux = rain;
    let mut order = (0..cell_count).collect::<Vec<_>>();
    order.sort_by(|&a, &b| {
        world.state.geology.height[b]
            .partial_cmp(&world.state.geology.height[a])
            .unwrap_or(Ordering::Equal)
    });
    for i in order {
        let next = river_next[i];
        if next < 0 {
            continue;
        }
        let n = next as usize;
        if n < cell_count {
            flux[n] += flux[i];
        }
    }

    world.state.geology.river_next = river_next;
    world.state.geology.river_flux = flux.clone();
    if let Some(state) = world.exec.river_erosion_state.as_mut() {
        if state.river_flux.len() == flux.len() {
            state.river_flux.clone_from(&flux);
            state.river_next.clone_from(&world.state.geology.river_next);
            state.height.clone_from(&world.state.geology.height);
        }
    }
}

pub(super) fn build_rain_for_fallback(world: &World) -> Vec<f32> {
    if world.exec.era != EraKind::Crust {
        return world.state.climate.rain.clone();
    }
    world
        .state
        .geology
        .height
        .iter()
        .map(|&h| {
            if h > 0.0 {
                CRUST_RAIN_LAND
            } else {
                CRUST_RAIN_SEA
            }
        })
        .collect()
}

pub(super) fn route_river_flux(height: &[f32], river_next: &[i32], rain: &[f32]) -> Vec<f32> {
    let cell_count = height.len();
    let mut flux = vec![0.0; cell_count];
    let mut local_runoff = vec![0.0; cell_count];
    for i in 0..cell_count {
        let altitude = height[i].max(0.0);
        let land_factor = if height[i] > 0.0 {
            1.0
        } else {
            RUNOFF_OCEAN_FACTOR
        };
        let runoff_ratio =
            (RUNOFF_BASE + rain[i] * RUNOFF_RAIN_GAIN + altitude * RUNOFF_ALTITUDE_GAIN)
                .clamp(0.0, 0.35);
        local_runoff[i] = rain[i] * runoff_ratio * land_factor;
        flux[i] = local_runoff[i];
    }
    let mut order = (0..cell_count).collect::<Vec<_>>();
    order.sort_by(|&a, &b| height[b].partial_cmp(&height[a]).unwrap_or(Ordering::Equal));
    for i in order {
        let next = river_next.get(i).copied().unwrap_or(-1);
        if next < 0 {
            continue;
        }
        let n = next as usize;
        if n < cell_count {
            let drop = (height[i] - height[n]).max(0.0);
            let transfer = (CHANNEL_TRANSFER_BASE + drop * CHANNEL_TRANSFER_SLOPE_GAIN)
                .clamp(CHANNEL_TRANSFER_BASE, CHANNEL_TRANSFER_MAX);
            let carried =
                (flux[i] - local_runoff[i] * (1.0 - FLUX_LOCAL_DECAY)).max(0.0) * transfer;
            flux[n] += carried;
        }
    }
    flux
}
