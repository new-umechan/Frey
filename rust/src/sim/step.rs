use std::cmp::Ordering;
use std::collections::VecDeque;

use crate::domains;
use crate::TerrainParams;

use super::world::{
    era_for_tick, CellLayer, CivilizationLayer, ClimateLayer, EcologyLayer, EraKind, LayerKind,
    BoundaryDynamicsState, PlateKinematicsState, SubsystemBudgets, TerrainDynamicsState,
    VertexCrustState, World,
};

pub fn step_world(world: &mut World) {
    world.budgets = compute_budgets(world.era);
    ensure_required_layers(world);
    run_terrain_step(world, world.budgets.terrain);
    run_river_step(world, world.budgets.river);
    run_climate_step(world, world.budgets.climate);
    run_ecology_step(world, world.budgets.ecology);
    run_civilization_step(world, world.budgets.civilization);
    update_era_transition(world);
    world.tick = world.tick.saturating_add(1);
}

fn compute_budgets(era: EraKind) -> SubsystemBudgets {
    match era {
        EraKind::Crust => SubsystemBudgets {
            terrain: 4,
            river: 1,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        EraKind::Environment => SubsystemBudgets {
            terrain: 2,
            river: 4,
            climate: 3,
            ecology: 1,
            civilization: 0,
        },
        EraKind::Life => SubsystemBudgets {
            terrain: 1,
            river: 2,
            climate: 3,
            ecology: 4,
            civilization: 1,
        },
        EraKind::Civilization => SubsystemBudgets {
            terrain: 1,
            river: 1,
            climate: 2,
            ecology: 2,
            civilization: 4,
        },
        EraKind::History => SubsystemBudgets {
            terrain: 0,
            river: 1,
            climate: 1,
            ecology: 1,
            civilization: 4,
        },
    }
}

fn ensure_required_layers(world: &mut World) {
    let cell_count = world.core.height.len();
    if world.era == EraKind::Environment
        || world.era == EraKind::Life
        || world.era == EraKind::Civilization
        || world.era == EraKind::History
    {
        let layer = world.layers.entry(LayerKind::Climate).or_insert_with(|| {
            CellLayer::Climate(ClimateLayer {
                temp: vec![0.5; cell_count],
                rain: vec![0.5; cell_count],
            })
        });
        if let CellLayer::Climate(climate) = layer {
            resize_with_fill(&mut climate.temp, cell_count, 0.5);
            resize_with_fill(&mut climate.rain, cell_count, 0.5);
        }
    }
    if world.era == EraKind::Life
        || world.era == EraKind::Civilization
        || world.era == EraKind::History
    {
        let layer = world.layers.entry(LayerKind::Ecology).or_insert_with(|| {
            CellLayer::Ecology(EcologyLayer {
                habitability: vec![0.0; cell_count],
                productivity: vec![0.0; cell_count],
            })
        });
        if let CellLayer::Ecology(ecology) = layer {
            resize_with_fill(&mut ecology.habitability, cell_count, 0.0);
            resize_with_fill(&mut ecology.productivity, cell_count, 0.0);
        }
    }
    if world.era == EraKind::Civilization || world.era == EraKind::History {
        let layer = world
            .layers
            .entry(LayerKind::Civilization)
            .or_insert_with(|| {
                CellLayer::Civilization(CivilizationLayer {
                    population: vec![0.0; cell_count],
                    state_id: vec![0; cell_count],
                })
            });
        if let CellLayer::Civilization(civilization) = layer {
            resize_with_fill(&mut civilization.population, cell_count, 0.0);
            resize_with_fill(&mut civilization.state_id, cell_count, 0_u32);
        }
    }
}

fn run_terrain_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    if world.mesh.nbr_offsets.len() != world.core.height.len() + 1 {
        return;
    }
    if world.core.plate_id.len() != world.core.height.len() {
        return;
    }

    ensure_terrain_dynamics(world);
    let Some(dynamics) = world.terrain_dynamics.as_mut() else {
        return;
    };

    let mut current_height = world.core.height.clone();
    let mut next_height = vec![0.0; current_height.len()];
    let mut current_vertex_states = dynamics.vertex_states.clone();
    let mut next_vertex_states = current_vertex_states.clone();
    let target_sea_ratio = world.target_sea_ratio.clamp(0.08, 0.92);
    let tectonic_phase = world.tick as f32 * 0.042;
    let default_params = TerrainParams::default();
    let params = world
        .river_erosion_state
        .as_ref()
        .map(|state| &state.params)
        .unwrap_or(&default_params);
    let uplift_soft = params.uplift_saturation_soft.clamp(0.0, 1.0);
    let uplift_hard = params
        .uplift_saturation_hard
        .max(uplift_soft + 1e-3)
        .clamp(0.0, 1.0);

    for _ in 0..budget {
        if dynamics.tick_internal.saturating_sub(dynamics.boundary_state.last_reclassify_tick)
            >= dynamics.boundary_state.reclassify_interval_ticks as u64
        {
            dynamics.boundary_state.last_reclassify_tick = dynamics.tick_internal;
        }

        for i in 0..world.core.height.len() {
            let start = world.mesh.nbr_offsets[i] as usize;
            let end = world.mesh.nbr_offsets[i + 1] as usize;
            let mut vertex_state_next = current_vertex_states[i];
            let height_i = current_height[i];
            next_height[i] = height_i;
            if end <= start {
                next_vertex_states[i] = vertex_state_next;
                continue;
            }

            let mut nbr_sum = 0.0_f32;
            let mut nbr_count = 0usize;
            let mut same_plate_count = 0usize;
            let mut convergent_strength = 0.0_f32;
            let mut divergent_strength = 0.0_f32;
            let mut shear_strength = 0.0_f32;
            let mut max_drop = 0.0_f32;
            let mut mean_abs_slope = 0.0_f32;
            let pos_i = world.mesh.positions[i];
            let plate_i = world.core.plate_id[i];
            let vel_i = plate_velocity_from_state(
                dynamics.plate_states.get(plate_i as usize),
                plate_i,
                pos_i,
                tectonic_phase,
            );
            for &n_u32 in &world.mesh.nbrs[start..end] {
                let n = n_u32 as usize;
                if n >= world.core.height.len() {
                    continue;
                }
                nbr_sum += current_height[n];
                nbr_count += 1;
                let h_drop = current_height[i] - current_height[n];
                if h_drop > max_drop {
                    max_drop = h_drop;
                }
                mean_abs_slope += h_drop.abs();

                let same_plate = world.core.plate_id.get(i) == world.core.plate_id.get(n);
                if same_plate {
                    same_plate_count += 1;
                }

                let pos_n = world.mesh.positions[n];
                let edge_vec = [
                    pos_n[0] - pos_i[0],
                    pos_n[1] - pos_i[1],
                    pos_n[2] - pos_i[2],
                ];
                let edge_len = length3(edge_vec).max(1e-5);
                let edge_dir = [
                    edge_vec[0] / edge_len,
                    edge_vec[1] / edge_len,
                    edge_vec[2] / edge_len,
                ];
                let vel_n = plate_velocity_from_state(
                    dynamics.plate_states.get(world.core.plate_id[n] as usize),
                    world.core.plate_id[n],
                    pos_n,
                    tectonic_phase,
                );
                let rel_v = [
                    vel_n[0] - vel_i[0],
                    vel_n[1] - vel_i[1],
                    vel_n[2] - vel_i[2],
                ];
                let rel_n = dot(rel_v, edge_dir);
                let rel_mag = length3(rel_v);

                if same_plate {
                    shear_strength += (rel_mag * 0.55).min(0.8);
                } else if rel_n > 0.0 {
                    convergent_strength += rel_n;
                    shear_strength += (rel_mag * 0.30).min(0.4);
                } else {
                    divergent_strength += -rel_n;
                }
            }
            if nbr_count == 0 {
                continue;
            }

            let avg_nbr = nbr_sum / nbr_count as f32;
            let plate_similarity = same_plate_count as f32 / nbr_count as f32;
            let boundary_strength = (1.0 - plate_similarity).clamp(0.0, 1.0);
            let inland_strength = plate_similarity.clamp(0.0, 1.0);
            let land = if height_i > 0.0 { 1.0 } else { 0.0 };
            let ridge_anchor = (height_i.max(0.0) / 0.55).clamp(0.0, 1.0);
            let slope = (max_drop / 0.18).clamp(0.0, 1.0);
            mean_abs_slope /= nbr_count as f32;

            let conv = convergent_strength / nbr_count as f32;
            let div = divergent_strength / nbr_count as f32;
            let shear = shear_strength / nbr_count as f32;
            let uplift_cap = 1.0 - smoothstep(uplift_soft, uplift_hard, height_i.max(0.0));
            let tectonic_gain = params.tectonic_uplift_gain.max(0.0);
            let uplift = tectonic_gain * conv * (0.40 + 0.60 * land) * uplift_cap;
            let subsidence = 0.046 * div * (0.70 + 0.30 * (1.0 - land));
            let intraplate_fold =
                0.022 * shear * inland_strength * (0.35 + 0.65 * (1.0 - ridge_anchor)) * uplift_cap;

            let prev_memory = current_vertex_states[i].uplift_memory;
            let next_memory = lerp(prev_memory, conv - div, 0.22).clamp(-1.0, 1.0);
            vertex_state_next.uplift_memory = next_memory;
            let memory_term = 0.010 * next_memory * (1.0 - ridge_anchor);

            let slope_drive = (mean_abs_slope / 0.12).clamp(0.0, 1.0);
            let diffusion = params.nonlinear_diffusion_gain.max(0.0)
                * (avg_nbr - height_i)
                * (0.40 + 0.60 * slope_drive)
                * (0.35 + 0.65 * boundary_strength);

            let isostatic = -params.isostatic_relax_gain.max(0.0)
                * (height_i - uplift_soft).max(0.0)
                * (0.35 + 0.65 * inland_strength);

            let mut marine_subsidence = 0.0_f32;
            if current_vertex_states[i].is_ocean_cell != 0 {
                let age_i = current_vertex_states[i].ocean_age_norm;
                let rejuvenation = (0.90 * div + 0.22 * boundary_strength).clamp(0.0, 1.5);
                let aging = (0.55 * conv + 0.12 * inland_strength + 0.10 * slope).clamp(0.0, 1.5);
                let age_next =
                    (age_i + params.age_advection_gain * (aging - rejuvenation)).clamp(0.0, 1.0);
                vertex_state_next.ocean_age_norm = age_next;
                let target = (-0.03 - 0.20 * age_next).clamp(-1.0, 0.08);
                vertex_state_next.target_buoyancy = target;
                marine_subsidence = params.marine_subsidence_gain.max(0.0) * (target - height_i);
            } else {
                vertex_state_next.ocean_age_norm = 0.0;
            }

            let tectonic = uplift + intraplate_fold - subsidence + memory_term;
            next_height[i] =
                (height_i + tectonic + marine_subsidence + diffusion + isostatic).clamp(-1.0, 1.0);
            next_vertex_states[i] = vertex_state_next;
        }
        std::mem::swap(&mut current_vertex_states, &mut next_vertex_states);
        dynamics.tick_internal = dynamics.tick_internal.saturating_add(1);
        preserve_target_sea_ratio(&mut next_height, target_sea_ratio, 0.56);
        std::mem::swap(&mut current_height, &mut next_height);
    }
    dynamics.vertex_states = current_vertex_states;
    world.core.height.clone_from(&current_height);

    if let Some(state) = world.river_erosion_state.as_mut() {
        if state.height.len() == world.core.height.len() {
            state.height.clone_from(&world.core.height);
        }
    }
}

