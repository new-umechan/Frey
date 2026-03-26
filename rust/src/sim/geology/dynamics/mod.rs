use crate::GeologyParams;

mod boundary_dynamics;
mod surface_dynamics;

use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, CrustType, GeologyDynamicsState, GeologyInternal,
    GeologyStepMetrics, PlateId, PlateKinematicsState, StressTensor, VertexCrustState, World,
};

use crate::sim::exec::math::{hash01, seeded_axis};
use boundary_dynamics::{
    plate_velocity_for_cell, reclassify_boundaries, update_plate_kinematics,
    ReclassifyBoundariesInput,
};
use surface_dynamics::{apply_stress_and_surface_update, SurfaceUpdateInput, SurfaceUpdateOutput};

pub(crate) fn run_geology_dynamics_step(world: &mut World) {
    if world.mesh.nbr_offsets.len() != world.state.geology.height.len() + 1 {
        return;
    }
    if world.state.geology.plate_id.len() != world.state.geology.height.len() {
        return;
    }

    ensure_geology_dynamics(world);
    let Some(dynamics) = world.runtime.geology_dynamics.as_mut() else {
        return;
    };

    let cell_count = world.state.geology.height.len();
    let default_params = GeologyParams::default();
    let params = world
        .runtime
        .hydrology_dynamics
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
    if dynamics.boundary_state.rollback_fraction.len() != cell_count {
        dynamics.boundary_state.rollback_fraction = vec![0.0; cell_count];
    }
    if dynamics.boundary_state.backarc_tension.len() != cell_count {
        dynamics.boundary_state.backarc_tension = vec![0.0; cell_count];
    }
    if world.state.geology.volcanism.len() != cell_count {
        world.state.geology.volcanism = vec![0.0; cell_count];
    }
    if world.state.geology.vertex_buoyancy.len() != cell_count {
        world.state.geology.vertex_buoyancy = vec![0.0; cell_count];
    }
    if world.state.geology.geology_internal.len() != cell_count {
        world.state.geology.geology_internal = vec![GeologyInternal::default(); cell_count];
    }

    let heights = world.state.geology.height.clone();
    let plate_id = world.state.geology.plate_id.clone();
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

    update_plate_kinematics(
        &plate_id,
        &mut dynamics.plate_states,
        &dynamics.boundary_state,
        params,
    );

    let mut next_vertex_states = advect_continuous_attributes(
        &positions,
        &nbr_offsets,
        &nbrs,
        &plate_id,
        &dynamics.plate_states,
        &dynamics.vertex_states,
        params,
    );
    let mut next_plate_id = plate_id.clone();
    apply_boundary_crossing_discrete_attrs(
        BoundaryCrossingInput {
            positions: &positions,
            nbr_offsets: &nbr_offsets,
            nbrs: &nbrs,
            plate_states: &dynamics.plate_states,
            plate_id_prev: &plate_id,
            boundary_state: &dynamics.boundary_state,
        },
        &mut next_plate_id,
        &mut next_vertex_states,
    );

    let reclassify_interval = params.boundary_reclassify_interval.max(1);
    dynamics.boundary_state.reclassify_interval_ticks = reclassify_interval;
    if dynamics.boundary_state.steps_since_reclassify >= reclassify_interval
        || dynamics.boundary_state.steps_since_reclassify == 0
    {
        reclassify_boundaries(
            ReclassifyBoundariesInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &next_plate_id,
                plate_states: &dynamics.plate_states,
                vertex_states: &next_vertex_states,
                params,
            },
            &mut dynamics.boundary_state,
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

    let mut next_height = heights.clone();
    let mut next_volcanism = world.state.geology.volcanism.clone();
    let mut next_vertex_buoyancy = world.state.geology.vertex_buoyancy.clone();
    let mut surface_output = SurfaceUpdateOutput {
        next_vertex_states: &mut next_vertex_states,
        next_height: &mut next_height,
        next_volcanism: &mut next_volcanism,
        next_vertex_buoyancy: &mut next_vertex_buoyancy,
    };
    let metrics = apply_stress_and_surface_update(
        SurfaceUpdateInput {
            nbr_offsets: &nbr_offsets,
            nbrs: &nbrs,
            heights: &heights,
            plate_id: &next_plate_id,
            boundary_state: &dynamics.boundary_state,
            mantle_heat: &dynamics.mantle_heat,
            plume_force: &plume_force,
            params,
        },
        &mut surface_output,
    );

    dynamics.vertex_states = next_vertex_states;
    dynamics.cached_metrics = metrics;
    dynamics.update_index = dynamics.update_index.saturating_add(1);
    world.state.geology.height = next_height;
    world.state.geology.plate_id = next_plate_id;
    world.state.geology.volcanism = next_volcanism;
    world.state.geology.vertex_buoyancy = next_vertex_buoyancy;
    world.state.geology.boundary_condition = dynamics.boundary_state.activity.clone();
    sync_geology_internal(
        &mut world.state.geology.geology_internal,
        &dynamics.vertex_states,
    );

    if let Some(state) = world.runtime.hydrology_dynamics.as_mut() {
        if state.height.len() == world.state.geology.height.len() {
            state.height.clone_from(&world.state.geology.height);
        }
    }
}

fn ensure_geology_dynamics(world: &mut World) {
    let cell_count = world.state.geology.height.len();
    let plate_count = world
        .state
        .geology
        .plate_id
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let needs_rebuild = match world.runtime.geology_dynamics.as_ref() {
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

    let plate_states = build_plate_states(&world.state.geology.plate_id);
    let mut vertex_states = vec![
        VertexCrustState {
            crust_type: CrustType::Continental,
            thickness: 0.65,
            density: 0.45,
            age: 0.0,
            stress: 0.0,
            temperature: 0.5,
            rigidity: 0.75,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: StressTensor::default(),
        };
        cell_count
    ];
    let mut mantle_heat = vec![0.5; cell_count];

    for i in 0..cell_count {
        let h = world.state.geology.height[i];
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
        let age_ref = world
            .runtime
            .hydrology_dynamics
            .as_ref()
            .map(|s| s.params.age_ref.max(1e-4))
            .unwrap_or(1.0);
        let oceanic_base_density = world
            .runtime
            .hydrology_dynamics
            .as_ref()
            .map(|s| s.params.oceanic_base_density)
            .unwrap_or(2.90);
        let continental_density = world
            .runtime
            .hydrology_dynamics
            .as_ref()
            .map(|s| s.params.continental_crust_density)
            .unwrap_or(2.70);
        let age_density_gain = world
            .runtime
            .hydrology_dynamics
            .as_ref()
            .map(|s| s.params.age_density_gain.max(0.0))
            .unwrap_or(0.25);
        vertex_states[i].age = if is_oceanic {
            (0.08 + (-h).clamp(0.0, 0.5) * 0.5).clamp(0.0, 1.0) * age_ref
        } else {
            age_ref
        };
        vertex_states[i].density = if is_oceanic {
            let age_norm = (vertex_states[i].age / age_ref).clamp(0.0, 1.0);
            oceanic_base_density + age_density_gain * age_norm.sqrt()
        } else {
            continental_density
        };
        vertex_states[i].rigidity = if is_oceanic { 0.55 } else { 0.82 };
        mantle_heat[i] = if is_oceanic { 0.34 } else { 0.58 };
        vertex_states[i].temperature = mantle_heat[i];
    }

    world.runtime.geology_dynamics = Some(GeologyDynamicsState {
        update_index: 0,
        plate_states,
        vertex_states,
        boundary_state: BoundaryDynamicsState {
            reclassify_interval_ticks: 4,
            steps_since_reclassify: 0,
            dominant_type: vec![BoundaryType::PassiveMargin; cell_count],
            activity: vec![0.0; cell_count],
            edge_pairs: Vec::new(),
            edge_internal: Vec::new(),
            rollback_fraction: vec![0.0; cell_count],
            backarc_tension: vec![0.0; cell_count],
            slab_convergence_component: vec![0.0; cell_count],
            slab_rollback_component: vec![0.0; cell_count],
        },
        mantle_heat,
        cached_metrics: GeologyStepMetrics::default(),
    });
    if world.state.geology.geology_internal.len() != cell_count {
        world.state.geology.geology_internal = vec![GeologyInternal::default(); cell_count];
    }
    if let Some(dynamics) = world.runtime.geology_dynamics.as_ref() {
        sync_geology_internal(
            &mut world.state.geology.geology_internal,
            &dynamics.vertex_states,
        );
    }
}

fn build_plate_states(plate_ids: &[PlateId]) -> Vec<PlateKinematicsState> {
    let plate_count = plate_ids
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let mut plate_states = Vec::with_capacity(plate_count);
    for plate in 0..plate_count {
        let seed = plate as u32;
        plate_states.push(PlateKinematicsState {
            angular_axis: seeded_axis(seed ^ 0x27d4_eb2f),
            angular_speed: 0.06 + 0.10 * hash01(seed ^ 0xc2b2_ae35),
            phase_offset: std::f32::consts::TAU * hash01(seed ^ 0x85eb_ca6b),
            activity: (0.60_f32 + 0.40_f32 * hash01(seed ^ 0x9e37_79b9)).clamp(0.0, 1.0),
        });
    }
    plate_states
}

fn update_mantle_heat_and_plumes(
    mantle_heat: &mut [f32],
    vertex_states: &[VertexCrustState],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    params: &GeologyParams,
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

fn advect_continuous_attributes(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_states: &[PlateKinematicsState],
    vertex_states: &[VertexCrustState],
    params: &GeologyParams,
) -> Vec<VertexCrustState> {
    let mut next = vertex_states.to_vec();
    let dt = params.age_advection_gain.clamp(0.0, 0.25);
    if dt <= 0.0 {
        return next;
    }

    let age_ref = params.age_ref.max(1e-4);
    let density_min = params
        .continental_crust_density
        .min(params.oceanic_base_density)
        * 0.75;
    let density_max = (params.oceanic_base_density + params.age_density_gain.max(0.0) + 0.2)
        .max(density_min + 1e-3);
    let age_values = vertex_states.iter().map(|s| s.age).collect::<Vec<_>>();
    let thickness_values = vertex_states
        .iter()
        .map(|s| s.thickness)
        .collect::<Vec<_>>();
    let density_values = vertex_states.iter().map(|s| s.density).collect::<Vec<_>>();
    for i in 0..vertex_states.len() {
        let pos_i = positions[i];
        let velocity = plate_velocity_for_cell(plate_states, plate_id[i], pos_i);
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let neighbors = &nbrs[start..end];
        if neighbors.is_empty() {
            continue;
        }

        next[i].age = muscl_like_advect_scalar(
            i,
            vertex_states[i].age,
            &age_values,
            neighbors,
            positions,
            velocity,
            dt,
        )
        .clamp(0.0, age_ref);
        next[i].thickness = muscl_like_advect_scalar(
            i,
            vertex_states[i].thickness,
            &thickness_values,
            neighbors,
            positions,
            velocity,
            dt,
        )
        .clamp(0.18, 1.25);
        next[i].density = muscl_like_advect_scalar(
            i,
            vertex_states[i].density,
            &density_values,
            neighbors,
            positions,
            velocity,
            dt,
        )
        .clamp(density_min, density_max);
    }
    next
}

fn muscl_like_advect_scalar(
    index: usize,
    center_value: f32,
    field: &[f32],
    neighbors: &[u32],
    positions: &[[f32; 3]],
    velocity: [f32; 3],
    dt: f32,
) -> f32 {
    let mut raw = 0.0_f32;
    let mut count = 0_u32;
    let mut min_v = center_value;
    let mut max_v = center_value;
    for &n_u32 in neighbors {
        let n = n_u32 as usize;
        if n >= field.len() {
            continue;
        }
        let dir_raw = [
            positions[n][0] - positions[index][0],
            positions[n][1] - positions[index][1],
            positions[n][2] - positions[index][2],
        ];
        let len =
            ((dir_raw[0] * dir_raw[0]) + (dir_raw[1] * dir_raw[1]) + (dir_raw[2] * dir_raw[2]))
                .sqrt()
                .max(1e-5);
        let dir = [dir_raw[0] / len, dir_raw[1] / len, dir_raw[2] / len];
        let dq = field[n] - center_value;
        raw += dq * (velocity[0] * dir[0] + velocity[1] * dir[1] + velocity[2] * dir[2]);
        min_v = min_v.min(field[n]);
        max_v = max_v.max(field[n]);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return center_value;
    }
    let predicted = center_value - dt * (raw / count as f32);
    predicted.clamp(min_v, max_v)
}

struct BoundaryCrossingInput<'a> {
    positions: &'a [[f32; 3]],
    nbr_offsets: &'a [u32],
    nbrs: &'a [u32],
    plate_states: &'a [PlateKinematicsState],
    plate_id_prev: &'a [PlateId],
    boundary_state: &'a BoundaryDynamicsState,
}

fn apply_boundary_crossing_discrete_attrs(
    input: BoundaryCrossingInput<'_>,
    plate_id_next: &mut [PlateId],
    vertex_states: &mut [VertexCrustState],
) {
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let plate_states = input.plate_states;
    let plate_id_prev = input.plate_id_prev;
    let boundary_state = input.boundary_state;

    let mut next_crust = vertex_states
        .iter()
        .map(|s| s.crust_type)
        .collect::<Vec<_>>();
    for i in 0..plate_id_prev.len() {
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let boundary_activity = boundary_state.activity.get(i).copied().unwrap_or(0.0);
        if boundary_activity < 0.12 {
            continue;
        }

        let mut best_score = 0.0_f32;
        let mut best_plate = plate_id_prev[i];
        let mut best_crust = next_crust[i];
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= plate_id_prev.len() || plate_id_prev[n] == plate_id_prev[i] {
                continue;
            }
            let vel_n = plate_velocity_for_cell(plate_states, plate_id_prev[n], positions[n]);
            let dir_raw = [
                positions[i][0] - positions[n][0],
                positions[i][1] - positions[n][1],
                positions[i][2] - positions[n][2],
            ];
            let len =
                ((dir_raw[0] * dir_raw[0]) + (dir_raw[1] * dir_raw[1]) + (dir_raw[2] * dir_raw[2]))
                    .sqrt()
                    .max(1e-5);
            let dir = [dir_raw[0] / len, dir_raw[1] / len, dir_raw[2] / len];
            let inflow = vel_n[0] * dir[0] + vel_n[1] * dir[1] + vel_n[2] * dir[2];
            let score = inflow.max(0.0) * boundary_activity;
            if score > best_score && inflow > 0.02 {
                best_score = score;
                best_plate = plate_id_prev[n];
                best_crust = vertex_states[n].crust_type;
            }
        }
        if best_score > 0.03 {
            plate_id_next[i] = best_plate;
            next_crust[i] = best_crust;
        }
    }
    for (i, crust) in next_crust.into_iter().enumerate() {
        vertex_states[i].crust_type = crust;
    }
}

fn sync_geology_internal(target: &mut [GeologyInternal], source: &[VertexCrustState]) {
    let count = target.len().min(source.len());
    for i in 0..count {
        target[i] = GeologyInternal {
            crust_type: source[i].crust_type,
            age: source[i].age,
            thickness: source[i].thickness,
            density: source[i].density,
            stress: source[i].stress_tensor,
            temperature: source[i].temperature,
            rigidity: source[i].rigidity,
            arc_volcanism: source[i].arc_volcanism,
            ridge_volcanism: source[i].ridge_volcanism,
            hotspot_volcanism: source[i].hotspot_volcanism,
            backarc_volcanism: source[i].backarc_volcanism,
        };
    }
}
