use std::cmp::Ordering;

use crate::domains;
use crate::TerrainParams;

use super::world::{
    era_for_tick, BoundaryDynamicsState, BoundaryType, CellLayer, CivilizationLayer, ClimateLayer,
    CrustType, EcologyLayer, EraKind, LayerKind, PlateKinematicsState, StressTensor,
    SubsystemBudgets, TerrainDynamicsState, TerrainStepMetrics, VertexCrustState, World,
};

const MAX_HEIGHT_DELTA_PER_STEP: f32 = 0.020;
const DEFAULT_DIFFUSION_WEIGHT: f32 = 0.06;
const CONVERGENT_THRESHOLD: f32 = 0.010;
const DIVERGENT_THRESHOLD: f32 = 0.010;
const TRANSFORM_THRESHOLD: f32 = 0.014;

pub fn step_world(world: &mut World) {
    world.budgets = compute_budgets(world.era);
    ensure_required_layers(world);
    run_terrain_step(world);
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
            terrain: 1,
            river: 1,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        EraKind::Environment => SubsystemBudgets {
            terrain: 1,
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
            terrain: 1,
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

fn run_terrain_step(world: &mut World) {
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

    let cell_count = world.core.height.len();
    let default_params = TerrainParams::default();
    let params = world
        .river_erosion_state
        .as_ref()
        .map(|state| &state.params)
        .unwrap_or(&default_params);

    if dynamics.vertex_states.len() != cell_count {
        return;
    }
    if dynamics.mantle_heat.len() != cell_count {
        dynamics.mantle_heat = vec![0.5; cell_count];
    }
    if dynamics.boundary_state.dominant_type.len() != cell_count {
        dynamics.boundary_state.dominant_type = vec![BoundaryType::PassiveMargin; cell_count];
    }
    if dynamics.boundary_state.activity.len() != cell_count {
        dynamics.boundary_state.activity = vec![0.0; cell_count];
    }

    let heights = world.core.height.clone();
    let plate_id = world.core.plate_id.clone();
    let positions = world.mesh.positions.clone();
    let nbr_offsets = world.mesh.nbr_offsets.clone();
    let nbrs = world.mesh.nbrs.clone();

    let plume_force = update_mantle_heat_and_plumes(
        &mut dynamics.mantle_heat,
        &dynamics.vertex_states,
        &nbr_offsets,
        &nbrs,
        params,
    );

    let reclassify_interval = params.boundary_reclassify_interval.max(1);
    dynamics.boundary_state.reclassify_interval_ticks = reclassify_interval;
    if dynamics.boundary_state.steps_since_reclassify >= reclassify_interval
        || dynamics.boundary_state.steps_since_reclassify == 0
    {
        reclassify_boundaries(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &dynamics.plate_states,
            &dynamics.vertex_states,
            &mut dynamics.boundary_state,
            params,
        );
        dynamics.boundary_state.steps_since_reclassify = 1;
    } else {
        for v in &mut dynamics.boundary_state.activity {
            *v *= 0.97;
        }
        dynamics.boundary_state.steps_since_reclassify = dynamics
            .boundary_state
            .steps_since_reclassify
            .saturating_add(1);
    }

    update_plate_kinematics(
        &plate_id,
        &mut dynamics.plate_states,
        &dynamics.boundary_state,
        params,
    );

    let mut next_height = heights.clone();
    let mut next_vertex_states = dynamics.vertex_states.clone();
    let metrics = apply_stress_and_surface_update(
        &positions,
        &nbr_offsets,
        &nbrs,
        &heights,
        &plate_id,
        &dynamics.boundary_state,
        &dynamics.mantle_heat,
        &plume_force,
        &mut next_vertex_states,
        &mut next_height,
        params,
    );

    preserve_target_sea_ratio(&mut next_height, world.target_sea_ratio, 0.35);

    dynamics.vertex_states = next_vertex_states;
    dynamics.cached_metrics = metrics;
    dynamics.update_index = dynamics.update_index.saturating_add(1);
    world.core.height = next_height;

    if let Some(state) = world.river_erosion_state.as_mut() {
        if state.height.len() == world.core.height.len() {
            state.height.clone_from(&world.core.height);
        }
    }
}

fn ensure_terrain_dynamics(world: &mut World) {
    let cell_count = world.core.height.len();
    let plate_count = world
        .core
        .plate_id
        .iter()
        .copied()
        .max()
        .map(|v| v as usize + 1)
        .unwrap_or(0);
    let needs_rebuild = match world.terrain_dynamics.as_ref() {
        Some(state) => {
            state.vertex_states.len() != cell_count
                || state.mantle_heat.len() != cell_count
                || state.plate_states.len() != plate_count
        }
        None => true,
    };
    if !needs_rebuild {
        return;
    }

    let plate_states = build_plate_states(&world.core.plate_id);
    let mut vertex_states = vec![
        VertexCrustState {
            crust_type: CrustType::Continental,
            thickness: 0.65,
            density: 0.45,
            age: 0.0,
            stress: 0.0,
            temperature: 0.5,
            rigidity: 0.75,
            stress_tensor: StressTensor::default(),
        };
        cell_count
    ];
    let mut mantle_heat = vec![0.5; cell_count];

    for i in 0..cell_count {
        let h = world.core.height[i];
        let is_oceanic = h <= 0.0;
        vertex_states[i].crust_type = if is_oceanic {
            CrustType::Oceanic
        } else {
            CrustType::Continental
        };
        vertex_states[i].thickness = if is_oceanic {
            0.35 + (-h).clamp(0.0, 0.6) * 0.25
        } else {
            0.65 + h.clamp(0.0, 0.6) * 0.20
        };
        vertex_states[i].density = if is_oceanic {
            0.58 + (-h).clamp(0.0, 0.5) * 0.12
        } else {
            0.42 + (h.max(0.0)).clamp(0.0, 0.5) * 0.08
        };
        vertex_states[i].age = if is_oceanic {
            (0.08 + (-h).clamp(0.0, 0.5) * 0.5).clamp(0.0, 1.0)
        } else {
            1.0
        };
        vertex_states[i].rigidity = if is_oceanic { 0.55 } else { 0.82 };
        mantle_heat[i] = if is_oceanic { 0.34 } else { 0.58 };
        vertex_states[i].temperature = mantle_heat[i];
    }

    world.terrain_dynamics = Some(TerrainDynamicsState {
        update_index: 0,
        plate_states,
        vertex_states,
        boundary_state: BoundaryDynamicsState {
            reclassify_interval_ticks: 4,
            steps_since_reclassify: 0,
            dominant_type: vec![BoundaryType::PassiveMargin; cell_count],
            activity: vec![0.0; cell_count],
        },
        mantle_heat,
        cached_metrics: TerrainStepMetrics::default(),
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
            angular_speed: 0.06 + 0.10 * hash01(seed ^ 0xc2b2_ae35),
            phase_offset: std::f32::consts::TAU * hash01(seed ^ 0x85eb_ca6b),
            activity: (0.60 + 0.40 * hash01(seed ^ 0x9e37_79b9)).clamp(0.0, 1.0),
        });
    }
    plate_states
}

fn update_mantle_heat_and_plumes(
    mantle_heat: &mut [f32],
    vertex_states: &[VertexCrustState],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    params: &TerrainParams,
) -> Vec<f32> {
    let cell_count = mantle_heat.len();
    let mut next = mantle_heat.to_vec();
    let mut plume_force = vec![0.0_f32; cell_count];

    for i in 0..cell_count {
        let discharge_rate = match vertex_states[i].crust_type {
            CrustType::Continental => 0.10,
            CrustType::Oceanic => 1.00,
        };
        let mut heat = mantle_heat[i] + params.mantle_heat_input.max(0.0);
        heat -= params.mantle_heat_loss.max(0.0) * discharge_rate;

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut diff = 0.0;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            diff += (mantle_heat[n] - mantle_heat[i]) * params.mantle_diffusion_rate.max(0.0);
        }
        next[i] = (heat + diff).clamp(0.0, 1.0);
    }

    for i in 0..cell_count {
        let mut heat = next[i];
        if heat > params.plume_threshold {
            plume_force[i] = (heat - params.plume_threshold).max(0.0) * params.plume_gain.max(0.0);
            heat *= params.plume_heat_release_rate.clamp(0.0, 1.0);
        }
        next[i] = heat.clamp(0.0, 1.0);
    }

    mantle_heat.copy_from_slice(&next);
    plume_force
}