fn ensure_terrain_dynamics(world: &mut World) {
    let cell_count = world.core.height.len();
    let needs_rebuild = match world.terrain_dynamics.as_ref() {
        Some(state) => {
            state.vertex_states.len() != cell_count
        }
        None => true,
    };
    if !needs_rebuild {
        return;
    }

    let mut vertex_states = vec![
        VertexCrustState {
            ocean_age_norm: 0.0,
            uplift_memory: 0.0,
            is_ocean_cell: 0,
            target_buoyancy: 0.0,
        };
        cell_count
    ];
    let plate_states = build_plate_states(&world.core.plate_id);

    for (i, h) in world.core.height.iter().copied().enumerate() {
        if h <= 0.0 {
            vertex_states[i].is_ocean_cell = 1;
        }
    }

    if cell_count > 0 && world.mesh.nbr_offsets.len() == cell_count + 1 {
        let mut dist = vec![u32::MAX; cell_count];
        let mut queue = VecDeque::new();

        for i in 0..cell_count {
            if vertex_states[i].is_ocean_cell == 0 {
                continue;
            }
            let start = world.mesh.nbr_offsets[i] as usize;
            let end = world.mesh.nbr_offsets[i + 1] as usize;
            let is_boundary_seed = world.mesh.nbrs[start..end].iter().any(|&n_u32| {
                    let n = n_u32 as usize;
                n < cell_count
                    && (vertex_states[n].is_ocean_cell == 0
                        || world.core.plate_id[n] != world.core.plate_id[i])
            });
            if is_boundary_seed {
                dist[i] = 0;
                queue.push_back(i);
            }
        }

        while let Some(v) = queue.pop_front() {
            let next_dist = dist[v].saturating_add(1);
            let start = world.mesh.nbr_offsets[v] as usize;
            let end = world.mesh.nbr_offsets[v + 1] as usize;
            for &n_u32 in &world.mesh.nbrs[start..end] {
                let n = n_u32 as usize;
                if n >= cell_count || vertex_states[n].is_ocean_cell == 0 {
                    continue;
                }
                if next_dist < dist[n] {
                    dist[n] = next_dist;
                    queue.push_back(n);
                }
            }
        }

        let max_dist = dist
            .iter()
            .copied()
            .filter(|d| *d != u32::MAX)
            .max()
            .unwrap_or(1) as f32;
        let max_dist = max_dist.max(1.0);

        for i in 0..cell_count {
            if vertex_states[i].is_ocean_cell == 0 {
                vertex_states[i].target_buoyancy = world.core.height[i].max(0.0);
                continue;
            }
            let age = if dist[i] == u32::MAX {
                0.5
            } else {
                (dist[i] as f32 / max_dist).clamp(0.0, 1.0)
            };
            vertex_states[i].ocean_age_norm = age;
            vertex_states[i].target_buoyancy = (-0.03 - 0.20 * age).clamp(-1.0, 0.08);
        }
    }

    world.terrain_dynamics = Some(TerrainDynamicsState {
        tick_internal: 0,
        plate_states,
        vertex_states,
        boundary_state: BoundaryDynamicsState {
            reclassify_interval_ticks: 8,
            last_reclassify_tick: 0,
        },
    });
}

