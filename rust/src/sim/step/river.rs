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
};

const RIVER_RUNOFF_SCALE_MM: f32 = 1_200.0;

pub(super) fn run_river_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let runoff = build_runoff_for_routing(world);

    if let Some(state) = world.exec.river_erosion_state.as_mut() {
        if state.height.len() == world.state.geology.height.len()
            && state.river_flux.len() == world.state.geology.river_flux.len()
            && state.river_next.len() == world.state.geology.river_next.len()
        {
            sync_erosion_rain(state, &runoff);
            let cell_count = world.state.geology.height.len() as u32;
            let budget_cells = (cell_count.saturating_mul(budget).max(1) / 12).max(32);
            domains::step_erosion_automaton(state, budget_cells);
            world.state.geology.height.clone_from(&state.height);
            world.state.geology.erosion_rate.fill(0.0);
            world.state.geology.deposition_rate.fill(0.0);
        }
    }

    run_river_fallback(world, &runoff);
}

fn run_river_fallback(world: &mut World, runoff: &[f32]) {
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

    let flux = route_river_flux(&world.state.geology.height, &river_next, &runoff);

    world.state.geology.river_next = river_next;
    world.state.geology.river_flux = flux.clone();
    if let Some(state) = world.exec.river_erosion_state.as_mut() {
        if state.river_flux.len() == flux.len() {
            sync_erosion_rain(state, runoff);
            state.river_flux.clone_from(&flux);
            state.river_next.clone_from(&world.state.geology.river_next);
            state.height.clone_from(&world.state.geology.height);
        }
    }
}

pub(super) fn build_runoff_for_routing(world: &World) -> Vec<f32> {
    if world.exec.era != EraKind::Crust {
        return world
            .state
            .climate
            .runoff
            .iter()
            .copied()
            .map(normalize_runoff_mm)
            .collect();
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

pub(super) fn route_river_flux(height: &[f32], river_next: &[i32], runoff: &[f32]) -> Vec<f32> {
    let cell_count = height.len();
    let mut flux = vec![0.0; cell_count];
    let mut local_runoff = vec![0.0; cell_count];
    for i in 0..cell_count {
        local_runoff[i] = runoff.get(i).copied().unwrap_or(0.0).max(0.0);
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

fn normalize_runoff_mm(runoff_mm: f32) -> f32 {
    (runoff_mm.max(0.0) / RIVER_RUNOFF_SCALE_MM).clamp(0.0, 1.0)
}

fn sync_erosion_rain(state: &mut crate::ErosionAutomatonState, runoff: &[f32]) {
    if state.rain.len() != runoff.len() {
        return;
    }
    for (dst, src) in state.rain.iter_mut().zip(runoff.iter().copied()) {
        *dst = src.max(0.0);
    }
}