fn reclassify_boundaries(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u16],
    plate_states: &[PlateKinematicsState],
    vertex_states: &[VertexCrustState],
    boundary_state: &mut BoundaryDynamicsState,
    params: &TerrainParams,
) {
    let cell_count = plate_id.len();
    if boundary_state.dominant_type.len() != cell_count {
        boundary_state.dominant_type = vec![BoundaryType::PassiveMargin; cell_count];
    }
    if boundary_state.activity.len() != cell_count {
        boundary_state.activity = vec![0.0; cell_count];
    }

    for i in 0..cell_count {
        let pos_i = positions[i];
        let vel_i =
            plate_velocity_from_state(plate_states.get(plate_id[i] as usize), plate_id[i], pos_i);
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;

        let mut best_type = BoundaryType::PassiveMargin;
        let mut best_score = 0.0_f32;

        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= cell_count || plate_id[n] == plate_id[i] {
                continue;
            }

            let pos_n = positions[n];
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
                plate_states.get(plate_id[n] as usize),
                plate_id[n],
                pos_n,
            );
            let rel_v = [
                vel_n[0] - vel_i[0],
                vel_n[1] - vel_i[1],
                vel_n[2] - vel_i[2],
            ];
            let rel_n = dot(rel_v, edge_dir);
            let rel_mag = length3(rel_v);
            let rel_t = (rel_mag * rel_mag - rel_n * rel_n).max(0.0).sqrt();

            let candidate =
                classify_boundary_pair(rel_n, rel_t, vertex_states[i], vertex_states[n], params);
            if candidate.1 > best_score {
                best_type = candidate.0;
                best_score = candidate.1;
            }
        }

        boundary_state.dominant_type[i] = best_type;
        boundary_state.activity[i] = best_score.clamp(0.0, 1.0);
    }
}