fn build_plate_states(plate_ids: &[u16]) -> Vec<PlateKinematicsState> {
    let plate_count = plate_ids
        .iter()
        .copied()
        .max()
        .map(|v| v as usize + 1)
        .unwrap_or(0);
    let mut plate_states = Vec::with_capacity(plate_count);
    for plate in 0..plate_count {
        let seed = plate as u32;
        plate_states.push(PlateKinematicsState {
            angular_axis: seeded_axis(seed ^ 0x27d4_eb2f),
            angular_speed: 0.08 + 0.18 * hash01(seed ^ 0xc2b2_ae35),
            phase_offset: std::f32::consts::TAU * hash01(seed ^ 0x85eb_ca6b),
            activity: (0.55 + 0.45 * hash01(seed ^ 0x9e37_79b9)).clamp(0.0, 1.0),
        });
    }
    plate_states
}

fn run_river_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    if let Some(state) = world.river_erosion_state.as_mut() {
        if state.height.len() == world.core.height.len()
            && state.river_flux.len() == world.core.river_flux.len()
            && state.river_next.len() == world.core.river_next.len()
        {
            let cell_count = world.core.height.len() as u32;
            let budget_cells = (cell_count.saturating_mul(budget).max(1) / 12).max(32);
            domains::step_erosion_automaton(state, budget_cells);
            world.core.height.clone_from(&state.height);
            world.core.river_flux.clone_from(&state.river_flux);
            world.core.river_next.clone_from(&state.river_next);
            return;
        }
    }

    run_river_fallback(world);
}

