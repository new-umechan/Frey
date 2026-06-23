use crate::GeologyParams;

mod boundary_dynamics;
mod surface_dynamics;

use crate::sim::geology_types::{CrustType, GeologyInternal, PlateId, StressTensor};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, EraKind, GeologyDynamicsState, GeologyStepMetrics,
    PlateKinematicsState, VertexCrustState, World,
};

use crate::sim::exec::math::{hash01, seeded_axis};
use boundary_dynamics::{
    plate_velocity_for_cell, reclassify_boundaries, update_plate_kinematics,
    ReclassifyBoundariesInput,
};
use surface_dynamics::{apply_stress_and_surface_update, SurfaceUpdateInput, SurfaceUpdateOutput};

const ENVIRONMENT_GEOLOGY_ACTIVITY_TARGET: f32 = 0.02;
const ENVIRONMENT_GEOLOGY_SPINUP_TICKS: f32 = 32.0;
const MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS: usize = 3;
const MAX_BOUNDARY_CROSSING_DONOR_FLOOR_CELLS: usize = 24;
const MIN_BOUNDARY_CROSSING_TARGET_NEIGHBORS: usize = 2;
pub(super) const EARTH_MEAN_RADIUS_KM: f32 = 6_371.0;
pub(super) const EARTH_PLATE_REFERENCE_SPEED_KM_PER_MYR: f32 = 50.0;
pub(super) const YEARS_PER_MYR: f32 = 1_000_000.0;

#[inline]
fn debug_assert_finite_non_negative(value: f32, label: &str, index: usize) {
    debug_assert!(
        value.is_finite() && value >= 0.0,
        "{label}[{index}] must be finite and non-negative, got {value}"
    );
}

#[inline]
fn debug_assert_finite_unit_interval(value: f32, label: &str, index: usize) {
    debug_assert!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "{label}[{index}] must be finite and in [0, 1], got {value}"
    );
}

fn debug_assert_river_next_no_cycle(river_next: &[i32], label: &str) {
    let n = river_next.len();
    for start in 0..n {
        let mut node = start as i32;
        let mut steps = 0usize;
        while node != -1 {
            debug_assert!(
                node >= 0 && (node as usize) < n,
                "{label}[{start}] has out-of-range link {node}"
            );
            steps = steps.saturating_add(1);
            debug_assert!(steps <= n, "{label}[{start}] forms a cycle");
            node = river_next[node as usize];
        }
    }
}

#[inline]
fn should_run_debug_validation() -> bool {
    cfg!(test)
}