fn classify_boundary_pair(
    rel_n: f32,
    rel_t: f32,
    a: VertexCrustState,
    b: VertexCrustState,
    params: &TerrainParams,
) -> (BoundaryType, f32) {
    if rel_n < -DIVERGENT_THRESHOLD {
        let bt = if a.crust_type == CrustType::Continental && b.crust_type == CrustType::Continental
        {
            BoundaryType::Rift
        } else {
            BoundaryType::Ridge
        };
        return (bt, (-rel_n * 8.0 + rel_t * 2.0).clamp(0.0, 1.0));
    }

    if rel_n > CONVERGENT_THRESHOLD {
        let mut bt = BoundaryType::Collision;
        let mut oceanic = None;
        if a.crust_type == CrustType::Oceanic {
            oceanic = Some(a);
        }
        if b.crust_type == CrustType::Oceanic {
            oceanic = Some(match oceanic {
                Some(prev) if prev.density >= b.density => prev,
                _ => b,
            });
        }

        if let Some(oceanic_state) = oceanic {
            let age_gate = oceanic_state.age > params.subduction_initiation_threshold;
            let density_gate = oceanic_state.density > params.subduction_density_threshold;
            let age_coupled = (oceanic_state.age * params.subduction_age_coupling
                + oceanic_state.density)
                .clamp(0.0, 2.0);
            if age_gate && density_gate || age_coupled > 1.0 {
                bt = BoundaryType::Subduction;
            }
        }
        return (bt, (rel_n * 8.0 + rel_t).clamp(0.0, 1.0));
    }

    if rel_t > TRANSFORM_THRESHOLD {
        return (BoundaryType::Transform, (rel_t * 7.0).clamp(0.0, 1.0));
    }

    (BoundaryType::PassiveMargin, 0.03)
}