fn run_river_fallback(world: &mut World) {
    let cell_count = world.core.height.len();
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
            let drop = world.core.height[i] - world.core.height[n];
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
        world.core.height[b]
            .partial_cmp(&world.core.height[a])
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

    world.core.river_next = river_next;
    world.core.river_flux = flux;
}

fn build_rain_for_fallback(world: &World) -> Vec<f32> {
    if let Some(CellLayer::Climate(climate)) = world.layers.get(&LayerKind::Climate) {
        if climate.rain.len() == world.core.height.len() {
            return climate.rain.clone();
        }
    }
    world
        .core
        .height
        .iter()
        .map(|&h| if h > 0.0 { 0.65 } else { 0.35 })
        .collect()
}

fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let Some(CellLayer::Climate(climate)) = world.layers.get_mut(&LayerKind::Climate) else {
        return;
    };
    let alpha = blend_alpha(budget, 0.10);
    let max_flux = world
        .core
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.core.height.len() {
        let pos = world
            .mesh
            .positions
            .get(i)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let latitude = pos[1].abs().clamp(0.0, 1.0);
        let altitude = world.core.height[i].max(0.0);
        let base_temp = 0.15 + (1.0 - latitude) * 0.85;
        let target_temp = (base_temp - altitude * 0.35).clamp(0.0, 1.0);

        let river_norm = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let orographic = (altitude * 0.50).clamp(0.0, 0.35);
        let target_rain =
            (0.20 + river_norm * 0.45 + (1.0 - latitude) * 0.25 + orographic).clamp(0.0, 1.0);

        climate.temp[i] = lerp(climate.temp[i], target_temp, alpha);
        climate.rain[i] = lerp(climate.rain[i], target_rain, alpha);
    }
}

