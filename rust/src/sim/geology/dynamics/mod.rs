use crate::GeologyParams;

mod boundary_dynamics;
mod surface_dynamics;

use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, CrustType, GeologyDynamicsState, GeologyStepMetrics,
    PlateKinematicsState, StressTensor, VertexCrustState, World,
};

use crate::sim::exec::math::{hash01, seeded_axis};
use boundary_dynamics::{reclassify_boundaries, update_plate_kinematics};
use surface_dynamics::{apply_stress_and_surface_update, preserve_target_sea_ratio};

pub(crate) fn run_geology_dynamics_step(world: &mut World) {
    if world.mesh.nbr_offsets.len() != world.state.geology.height.len() + 1 {
        return;
    }
    if world.state.geology.plate_id.len() != world.state.geology.height.len() {
        return;
    }

    ensure_geology_dynamics(world);
    let Some(dynamics) = world.exec.geology_dynamics.as_mut() else {
        return;
    };

    let cell_count = world.state.geology.height.len();
    let default_params = GeologyParams::default();
    let params = world
        .exec
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

    preserve_target_sea_ratio(&mut next_height, world.exec.target_sea_ratio, 0.35);

    dynamics.vertex_states = next_vertex_states;
    dynamics.cached_metrics = metrics;
    dynamics.update_index = dynamics.update_index.saturating_add(1);
    world.state.geology.height = next_height;
    world.state.geology.boundary_condition = dynamics.boundary_state.activity.clone();

    if let Some(state) = world.exec.hydrology_dynamics.as_mut() {
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
        .map(|v| v as usize + 1)
        .unwrap_or(0);
    let needs_rebuild = match world.exec.geology_dynamics.as_ref() {
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
        vertex_states[i].density = if is_oceanic {
            0.58 + (-h).clamp(0.0, 0.5) * 0.12
        } else {
            0.42 + h.max(0.0).clamp(0.0, 0.5) * 0.08
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

    world.exec.geology_dynamics = Some(GeologyDynamicsState {
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
        cached_metrics: GeologyStepMetrics::default(),
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