fn update_plate_kinematics(
    plate_id: &[u16],
    plate_states: &mut [PlateKinematicsState],
    boundary_state: &BoundaryDynamicsState,
    params: &TerrainParams,
) {
    if plate_states.is_empty() {
        return;
    }

    let mut plate_activity = vec![0.0_f32; plate_states.len()];
    let mut plate_count = vec![0_u32; plate_states.len()];

    for i in 0..plate_id.len() {
        let pid = plate_id[i] as usize;
        if pid >= plate_states.len() {
            continue;
        }
        plate_activity[pid] += boundary_state.activity.get(i).copied().unwrap_or(0.0);
        plate_count[pid] = plate_count[pid].saturating_add(1);
    }

    let gain = params.plate_motion_gain.max(0.0);
    for pid in 0..plate_states.len() {
        let denom = plate_count[pid].max(1) as f32;
        let activity = (plate_activity[pid] / denom).clamp(0.0, 1.0);
        let damping =
            match dominant_plate_boundary_type(pid as u16, plate_id, &boundary_state.dominant_type)
            {
                BoundaryType::PassiveMargin => 0.985,
                BoundaryType::Collision => 0.980,
                BoundaryType::Subduction => 0.995,
                _ => 0.990,
            };
        plate_states[pid].angular_speed =
            (plate_states[pid].angular_speed * damping + gain * activity * 0.015).clamp(0.01, 0.30);
        plate_states[pid].activity =
            lerp(plate_states[pid].activity, activity, 0.20).clamp(0.0, 1.0);
    }
}

fn dominant_plate_boundary_type(
    plate: u16,
    plate_id: &[u16],
    boundary_types: &[BoundaryType],
) -> BoundaryType {
    let mut counts = [0_u32; 6];
    for i in 0..plate_id.len() {
        if plate_id[i] != plate {
            continue;
        }
        let t = boundary_types
            .get(i)
            .copied()
            .unwrap_or(BoundaryType::PassiveMargin);
        counts[boundary_type_index(t)] = counts[boundary_type_index(t)].saturating_add(1);
    }
    let mut best = BoundaryType::PassiveMargin;
    let mut best_count = 0_u32;
    for t in [
        BoundaryType::Subduction,
        BoundaryType::Collision,
        BoundaryType::Ridge,
        BoundaryType::Rift,
        BoundaryType::Transform,
        BoundaryType::PassiveMargin,
    ] {
        let c = counts[boundary_type_index(t)];
        if c > best_count {
            best_count = c;
            best = t;
        }
    }
    best
}