fn run_ecology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let (climate_temp, climate_rain) = match world.layers.get(&LayerKind::Climate) {
        Some(CellLayer::Climate(climate))
            if climate.temp.len() == world.core.height.len()
                && climate.rain.len() == world.core.height.len() =>
        {
            (climate.temp.clone(), climate.rain.clone())
        }
        _ => return,
    };
    let Some(CellLayer::Ecology(ecology)) = world.layers.get_mut(&LayerKind::Ecology) else {
        return;
    };
    if ecology.habitability.len() != world.core.height.len()
        || ecology.productivity.len() != world.core.height.len()
    {
        return;
    }
    let alpha = blend_alpha(budget, 0.16);
    let max_flux = world
        .core
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.core.height.len() {
        let temp = climate_temp[i];
        let rain = climate_rain[i];
        let land = if world.core.height[i] > 0.0 {
            1.0
        } else {
            0.15
        };
        let river_bonus = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0) * 0.20;
        let temp_suit = 1.0 - ((temp - 0.55).abs() / 0.55).clamp(0.0, 1.0);
        let rain_suit = 1.0 - ((rain - 0.60).abs() / 0.60).clamp(0.0, 1.0);
        let target_habitability =
            ((temp_suit * 0.55 + rain_suit * 0.45) * land + river_bonus).clamp(0.0, 1.0);
        let target_productivity =
            (target_habitability * (0.45 + rain * 0.40 + river_bonus)).clamp(0.0, 1.0);

        ecology.habitability[i] = lerp(ecology.habitability[i], target_habitability, alpha);
        ecology.productivity[i] = lerp(ecology.productivity[i], target_productivity, alpha);
    }
}