pub(crate) fn run_geology_dynamics_step_with_state(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
) {
    if world.mesh().nbr_offsets.len() != world.state.geology.height.len() + 1 {
        return;
    }
    if world.state.geology.plate_id.len() != world.state.geology.height.len() {
        return;
    }

    let cell_count = world.state.geology.height.len();
    let rebuilt_runtime_state = ensure_geology_dynamics(world, geology_state);
    if should_run_debug_validation() {
        debug_validate_geology_state_with_state(
            world,
            geology_state.as_ref(),
            &world.control.geology_params,
            "pre-step",
        );
    }

    let Some(dynamics) = geology_state.as_mut() else {
        return;
    };

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
    let mesh = world.mesh();
    let positions = &mesh.positions;
    let nbr_offsets = &mesh.nbr_offsets;
    let nbrs = &mesh.nbrs;
    let heights = &world.state.geology.height;
    let plate_id = &world.state.geology.plate_id;
    let activity_scale = geology_activity_scale(world);

    let plume_force = update_mantle_heat_and_plumes(
        &mut dynamics.mantle_heat,
        &dynamics.vertex_states,
        nbr_offsets,
        nbrs,
        &world.control.geology_params,
    );

    update_plate_kinematics(
        plate_id,
        &mut dynamics.plate_states,
        &dynamics.boundary_state,
        &world.control.geology_params,
        world.clock.real_years_per_tick,
    );

    let mut next_vertex_states = advect_continuous_attributes(
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        &dynamics.plate_states,
        &dynamics.vertex_states,
        &world.control.geology_params,
    );
    let mut next_plate_id = plate_id.to_vec();
    let boundary_crossing_substeps = apply_boundary_crossing_discrete_attrs(
        BoundaryCrossingInput {
            positions,
            nbr_offsets,
            nbrs,
            plate_states: &dynamics.plate_states,
            plate_id_prev: plate_id,
            boundary_state: &dynamics.boundary_state,
            tick_seed: (world.clock.tick as u32) ^ (dynamics.update_index as u32).rotate_left(13),
        },
        &mut next_plate_id,
        &mut next_vertex_states,
    );
    let plate_id_churn_rate = plate_id_churn_rate(plate_id, &next_plate_id);
    let orphan_cell_count = orphan_cell_count(nbr_offsets, nbrs, &next_plate_id);
    let single_cell_plate_count = single_cell_plate_count(&next_plate_id);

    let reclassify_interval = world
        .control
        .geology_params
        .boundary_reclassify_interval
        .max(1);
    dynamics.boundary_state.reclassify_interval_ticks = reclassify_interval;
    if dynamics.boundary_state.steps_since_reclassify >= reclassify_interval
        || dynamics.boundary_state.steps_since_reclassify == 0
    {
        reclassify_boundaries(
            ReclassifyBoundariesInput {
                positions,
                nbr_offsets,
                nbrs,
                plate_id: &next_plate_id,
                plate_states: &dynamics.plate_states,
                vertex_states: &next_vertex_states,
                params: &world.control.geology_params,
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

    let mut next_height = heights.to_vec();
    let mut next_volcanism = world.state.geology.volcanism.clone();
    let mut next_vertex_buoyancy = world.state.geology.vertex_buoyancy.clone();
    let mut surface_output = SurfaceUpdateOutput {
        next_vertex_states: &mut next_vertex_states,
        next_height: &mut next_height,
        next_volcanism: &mut next_volcanism,
        next_vertex_buoyancy: &mut next_vertex_buoyancy,
    };
    let mut metrics = apply_stress_and_surface_update(
        SurfaceUpdateInput {
            nbr_offsets,
            nbrs,
            heights,
            plate_id: &next_plate_id,
            boundary_state: &dynamics.boundary_state,
            mantle_heat: &dynamics.mantle_heat,
            plume_force: &plume_force,
            activity_scale,
            params: &world.control.geology_params,
        },
        &mut surface_output,
    );
    metrics.mean_abs_surface_output_delta = mean_abs_height_delta(heights, &next_height);
    metrics.runtime_rebuild_applied = if rebuilt_runtime_state { 1.0 } else { 0.0 };
    metrics.activity_scale = activity_scale;
    metrics.plate_id_churn_rate = plate_id_churn_rate;
    metrics.orphan_cell_count = orphan_cell_count as f32;
    metrics.single_cell_plate_count = single_cell_plate_count as f32;
    metrics.boundary_crossing_substeps = boundary_crossing_substeps as f32;

    dynamics.vertex_states = next_vertex_states;
    dynamics.cached_metrics = metrics;
    dynamics.update_index = dynamics.update_index.saturating_add(1);
    world.state.geology.height = next_height;
    world.state.geology.plate_id = next_plate_id;
    world.state.geology.volcanism = next_volcanism;
    world.state.geology.vertex_buoyancy = next_vertex_buoyancy;
    world.state.geology.smoothing_limited_cells_ratio = metrics.smoothing_limited_cells_ratio;
    world.state.geology.mean_smoothing_factor = metrics.mean_smoothing_factor;
    world.state.geology.zero_mean_adjusted_cells_ratio = metrics.zero_mean_adjusted_cells_ratio;
    world.state.geology.zero_mean_mean_abs_correction = metrics.zero_mean_mean_abs_correction;
    world.state.geology.zero_mean_std_delta = metrics.zero_mean_std_delta;
    if world.state.geology.boundary_condition.len() == dynamics.boundary_state.activity.len() {
        world
            .state
            .geology
            .boundary_condition
            .clone_from_slice(&dynamics.boundary_state.activity);
    } else {
        world.state.geology.boundary_condition = dynamics.boundary_state.activity.clone();
    }
    sync_geology_internal(
        &mut world.state.geology.geology_internal,
        &dynamics.vertex_states,
    );

    let _ = dynamics;
    if should_run_debug_validation() {
        debug_validate_geology_state_with_state(
            world,
            geology_state.as_ref(),
            &world.control.geology_params,
            "post-step",
        );
    }
}

fn mean_abs_height_delta(before: &[f32], after: &[f32]) -> f32 {
    let count = before.len().min(after.len());
    if count == 0 {
        return 0.0;
    }
    before
        .iter()
        .zip(after.iter())
        .take(count)
        .map(|(before, after)| (after - before).abs())
        .sum::<f32>()
        / count as f32
}

fn plate_id_churn_rate(before: &[PlateId], after: &[PlateId]) -> f32 {
    let count = before.len().min(after.len());
    if count == 0 {
        return 0.0;
    }
    let changed = before
        .iter()
        .zip(after.iter())
        .take(count)
        .filter(|(a, b)| a != b)
        .count();
    changed as f32 / count as f32
}

fn orphan_cell_count(nbr_offsets: &[u32], nbrs: &[u32], plate_id: &[PlateId]) -> usize {
    let mut orphan_count = 0usize;
    for v in 0..plate_id.len() {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        if start == end {
            continue;
        }
        let same_neighbors = nbrs[start..end]
            .iter()
            .filter(|&&n| plate_id.get(n as usize) == Some(&plate_id[v]))
            .count();
        if same_neighbors == 0 {
            orphan_count += 1;
        }
    }
    orphan_count
}

fn single_cell_plate_count(plate_id: &[PlateId]) -> usize {
    let plate_count = plate_id
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let mut counts = vec![0usize; plate_count];
    for &pid in plate_id {
        let idx = pid.as_usize();
        if idx < counts.len() {
            counts[idx] += 1;
        }
    }
    counts.into_iter().filter(|&count| count == 1).count()
}

fn geology_activity_scale(world: &World) -> f32 {
    match world.clock.epoch {
        EraKind::Crust => 1.0,
        EraKind::Environment => {
            let elapsed = world
                .clock
                .tick
                .saturating_sub(world.clock.transition.era_enter_tick)
                as f32;
            let ramp = (elapsed / ENVIRONMENT_GEOLOGY_SPINUP_TICKS).clamp(0.0, 1.0);
            ENVIRONMENT_GEOLOGY_ACTIVITY_TARGET * ramp
        }
        _ => 1.0,
    }
}

fn ensure_geology_dynamics(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
) -> bool {
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
    let needs_rebuild = match geology_state.as_ref() {
        Some(state) => {
            state.vertex_states.len() != cell_count
                || state.mantle_heat.len() != cell_count
                || state.plate_states.len() != plate_count
        }
        None => true,
    };
    if !needs_rebuild {
        return false;
    }

    let plate_states = build_plate_states(
        &world.state.geology.plate_id,
        &world.state.geology.initial_plate_kinematics,
    );
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
        let age_ref = world.control.geology_params.age_ref.max(1e-4);
        let oceanic_base_density = world.control.geology_params.oceanic_base_density;
        let continental_density = world.control.geology_params.continental_crust_density;
        let age_density_gain = world.control.geology_params.age_density_gain.max(0.0);
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

    *geology_state = Some(GeologyDynamicsState {
        update_index: 0,
        plate_states,
        vertex_states,
        boundary_state: BoundaryDynamicsState {
            reclassify_interval_ticks: 4,
            steps_since_reclassify: 0,
            dominant_type: vec![BoundaryType::PassiveMargin; cell_count],
            activity: vec![0.0; cell_count],
            edge_pairs: Vec::new(),
            edge_pairs_plate_hash: 0,
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
    if let Some(dynamics) = geology_state.as_ref() {
        sync_geology_internal(
            &mut world.state.geology.geology_internal,
            &dynamics.vertex_states,
        );
    }
    true
}

fn debug_validate_geology_state_with_state(
    world: &World,
    dynamics: Option<&GeologyDynamicsState>,
    params: &GeologyParams,
    stage: &str,
) {
    let cell_count = world.state.geology.height.len();
    debug_assert_eq!(
        world.mesh().nbr_offsets.len(),
        cell_count.saturating_add(1),
        "{stage}: mesh neighbor offsets length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.plate_id.len(),
        cell_count,
        "{stage}: geology.plate_id length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.volcanism.len(),
        cell_count,
        "{stage}: geology.volcanism length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.vertex_buoyancy.len(),
        cell_count,
        "{stage}: geology.vertex_buoyancy length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.geology_internal.len(),
        cell_count,
        "{stage}: geology.geology_internal length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.boundary_condition.len(),
        cell_count,
        "{stage}: geology.boundary_condition length mismatch"
    );

    for (i, &height) in world.state.geology.height.iter().enumerate() {
        debug_assert!(
            height.is_finite() && (-1.5..=1.5).contains(&height),
            "{stage}: height[{i}] must be finite and in [-1.5, 1.5], got {height}"
        );
    }
    for (i, &volcanism) in world.state.geology.volcanism.iter().enumerate() {
        debug_assert_finite_non_negative(volcanism, "geology.volcanism", i);
    }

    if world.state.hydrology.river_next.len() == cell_count {
        debug_assert_river_next_no_cycle(&world.state.hydrology.river_next, "hydrology.river_next");
    }

    let Some(dynamics) = dynamics else {
        return;
    };

    debug_assert_eq!(
        dynamics.vertex_states.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.vertex_states length mismatch"
    );
    debug_assert_eq!(
        dynamics.mantle_heat.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.mantle_heat length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.dominant_type.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.dominant_type length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.activity.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.activity length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.rollback_fraction.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.rollback_fraction length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.backarc_tension.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.backarc_tension length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.slab_convergence_component.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.slab_convergence_component length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.slab_rollback_component.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.slab_rollback_component length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.edge_pairs.len(),
        dynamics.boundary_state.edge_internal.len(),
        "{stage}: boundary_state edge_pairs/edge_internal length mismatch"
    );
    for (i, &plate_id) in world.state.geology.plate_id.iter().enumerate() {
        debug_assert!(
            plate_id.as_usize() < dynamics.plate_states.len(),
            "{stage}: plate_id[{i}]={} is out of range for plate_states={}",
            plate_id.as_u32(),
            dynamics.plate_states.len()
        );
    }

    for (i, &mantle_heat) in dynamics.mantle_heat.iter().enumerate() {
        debug_assert_finite_unit_interval(mantle_heat, "runtime.geology_dynamics.mantle_heat", i);
    }
    for (i, state) in dynamics.vertex_states.iter().enumerate() {
        debug_assert_finite_non_negative(state.thickness, "vertex_states.thickness", i);
        debug_assert_finite_non_negative(state.density, "vertex_states.density", i);
        debug_assert_finite_non_negative(state.age, "vertex_states.age", i);
        debug_assert!(
            state.stress.is_finite(),
            "vertex_states.stress[{i}] must be finite"
        );
        debug_assert!(
            state.temperature.is_finite(),
            "vertex_states.temperature[{i}] must be finite"
        );
        debug_assert_finite_non_negative(state.rigidity, "vertex_states.rigidity", i);
        debug_assert_finite_non_negative(state.arc_volcanism, "vertex_states.arc_volcanism", i);
        debug_assert_finite_non_negative(state.ridge_volcanism, "vertex_states.ridge_volcanism", i);
        debug_assert_finite_non_negative(
            state.hotspot_volcanism,
            "vertex_states.hotspot_volcanism",
            i,
        );
        debug_assert_finite_non_negative(
            state.backarc_volcanism,
            "vertex_states.backarc_volcanism",
            i,
        );
        debug_assert!(
            state.stress_tensor.xx.is_finite()
                && state.stress_tensor.yy.is_finite()
                && state.stress_tensor.xy.is_finite(),
            "vertex_states.stress_tensor[{i}] must be finite"
        );
    }
    for (i, edge) in dynamics.boundary_state.edge_internal.iter().enumerate() {
        debug_assert_finite_unit_interval(
            edge.convergence_memory,
            "boundary_state.edge_internal.convergence_memory",
            i,
        );
    }
    for (i, &rollback_fraction) in dynamics.boundary_state.rollback_fraction.iter().enumerate() {
        debug_assert!(
            rollback_fraction.is_finite()
                && rollback_fraction >= 0.0
                && rollback_fraction <= params.rollback_fraction_max,
            "rollback_fraction[{i}] must be finite and in [0, {}], got {rollback_fraction}",
            params.rollback_fraction_max
        );
    }
    for (i, &value) in dynamics
        .boundary_state
        .slab_convergence_component
        .iter()
        .enumerate()
    {
        debug_assert!(
            value.is_finite(),
            "boundary_state.slab_convergence_component[{i}] must be finite"
        );
    }
    for (i, &value) in dynamics
        .boundary_state
        .slab_rollback_component
        .iter()
        .enumerate()
    {
        debug_assert!(
            value.is_finite(),
            "boundary_state.slab_rollback_component[{i}] must be finite"
        );
    }
}

fn build_plate_states(
    plate_ids: &[PlateId],
    initial_kinematics: &[crate::sim::geology_types::InitialPlateKinematics],
) -> Vec<PlateKinematicsState> {
    let plate_count = plate_ids
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let mut plate_states = Vec::with_capacity(plate_count);
    for plate in 0..plate_count {
        if let Some(initial) = initial_kinematics.get(plate) {
            plate_states.push(PlateKinematicsState {
                angular_axis: initial.angular_axis,
                angular_speed: initial.angular_speed,
                reference_angular_speed: initial.angular_speed,
                slab_pull_drive: 0.0,
                ridge_push_drive: 0.0,
                collision_drag: 0.0,
                force_target_speed_km_per_myr: 0.0,
                basal_target_speed_km_per_myr: 0.0,
                phase_offset: std::f32::consts::TAU * hash01(plate as u32 ^ 0x85eb_ca6b),
                activity: initial.activity.clamp(0.0, 1.0),
            });
            continue;
        }
        let seed = plate as u32;
        plate_states.push(PlateKinematicsState {
            angular_axis: seeded_axis(seed ^ 0x27d4_eb2f),
            angular_speed: 0.06 + 0.10 * hash01(seed ^ 0xc2b2_ae35),
            reference_angular_speed: 0.06 + 0.10 * hash01(seed ^ 0xc2b2_ae35),
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
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

    let age_ref = finite_or(params.age_ref.max(1e-4), 1.0);
    let mut density_min = params
        .continental_crust_density
        .min(params.oceanic_base_density)
        * 0.75;
    density_min = finite_or(density_min, 0.5).max(1e-4);
    let mut density_max = (params.oceanic_base_density + params.age_density_gain.max(0.0) + 0.2)
        .max(density_min + 1e-3);
    if !density_max.is_finite() || density_max < density_min {
        density_max = density_min + 1e-3;
    }
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
    let center = finite_or(center_value, 0.0);
    let mut raw = 0.0_f32;
    let mut count = 0_u32;
    let mut min_v = center;
    let mut max_v = center;
    for &n_u32 in neighbors {
        let n = n_u32 as usize;
        if n >= field.len() {
            continue;
        }
        let neighbor_value = field[n];
        if !neighbor_value.is_finite() {
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
        let dq = neighbor_value - center;
        if !dq.is_finite() {
            continue;
        }
        let projected_velocity = velocity[0] * dir[0] + velocity[1] * dir[1] + velocity[2] * dir[2];
        let contribution = dq * projected_velocity;
        if !contribution.is_finite() {
            continue;
        }
        raw += contribution;
        min_v = min_v.min(neighbor_value);
        max_v = max_v.max(neighbor_value);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return center;
    }
    if !min_v.is_finite() || !max_v.is_finite() || min_v > max_v {
        return center;
    }
    let predicted = center - dt * (raw / count as f32);
    if !predicted.is_finite() {
        return center;
    }
    predicted.clamp(min_v, max_v)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

struct BoundaryCrossingInput<'a> {
    positions: &'a [[f32; 3]],
    nbr_offsets: &'a [u32],
    nbrs: &'a [u32],
    plate_states: &'a [PlateKinematicsState],
    plate_id_prev: &'a [PlateId],
    boundary_state: &'a BoundaryDynamicsState,
    tick_seed: u32,
}

fn apply_boundary_crossing_discrete_attrs(
    input: BoundaryCrossingInput<'_>,
    plate_id_next: &mut [PlateId],
    vertex_states: &mut [VertexCrustState],
) -> u32 {
    let substeps = boundary_crossing_substeps(
        input.positions,
        input.nbr_offsets,
        input.nbrs,
        input.plate_states,
        input.plate_id_prev,
        input.boundary_state,
    );
    let distance_scale = 1.0 / substeps as f32;
    for substep in 0..substeps {
        let plate_id_prev = plate_id_next.to_vec();
        apply_boundary_crossing_discrete_attrs_substep(
            BoundaryCrossingInput {
                positions: input.positions,
                nbr_offsets: input.nbr_offsets,
                nbrs: input.nbrs,
                plate_states: input.plate_states,
                plate_id_prev: &plate_id_prev,
                boundary_state: input.boundary_state,
                tick_seed: input.tick_seed ^ substep.rotate_left(11),
            },
            plate_id_next,
            vertex_states,
            distance_scale,
        );
    }
    substeps
}

fn apply_boundary_crossing_discrete_attrs_substep(
    input: BoundaryCrossingInput<'_>,
    plate_id_next: &mut [PlateId],
    vertex_states: &mut [VertexCrustState],
    distance_scale: f32,
) {
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let plate_states = input.plate_states;
    let plate_id_prev = input.plate_id_prev;
    let boundary_state = input.boundary_state;
    let mut plate_sizes = plate_cell_counts(plate_id_prev);
    let donor_floor = runtime_boundary_crossing_donor_floor(plate_id_prev.len());

    let mut next_crust = vertex_states
        .iter()
        .map(|s| s.crust_type)
        .collect::<Vec<_>>();
    for i in 0..plate_id_prev.len() {
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let boundary_activity = boundary_state.activity.get(i).copied().unwrap_or(0.0);
        if boundary_activity <= 0.0 {
            continue;
        }
        let current_plate = plate_id_prev[i].as_usize();
        if plate_sizes.get(current_plate).copied().unwrap_or(0) <= donor_floor {
            continue;
        }
        if !removing_cell_preserves_plate_local_connectivity(nbr_offsets, nbrs, plate_id_prev, i) {
            continue;
        }

        let mut best_score = 0.0_f32;
        let mut best_plate = plate_id_prev[i];
        let mut best_crust = next_crust[i];
        let mut best_edge_spacing = 1.0_f32;
        let vel_i = plate_velocity_for_cell(plate_states, plate_id_prev[i], positions[i]);
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
            let neighbor_inflow = vel_n[0] * dir[0] + vel_n[1] * dir[1] + vel_n[2] * dir[2];
            let current_motion = vel_i[0] * dir[0] + vel_i[1] * dir[1] + vel_i[2] * dir[2];
            let relative_inflow = neighbor_inflow - current_motion;
            if current_motion < -1e-5 {
                continue;
            }
            let score = neighbor_inflow.min(relative_inflow).max(0.0);
            if score > best_score {
                best_score = score;
                best_plate = plate_id_prev[n];
                best_crust = vertex_states[n].crust_type;
                best_edge_spacing = len;
            }
        }
        let crossing_probability =
            boundary_crossing_probability(best_score * distance_scale, best_edge_spacing);
        if same_plate_neighbor_count(nbr_offsets, nbrs, plate_id_prev, i, best_plate)
            < MIN_BOUNDARY_CROSSING_TARGET_NEIGHBORS
        {
            continue;
        }
        let sample = hash01(
            input.tick_seed
                ^ (i as u32).wrapping_mul(0x9e37_79b9)
                ^ best_plate.as_u32().rotate_left(7),
        );
        if crossing_probability > 0.0 && sample <= crossing_probability {
            if let Some(count) = plate_sizes.get_mut(current_plate) {
                *count = count.saturating_sub(1);
            }
            if let Some(count) = plate_sizes.get_mut(best_plate.as_usize()) {
                *count = count.saturating_add(1);
            }
            plate_id_next[i] = best_plate;
            next_crust[i] = best_crust;
        }
    }
    for (i, crust) in next_crust.into_iter().enumerate() {
        vertex_states[i].crust_type = crust;
    }
}

fn runtime_boundary_crossing_donor_floor(cell_count: usize) -> usize {
    (cell_count / 2048)
        .clamp(
            MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS,
            MAX_BOUNDARY_CROSSING_DONOR_FLOOR_CELLS,
        )
        .max(MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS)
}

fn same_plate_neighbor_count(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    cell: usize,
    target_plate: PlateId,
) -> usize {
    let start = nbr_offsets[cell] as usize;
    let end = nbr_offsets[cell + 1] as usize;
    nbrs[start..end]
        .iter()
        .filter(|&&neighbor_u32| {
            plate_id
                .get(neighbor_u32 as usize)
                .copied()
                .is_some_and(|pid| pid == target_plate)
        })
        .count()
}

fn removing_cell_preserves_plate_local_connectivity(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    cell: usize,
) -> bool {
    let target_plate = plate_id[cell];
    let start = nbr_offsets[cell] as usize;
    let end = nbr_offsets[cell + 1] as usize;
    let same_neighbors = nbrs[start..end]
        .iter()
        .map(|&neighbor_u32| neighbor_u32 as usize)
        .filter(|&neighbor| plate_id.get(neighbor).copied() == Some(target_plate))
        .collect::<Vec<_>>();
    if same_neighbors.len() <= 1 {
        return true;
    }

    let mut visited = vec![false; same_neighbors.len()];
    let mut stack = vec![0usize];
    visited[0] = true;
    while let Some(index) = stack.pop() {
        let neighbor_cell = same_neighbors[index];
        let n_start = nbr_offsets[neighbor_cell] as usize;
        let n_end = nbr_offsets[neighbor_cell + 1] as usize;
        for &candidate_u32 in &nbrs[n_start..n_end] {
            let candidate = candidate_u32 as usize;
            if candidate == cell || plate_id.get(candidate).copied() != Some(target_plate) {
                continue;
            }
            if let Some(candidate_index) = same_neighbors
                .iter()
                .position(|&same_neighbor| same_neighbor == candidate)
            {
                if !visited[candidate_index] {
                    visited[candidate_index] = true;
                    stack.push(candidate_index);
                }
            }
        }
    }

    visited.into_iter().all(|value| value)
}

fn boundary_crossing_substeps(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_states: &[PlateKinematicsState],
    plate_id: &[PlateId],
    boundary_state: &BoundaryDynamicsState,
) -> u32 {
    let mut max_cell_fraction = 0.0_f32;
    for i in 0..plate_id.len() {
        if boundary_state.activity.get(i).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let velocity = plate_velocity_for_cell(plate_states, plate_id[i], positions[i]);
        let speed =
            (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2])
                .sqrt();
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= plate_id.len() || plate_id[n] == plate_id[i] {
                continue;
            }
            let dx = [
                positions[i][0] - positions[n][0],
                positions[i][1] - positions[n][1],
                positions[i][2] - positions[n][2],
            ];
            let spacing = (dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2])
                .sqrt()
                .max(1e-5);
            max_cell_fraction = max_cell_fraction.max(speed / spacing);
        }
    }

    finite_or(max_cell_fraction.ceil(), 1.0).clamp(1.0, 4.0) as u32
}

fn boundary_crossing_probability(inflow_distance: f32, edge_spacing_unit_sphere: f32) -> f32 {
    if inflow_distance <= 0.0 || edge_spacing_unit_sphere <= 0.0 {
        return 0.0;
    }

    let cell_fraction = finite_or(inflow_distance, 0.0).max(0.0)
        / finite_or(edge_spacing_unit_sphere, 1.0).max(1e-5);

    finite_or(cell_fraction, 0.0).clamp(0.0, 0.95)
}

fn plate_cell_counts(plate_id: &[PlateId]) -> Vec<usize> {
    let plate_count = plate_id
        .iter()
        .copied()
        .max()
        .map(|value| value.as_usize() + 1)
        .unwrap_or(0);
    let mut counts = vec![0usize; plate_count];
    for &pid in plate_id {
        let index = pid.as_usize();
        if let Some(count) = counts.get_mut(index) {
            *count += 1;
        }
    }
    counts
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

#[cfg(test)]
mod tests {
    use super::{
        boundary_crossing_probability, boundary_crossing_substeps,
        removing_cell_preserves_plate_local_connectivity, runtime_boundary_crossing_donor_floor,
        same_plate_neighbor_count,
    };
    use crate::sim::geology_types::PlateId;
    use crate::sim::world::{BoundaryDynamicsState, BoundaryType, PlateKinematicsState};

    #[test]
    fn boundary_crossing_probability_uses_actual_inflow_distance() {
        let probability = boundary_crossing_probability(0.03, 0.10);

        assert!((probability - 0.30).abs() < 1e-5);
    }

    #[test]
    fn boundary_crossing_probability_is_zero_without_inflow() {
        let inactive = boundary_crossing_probability(0.0, 0.10);
        let active = boundary_crossing_probability(0.03, 0.10);

        assert_eq!(inactive, 0.0);
        assert!(active > inactive);
    }

    #[test]
    fn boundary_crossing_substeps_follow_cell_crossing_scale() {
        let positions = vec![[1.0, 0.0, 0.0], [0.995, 0.1, 0.0]];
        let nbr_offsets = vec![0, 1, 2];
        let nbrs = vec![1, 0];
        let plate_id = vec![PlateId(0), PlateId(1)];
        let boundary_state = BoundaryDynamicsState {
            dominant_type: vec![BoundaryType::Subduction; 2],
            activity: vec![1.0; 2],
            ..Default::default()
        };
        let slow_states = vec![
            plate_state([0.0, 0.0, 1.0], 0.02),
            plate_state([0.0, 0.0, 1.0], 0.02),
        ];
        let fast_states = vec![
            plate_state([0.0, 0.0, 1.0], 0.30),
            plate_state([0.0, 0.0, 1.0], 0.30),
        ];

        assert_eq!(
            boundary_crossing_substeps(
                &positions,
                &nbr_offsets,
                &nbrs,
                &slow_states,
                &plate_id,
                &boundary_state,
            ),
            1
        );
        assert!(
            boundary_crossing_substeps(
                &positions,
                &nbr_offsets,
                &nbrs,
                &fast_states,
                &plate_id,
                &boundary_state,
            ) > 1
        );
    }

    #[test]
    fn runtime_boundary_crossing_donor_floor_scales_with_mesh_size() {
        assert_eq!(runtime_boundary_crossing_donor_floor(128), 3);
        assert_eq!(runtime_boundary_crossing_donor_floor(40_960), 20);
        assert_eq!(runtime_boundary_crossing_donor_floor(400_000), 24);
    }

    #[test]
    fn boundary_crossing_shape_guard_rejects_local_bridge_cells() {
        let nbr_offsets = vec![0, 1, 3, 4];
        let nbrs = vec![1, 0, 2, 1];
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(0)];

        assert!(!removing_cell_preserves_plate_local_connectivity(
            &nbr_offsets,
            &nbrs,
            &plate_id,
            1,
        ));
        assert_eq!(
            same_plate_neighbor_count(&nbr_offsets, &nbrs, &plate_id, 1, PlateId(0)),
            2
        );
    }

    fn plate_state(axis: [f32; 3], speed: f32) -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: axis,
            angular_speed: speed,
            reference_angular_speed: speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 1.0,
        }
    }
}