fn boundary_type_index(boundary_type: BoundaryType) -> usize {
    match boundary_type {
        BoundaryType::Ridge => 0,
        BoundaryType::Rift => 1,
        BoundaryType::Subduction => 2,
        BoundaryType::Collision => 3,
        BoundaryType::Transform => 4,
        BoundaryType::PassiveMargin => 5,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_stress_and_surface_update(
    _positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    heights: &[f32],
    plate_id: &[u16],
    boundary_state: &BoundaryDynamicsState,
    mantle_heat: &[f32],
    plume_force: &[f32],
    next_vertex_states: &mut [VertexCrustState],
    next_height: &mut [f32],
    params: &TerrainParams,
) -> TerrainStepMetrics {
    let cell_count = heights.len();
    let mut terrain_delta_sum = 0.0_f32;
    let mut boundary_sum = 0.0_f32;
    let mut uplift_sum = 0.0_f32;
    let mut subsidence_sum = 0.0_f32;

    for i in 0..cell_count {
        let mut tensor = boundary_tensor(
            boundary_state
                .dominant_type
                .get(i)
                .copied()
                .unwrap_or(BoundaryType::PassiveMargin),
            boundary_state.activity.get(i).copied().unwrap_or(0.0),
        );

        let plume = plume_force.get(i).copied().unwrap_or(0.0);
        tensor.xx += plume * 0.7;
        tensor.yy += plume * 0.7;

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut nbr_sum = 0.0;
        let mut nbr_count = 0usize;
        let mut nbr_stress = StressTensor::default();

        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            nbr_sum += heights[n];
            nbr_count += 1;
            let n_tensor = next_vertex_states[n].stress_tensor;
            let atten = if plate_id[n] == plate_id[i] {
                0.12
            } else {
                0.18
            };
            nbr_stress.xx += n_tensor.xx * atten;
            nbr_stress.yy += n_tensor.yy * atten;
            nbr_stress.xy += n_tensor.xy * atten;
        }

        tensor.xx += nbr_stress.xx;
        tensor.yy += nbr_stress.yy;
        tensor.xy += nbr_stress.xy;

        let prev = next_vertex_states[i];
        let rigidity =
            (prev.rigidity + 0.15 * prev.thickness - 0.20 * mantle_heat[i]).clamp(0.20, 1.40);
        let inv_rigidity = 1.0 / rigidity.max(1e-3);

        tensor.xx *= inv_rigidity;
        tensor.yy *= inv_rigidity;
        tensor.xy *= inv_rigidity;

        let stress_scalar = (tensor.xx + tensor.yy) * 0.5 + tensor.xy.abs() * 0.30;
        let relax = params.stress_relaxation_rate.clamp(0.0, 1.0);
        let stress = prev.stress * (1.0 - relax) + stress_scalar * relax;

        let mut state = prev;
        state.temperature = mantle_heat[i];
        state.stress_tensor = tensor;
        state.stress = stress;

        if state.crust_type == CrustType::Oceanic {
            state.age = (state.age + 0.003 + 0.006 * (1.0 - plume).clamp(0.0, 1.0)).clamp(0.0, 1.0);
            state.density = (0.56 + 0.25 * state.age).clamp(0.45, 0.92);
        } else {
            state.age = 1.0;
            state.density = (0.40 + 0.08 * state.thickness).clamp(0.32, 0.58);
        }

        let compressive = (-stress).max(0.0);
        let tensile = stress.max(0.0);
        let boundary_type = boundary_state
            .dominant_type
            .get(i)
            .copied()
            .unwrap_or(BoundaryType::PassiveMargin);
        let volcanic = match boundary_type {
            BoundaryType::Subduction => 0.004 + plume * 0.6,
            BoundaryType::Ridge | BoundaryType::Rift => 0.003 + plume * 0.5,
            _ => plume * 0.35,
        };

        let uplift = params.uplift_rate_gain.max(0.0) * (compressive + volcanic);
        let subsidence = params.subsidence_rate_gain.max(0.0)
            * (tensile
                + if state.crust_type == CrustType::Oceanic {
                    state.age * 0.6
                } else {
                    0.0
                });

        let diffusive = if nbr_count == 0 {
            0.0
        } else {
            (nbr_sum / nbr_count as f32 - heights[i]) * DEFAULT_DIFFUSION_WEIGHT
        };
        let isostasy = params.isostasy_rate.max(0.0) * (state.thickness - 0.55);
        let raw_delta = uplift - subsidence + diffusive + isostasy;
        let delta = raw_delta.clamp(-MAX_HEIGHT_DELTA_PER_STEP, MAX_HEIGHT_DELTA_PER_STEP);
        let next_h = (heights[i] + delta).clamp(-1.0, 1.0);

        if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift) && next_h < -0.02 {
            state.crust_type = CrustType::Oceanic;
            state.thickness = (state.thickness - 0.010).clamp(0.20, 1.20);
        } else if boundary_type == BoundaryType::Collision && next_h > 0.20 {
            state.crust_type = CrustType::Continental;
            state.thickness = (state.thickness + 0.008).clamp(0.20, 1.20);
        }

        state.thickness =
            (state.thickness + uplift * 0.5 - subsidence * 0.4 + plume * 0.2).clamp(0.18, 1.25);
        state.rigidity = rigidity;

        terrain_delta_sum += delta.abs();
        boundary_sum += boundary_state.activity.get(i).copied().unwrap_or(0.0);
        if delta > 0.0 {
            uplift_sum += delta;
        } else {
            subsidence_sum += -delta;
        }

        next_vertex_states[i] = state;
        next_height[i] = next_h;
    }

    let denom = cell_count.max(1) as f32;
    TerrainStepMetrics {
        terrain_activity: (terrain_delta_sum / denom).clamp(0.0, 1.0),
        boundary_activity: (boundary_sum / denom).clamp(0.0, 1.0),
        uplift_rate: uplift_sum / denom,
        subsidence_rate: subsidence_sum / denom,
    }
}