fn run_civilization_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let (eco_habitability, eco_productivity) = match world.layers.get(&LayerKind::Ecology) {
        Some(CellLayer::Ecology(ecology))
            if ecology.habitability.len() == world.core.height.len()
                && ecology.productivity.len() == world.core.height.len() =>
        {
            (ecology.habitability.clone(), ecology.productivity.clone())
        }
        _ => return,
    };
    let Some(CellLayer::Civilization(civilization)) =
        world.layers.get_mut(&LayerKind::Civilization)
    else {
        return;
    };
    if civilization.population.len() != world.core.height.len()
        || civilization.state_id.len() != world.core.height.len()
    {
        return;
    }
    let alpha = blend_alpha(budget, 0.12);
    let max_flux = world
        .core
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.core.height.len() {
        if world.core.height[i] <= 0.0 {
            civilization.population[i] *= 0.98;
            civilization.state_id[i] = 0;
            continue;
        }

        let river_support = (world.core.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let carrying =
            1.0 + eco_productivity[i] * 130.0 + eco_habitability[i] * 70.0 + river_support * 40.0;
        let current = civilization.population[i].max(0.0);
        let seeded = if current < 1.0 && eco_habitability[i] > 0.55 {
            1.0
        } else {
            current
        };
        let growth =
            0.18 * eco_habitability[i].max(0.05) * seeded * (1.0 - seeded / carrying).max(-0.5);
        let next_population = (seeded + growth * alpha * 4.0).max(0.0);
        civilization.population[i] = next_population;
        civilization.state_id[i] = if next_population >= 10.0 {
            (i + 1) as u32
        } else {
            0
        };
    }
}

fn update_era_transition(world: &mut World) {
    let next_tick = world.tick.saturating_add(1);
    world.era = era_for_tick(next_tick);
}

fn resize_with_fill<T: Clone>(values: &mut Vec<T>, size: usize, fill: T) {
    if values.len() != size {
        values.resize(size, fill);
    }
}

fn blend_alpha(budget: u32, base: f32) -> f32 {
    let b = budget.max(1) as f32;
    (1.0 - (1.0 - base).powf(b)).clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn preserve_target_sea_ratio(height: &mut [f32], target_sea_ratio: f32, strength: f32) {
    if height.is_empty() {
        return;
    }

    let mut sorted = height.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let sea_idx = ((sorted.len() as f32) * target_sea_ratio.clamp(0.02, 0.98)) as usize;
    let sea_idx = sea_idx.min(sorted.len().saturating_sub(1));
    let sea_level = sorted[sea_idx];
    let shift = sea_level * strength.clamp(0.0, 1.0);

    for h in height.iter_mut() {
        *h = (*h - shift).clamp(-1.0, 1.0);
    }
}

fn plate_velocity_from_state(
    state: Option<&PlateKinematicsState>,
    plate_id: u16,
    pos: [f32; 3],
    phase: f32,
) -> [f32; 3] {
    let seed = plate_id as u32;
    let fallback_axis = seeded_axis(seed ^ 0x27d4_eb2f);
    let angular_axis = state.map(|s| s.angular_axis).unwrap_or(fallback_axis);
    let angular_speed = state
        .map(|s| s.angular_speed * (0.55 + 0.45 * s.activity))
        .unwrap_or(0.12);
    let phase_offset = state.map(|s| s.phase_offset).unwrap_or(0.0);
    let omega = [
        angular_axis[0] * angular_speed,
        angular_axis[1] * angular_speed,
        angular_axis[2] * angular_speed,
    ];
    let mut base = cross3(omega, pos);
    let axis_b = seeded_axis(seed ^ 0x1656_67b1);
    let pulse = (dot(pos, axis_b) * 7.0 + phase + phase_offset).sin();
    base[0] += 0.03 * pulse;
    base[1] -= 0.02 * pulse;
    base[2] += 0.01 * pulse;
    base
}

fn seeded_axis(seed: u32) -> [f32; 3] {
    let z = 2.0 * hash01(seed ^ 0x7feb_352d) - 1.0;
    let phi = std::f32::consts::TAU * hash01(seed ^ 0x846c_a68b);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}

fn hash01(seed: u32) -> f32 {
    let s = ((seed as f32) * 12.9898 + 78.233).sin();
    fract01(s * 43_758.547)
}

fn fract01(v: f32) -> f32 {
    v - v.floor()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use crate::{ErosionAutomatonState, TerrainParams};

    use super::*;
    use crate::sim::world::{CoreCells, World, WorldMesh};

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                [0.0, 0.8, 0.6],
                [0.7, 0.2, 0.6],
                [0.4, -0.7, 0.6],
                [-0.6, -0.1, 0.8],
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let core = CoreCells {
            height: vec![0.45, 0.15, -0.25, 0.05],
            plate_id: vec![0, 0, 1, 1],
            river_flux: vec![0.1, 0.2, 0.3, 0.1],
            river_next: vec![1, 2, -1, 2],
        };
        World::new(mesh, core)
    }

    fn build_ocean_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                [0.0, 0.8, 0.6],
                [0.7, 0.2, 0.6],
                [0.4, -0.7, 0.6],
                [-0.6, -0.1, 0.8],
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let core = CoreCells {
            height: vec![0.55, -0.35, -0.12, 0.10],
            plate_id: vec![0, 1, 1, 0],
            river_flux: vec![0.1, 0.2, 0.3, 0.1],
            river_next: vec![1, 2, -1, 2],
        };
        World::new(mesh, core)
    }

    #[test]
    fn step_world_advances_tick_and_sets_budgets() {
        let mut world = build_test_world();
        step_world(&mut world);
        assert_eq!(world.tick, 1);
        assert_eq!(world.budgets.terrain, 4);
        assert_eq!(world.budgets.river, 1);
    }

    #[test]
    fn terrain_step_preserves_initial_sea_ratio() {
        let mut world = build_test_world();
        let base_ratio = world.core.height.iter().filter(|&&h| h <= 0.0).count() as f32
            / world.core.height.len() as f32;
        world.target_sea_ratio = base_ratio;

        for _ in 0..24 {
            run_terrain_step(&mut world, 4);
        }

        let ratio = world.core.height.iter().filter(|&&h| h <= 0.0).count() as f32
            / world.core.height.len() as f32;
        assert!((ratio - base_ratio).abs() <= 0.28);
    }

    #[test]
    fn plate_velocity_varies_inside_same_plate() {
        let plates = build_plate_states(&[0, 1, 2, 3, 3]);
        let a = plate_velocity_from_state(plates.get(3), 3, [0.0, 0.8, 0.6], 0.0);
        let b = plate_velocity_from_state(plates.get(3), 3, [0.7, 0.2, 0.6], 0.0);
        let diff = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
        assert!(diff > 1e-3);
    }

    #[test]
    fn step_world_generates_required_layers_by_era() {
        let mut world = build_test_world();
        world.era = EraKind::Life;
        step_world(&mut world);

        assert!(matches!(
            world.layers.get(&LayerKind::Climate),
            Some(CellLayer::Climate(_))
        ));
        assert!(matches!(
            world.layers.get(&LayerKind::Ecology),
            Some(CellLayer::Ecology(_))
        ));
        assert!(world.layers.get(&LayerKind::Civilization).is_none());
    }

    #[test]
    fn river_fallback_updates_flux_and_next() {
        let mut world = build_test_world();
        world.river_erosion_state = None;
        step_world(&mut world);

        assert!(world
            .core
            .river_flux
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0));
        assert!(world.core.river_next.iter().any(|&n| n >= 0));
    }

    #[test]
    fn river_erosion_state_is_used_when_present() {
        let mut world = build_test_world();
        let erosion_state = ErosionAutomatonState {
            positions: world.mesh.positions.clone(),
            nbr_offsets: world.mesh.nbr_offsets.clone(),
            nbrs: world.mesh.nbrs.clone(),
            height: world.core.height.clone(),
            water: vec![0.0; world.core.height.len()],
            sediment: vec![0.0; world.core.height.len()],
            armor: vec![0.0; world.core.height.len()],
            rain: vec![0.5; world.core.height.len()],
            river_flux: world.core.river_flux.clone(),
            river_next: world.core.river_next.clone(),
            active_queue: (0..world.core.height.len() as u32).collect(),
            active_head: 0,
            in_queue: vec![1; world.core.height.len()],
            rain_cursor: 0,
            tick: 0,
            recent_changed: Vec::new(),
            params: TerrainParams::default(),
        };
        world.attach_river_erosion_state(erosion_state).unwrap();

        step_world(&mut world);

        let tick = world
            .river_erosion_state
            .as_ref()
            .map(|s| s.tick)
            .unwrap_or(0);
        assert_eq!(tick, 1);
    }

    #[test]
    fn mountain_growth_is_saturated_over_long_ticks() {
        let mut world = build_test_world();
        world.core.height[0] = 0.82;
        let initial_high = world.core.height.iter().filter(|&&h| h > 0.8).count();

        for _ in 0..320 {
            run_terrain_step(&mut world, 4);
        }

        let final_high = world.core.height.iter().filter(|&&h| h > 0.8).count();
        assert!(final_high <= initial_high + 1);
        let max_h = world.core.height.iter().copied().fold(-1.0_f32, f32::max);
        assert!(max_h <= 0.95);
    }

    #[test]
    fn ocean_cells_do_not_flatten_too_fast() {
        let mut world = build_ocean_test_world();
        let initial_var = ocean_height_variance(&world);

        for _ in 0..200 {
            run_terrain_step(&mut world, 4);
        }

        let final_var = ocean_height_variance(&world);
        assert!(initial_var.is_finite());
        assert!(final_var.is_finite());
        assert!(final_var >= 0.0);
    }

    #[test]
    fn marine_subsidence_tracks_age_target() {
        let mut world = build_ocean_test_world();
        run_terrain_step(&mut world, 1);
        let initial_error = mean_ocean_target_error(&world);

        for _ in 0..160 {
            run_terrain_step(&mut world, 4);
        }

        let final_error = mean_ocean_target_error(&world);
        assert!(initial_error.is_finite());
        assert!(final_error.is_finite());
        assert!(final_error <= 1.2);
        assert!(has_ocean_dynamics(&world));
    }

    #[test]
    fn terrain_dynamics_contains_plate_and_boundary_state() {
        let mut world = build_ocean_test_world();
        run_terrain_step(&mut world, 1);
        let dynamics = world
            .terrain_dynamics
            .as_ref()
            .expect("terrain dynamics should be initialized");
        assert!(!dynamics.plate_states.is_empty());
        assert_eq!(dynamics.vertex_states.len(), world.core.height.len());
        assert!(dynamics.boundary_state.reclassify_interval_ticks >= 1);
    }

    fn ocean_height_variance(world: &World) -> f32 {
        let ocean = world
            .core
            .height
            .iter()
            .copied()
            .filter(|h| *h <= 0.0)
            .collect::<Vec<_>>();
        if ocean.len() <= 1 {
            return 0.0;
        }
        let mean = ocean.iter().sum::<f32>() / ocean.len() as f32;
        ocean
            .iter()
            .map(|h| {
                let d = *h - mean;
                d * d
            })
            .sum::<f32>()
            / ocean.len() as f32
    }

    fn mean_ocean_target_error(world: &World) -> f32 {
        let Some(dyn_state) = world.terrain_dynamics.as_ref() else {
            return 1.0;
        };
        let mut sum = 0.0_f32;
        let mut count = 0usize;
        for i in 0..world.core.height.len() {
            if dyn_state.vertex_states[i].is_ocean_cell == 0 {
                continue;
            }
            sum += (world.core.height[i] - dyn_state.vertex_states[i].target_buoyancy).abs();
            count += 1;
        }
        if count == 0 {
            return 0.0;
        }
        sum / count as f32
    }

    fn has_ocean_dynamics(world: &World) -> bool {
        let Some(dyn_state) = world.terrain_dynamics.as_ref() else {
            return false;
        };
        dyn_state.vertex_states.iter().any(|state| {
            state.is_ocean_cell != 0 && state.target_buoyancy.is_finite()
        })
    }
}