fn boundary_tensor(boundary_type: BoundaryType, activity: f32) -> StressTensor {
    let a = activity.clamp(0.0, 1.0);
    match boundary_type {
        BoundaryType::Subduction | BoundaryType::Collision => StressTensor {
            xx: -0.09 * a,
            yy: -0.09 * a,
            xy: 0.0,
        },
        BoundaryType::Ridge | BoundaryType::Rift => StressTensor {
            xx: 0.07 * a,
            yy: 0.07 * a,
            xy: 0.0,
        },
        BoundaryType::Transform => StressTensor {
            xx: 0.0,
            yy: 0.0,
            xy: 0.08 * a,
        },
        BoundaryType::PassiveMargin => StressTensor {
            xx: 0.0,
            yy: 0.0,
            xy: 0.0,
        },
    }
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
) -> [f32; 3] {
    let seed = plate_id as u32;
    let fallback_axis = seeded_axis(seed ^ 0x27d4_eb2f);
    let angular_axis = state.map(|s| s.angular_axis).unwrap_or(fallback_axis);
    let angular_speed = state
        .map(|s| s.angular_speed * (0.55 + 0.45 * s.activity))
        .unwrap_or(0.12);
    let omega = [
        angular_axis[0] * angular_speed,
        angular_axis[1] * angular_speed,
        angular_axis[2] * angular_speed,
    ];
    cross3(omega, pos)
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

#[cfg(test)]
fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = length3(v);
    if len <= 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
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
    use crate::TerrainParams;

    use super::*;
    use crate::sim::world::{CoreCells, World, WorldMesh};

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                normalize3([0.0, 0.8, 0.6]),
                normalize3([0.7, 0.2, 0.6]),
                normalize3([0.4, -0.7, 0.6]),
                normalize3([-0.6, -0.1, 0.8]),
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

    #[test]
    fn step_world_advances_tick_and_sets_budget_to_one() {
        let mut world = build_test_world();
        step_world(&mut world);
        assert_eq!(world.tick, 1);
        assert_eq!(world.budgets.terrain, 1);
    }

    #[test]
    fn terrain_dynamics_initializes_new_fields() {
        let mut world = build_test_world();
        run_terrain_step(&mut world);

        let dynamics = world.terrain_dynamics.as_ref().expect("terrain dynamics");
        assert_eq!(dynamics.vertex_states.len(), world.core.height.len());
        assert_eq!(dynamics.mantle_heat.len(), world.core.height.len());
        assert_eq!(
            dynamics.boundary_state.dominant_type.len(),
            world.core.height.len()
        );
        assert!(dynamics.cached_metrics.terrain_activity >= 0.0);
    }

    #[test]
    fn passive_margin_to_subduction_transition_is_possible() {
        let mut params = TerrainParams::default();
        params.subduction_initiation_threshold = 0.2;
        params.subduction_density_threshold = 0.5;
        params.subduction_age_coupling = 0.8;

        let oceanic = VertexCrustState {
            crust_type: CrustType::Oceanic,
            thickness: 0.40,
            density: 0.88,
            age: 0.90,
            stress: 0.0,
            temperature: 0.4,
            rigidity: 0.55,
            stress_tensor: StressTensor::default(),
        };
        let continental = VertexCrustState {
            crust_type: CrustType::Continental,
            thickness: 0.72,
            density: 0.48,
            age: 1.0,
            stress: 0.0,
            temperature: 0.5,
            rigidity: 0.85,
            stress_tensor: StressTensor::default(),
        };
        let (kind, score) = classify_boundary_pair(0.08, 0.02, oceanic, continental, &params);
        assert_eq!(kind, BoundaryType::Subduction);
        assert!(score > 0.3);
    }

    #[test]
    fn plate_velocity_is_tangent_to_sphere() {
        let plates = build_plate_states(&[0, 1, 2, 3, 3]);
        let pos = normalize3([0.4, 0.3, 0.8]);
        let vel = plate_velocity_from_state(plates.get(1), 1, pos);
        let radial = dot(pos, vel).abs();
        assert!(radial < 1e-5);
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
}
