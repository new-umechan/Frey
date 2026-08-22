use std::collections::HashMap;

use crate::sim::geology_types::{CrustType, PlateId};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, ConvergentRegime, PlateKinematicsState, VertexCrustState,
};
use crate::GeologyParams;

use crate::sim::exec::math::{cross3, dot, length3, seeded_axis};
use crate::sim::exec::{lerp, CONVERGENT_THRESHOLD, DIVERGENT_THRESHOLD, TRANSFORM_THRESHOLD};

use super::{
    EARTH_PLATE_REFERENCE_SPEED_KM_PER_MYR, PLATE_KINEMATIC_REFERENCE_STEP_MYR, YEARS_PER_MYR,
};

const EXPECTED_MOBILE_LID_DRIVE: f32 = 0.30;
const MAX_NORMALIZED_DRIVE: f32 = 1.8;

#[inline]
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn plate_id_signature(plate_id: &[PlateId]) -> u64 {
    // FNV-1a 64bit
    let mut hash = 0xcbf29ce484222325u64;
    hash ^= plate_id.len() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    for value in plate_id {
        hash ^= value.as_u32() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(super) struct ReclassifyBoundariesInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub plate_states: &'a [PlateKinematicsState],
    pub vertex_states: &'a [VertexCrustState],
    pub params: &'a GeologyParams,
}

pub(super) fn reclassify_boundaries(
    input: ReclassifyBoundariesInput<'_>,
    boundary_state: &mut BoundaryDynamicsState,
) {
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let plate_id = input.plate_id;
    let plate_states = input.plate_states;
    let vertex_states = input.vertex_states;
    let params = input.params;

    let cell_count = plate_id.len();
    if boundary_state.dominant_type.len() != cell_count {
        boundary_state.dominant_type = vec![BoundaryType::PassiveMargin; cell_count];
    }
    if boundary_state.activity.len() != cell_count {
        boundary_state.activity = vec![0.0; cell_count];
    }
    if boundary_state.rollback_fraction.len() != cell_count {
        boundary_state.rollback_fraction = vec![0.0; cell_count];
    }
    if boundary_state.backarc_tension.len() != cell_count {
        boundary_state.backarc_tension = vec![0.0; cell_count];
    }
    if boundary_state.slab_convergence_component.len() != cell_count {
        boundary_state.slab_convergence_component = vec![0.0; cell_count];
    }
    if boundary_state.slab_rollback_component.len() != cell_count {
        boundary_state.slab_rollback_component = vec![0.0; cell_count];
    }
    if boundary_state.convergence_component.len() != cell_count {
        boundary_state.convergence_component = vec![0.0; cell_count];
    }
    if boundary_state.divergence_component.len() != cell_count {
        boundary_state.divergence_component = vec![0.0; cell_count];
    }
    if boundary_state.transform_component.len() != cell_count {
        boundary_state.transform_component = vec![0.0; cell_count];
    }
    if boundary_state.obliquity.len() != cell_count {
        boundary_state.obliquity = vec![0.0; cell_count];
    }
    if boundary_state.subduction_gate.len() != cell_count {
        boundary_state.subduction_gate = vec![0.0; cell_count];
    }
    if boundary_state.subducting_plate.len() != cell_count {
        boundary_state.subducting_plate = vec![None; cell_count];
    }

    let current_plate_hash = plate_id_signature(plate_id);
    let needs_rebuild_edge_pairs = boundary_state.edge_pairs.is_empty()
        || boundary_state.edge_pairs_plate_hash != current_plate_hash;
    if needs_rebuild_edge_pairs {
        let previous_internal = boundary_state
            .edge_pairs
            .iter()
            .copied()
            .zip(boundary_state.edge_internal.iter().copied())
            .map(|(pair, internal)| (ordered_edge_pair(pair), internal))
            .collect::<HashMap<_, _>>();
        let mut edge_pairs = Vec::<[u32; 2]>::new();
        for i in 0..cell_count {
            let start = nbr_offsets[i] as usize;
            let end = nbr_offsets[i + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if n >= cell_count || i >= n || plate_id[i] == plate_id[n] {
                    continue;
                }
                edge_pairs.push([i as u32, n as u32]);
            }
        }
        boundary_state.edge_pairs = edge_pairs;
        boundary_state.edge_pairs_plate_hash = current_plate_hash;
        boundary_state.edge_internal = boundary_state
            .edge_pairs
            .iter()
            .copied()
            .map(|pair| {
                previous_internal
                    .get(&ordered_edge_pair(pair))
                    .copied()
                    .unwrap_or_default()
            })
            .collect();
    } else if boundary_state.edge_internal.len() != boundary_state.edge_pairs.len() {
        boundary_state.edge_internal = vec![Default::default(); boundary_state.edge_pairs.len()];
    }

    let edge_pairs = &boundary_state.edge_pairs;
    let mut convergence_norm_edge = vec![0.0_f32; edge_pairs.len()];
    let mut divergence_norm_edge = vec![0.0_f32; edge_pairs.len()];
    let mut transform_norm_edge = vec![0.0_f32; edge_pairs.len()];
    let mut obliquity_edge = vec![0.0_f32; edge_pairs.len()];
    let mut subduction_age_edge = vec![0.0_f32; edge_pairs.len()];
    let mut subduction_density_edge = vec![0.0_f32; edge_pairs.len()];
    let mut subduction_gate_edge = vec![0.0_f32; edge_pairs.len()];
    let mut edge_types = vec![BoundaryType::PassiveMargin; edge_pairs.len()];
    let mut edge_scores = vec![0.0_f32; edge_pairs.len()];
    let mut edge_convergent_regimes = vec![ConvergentRegime::None; edge_pairs.len()];
    let mut edge_convergent_plate = vec![None; edge_pairs.len()];

    for (eid, pair) in edge_pairs.iter().enumerate() {
        let i = pair[0] as usize;
        let j = pair[1] as usize;
        let rel = relative_kinematics(
            positions[i],
            positions[j],
            plate_states.get(plate_id[i].as_usize()),
            plate_states.get(plate_id[j].as_usize()),
            plate_id[i],
            plate_id[j],
        );

        let (mut bt, mut score) = classify_boundary_pair(
            rel.rel_n,
            rel.rel_t,
            vertex_states[i],
            vertex_states[j],
            params,
        );
        convergence_norm_edge[eid] = rel.convergence_norm;
        divergence_norm_edge[eid] = rel.divergence_norm;
        transform_norm_edge[eid] = rel.transform_norm;
        obliquity_edge[eid] = rel.obliquity;
        let edge_internal = &mut boundary_state.edge_internal[eid];
        let prev_memory = edge_internal.convergence_memory;
        let oceanic = densest_oceanic(vertex_states[i], vertex_states[j]);
        let strongly_convergent = rel.rel_n < -CONVERGENT_THRESHOLD;
        let immediately_eligible = bt == BoundaryType::Subduction;
        let buoyancy = oceanic
            .map(|state| oceanic_negative_buoyancy_proxy(state, params))
            .unwrap_or(0.0);
        let (progress, committed) = advance_subduction_initiation(
            edge_internal.subduction_initiation_progress,
            edge_internal.subduction_committed,
            strongly_convergent,
            oceanic.is_some(),
            rel.convergence_norm,
            buoyancy,
            immediately_eligible,
            params,
        );
        edge_internal.subduction_initiation_progress = progress;
        edge_internal.subduction_committed = committed;
        if committed {
            bt = BoundaryType::Subduction;
            score = score.max(progress);
        }
        edge_types[eid] = bt;
        edge_scores[eid] = finite_or(score, 0.0).clamp(0.0, 1.0);
        if rel.rel_n < -CONVERGENT_THRESHOLD {
            let (regime, candidate) = classify_convergent_regime(
                vertex_states[i],
                vertex_states[j],
                plate_id[i],
                plate_id[j],
                bt,
            );
            edge_convergent_regimes[eid] = regime;
            edge_convergent_plate[eid] = candidate;
        }

        if bt == BoundaryType::Subduction {
            if let Some(oceanic) = oceanic {
                subduction_age_edge[eid] = finite_or(oceanic.age, 0.0).max(0.0);
                subduction_density_edge[eid] = finite_or(oceanic.density, 0.0).max(0.0);
                subduction_gate_edge[eid] =
                    subduction_gate(oceanic, prev_memory, rel.convergence_norm, params);
            }
        }
    }

    boundary_state
        .dominant_type
        .fill(BoundaryType::PassiveMargin);
    boundary_state.activity.fill(0.0);
    boundary_state.convergence_component.fill(0.0);
    boundary_state.divergence_component.fill(0.0);
    boundary_state.transform_component.fill(0.0);
    boundary_state.obliquity.fill(0.0);
    boundary_state.subduction_gate.fill(0.0);
    boundary_state.subducting_plate.fill(None);
    for (eid, pair) in edge_pairs.iter().enumerate() {
        let bt = edge_types[eid];
        let score = edge_scores[eid];
        for cell in [pair[0] as usize, pair[1] as usize] {
            if score > boundary_state.activity[cell] {
                boundary_state.activity[cell] = score;
                boundary_state.dominant_type[cell] = bt;
                boundary_state.convergence_component[cell] = convergence_norm_edge[eid];
                boundary_state.divergence_component[cell] = divergence_norm_edge[eid];
                boundary_state.transform_component[cell] = transform_norm_edge[eid];
                boundary_state.obliquity[cell] = obliquity_edge[eid];
                boundary_state.subduction_gate[cell] = subduction_gate_edge[eid];
                boundary_state.subducting_plate[cell] = edge_convergent_plate[eid];
            }
        }
    }

    let memory_rate = params.convergence_memory_rate.clamp(0.0, 1.0);
    for (eid, edge_internal) in boundary_state.edge_internal.iter_mut().enumerate() {
        let prev = edge_internal.convergence_memory;
        let next = prev + (convergence_norm_edge[eid] - prev) * memory_rate;
        edge_internal.convergence_memory = next.clamp(0.0, 1.0);
    }

    let smooth_mix = params.convergence_memory_spatial_smooth.clamp(0.0, 1.0);
    let old_memory = boundary_state
        .edge_internal
        .iter()
        .map(|edge| edge.convergence_memory)
        .collect::<Vec<_>>();
    let mut cell_memory_sum = vec![0.0_f32; cell_count];
    let mut cell_edge_count = vec![0_u32; cell_count];
    for (eid, pair) in edge_pairs.iter().enumerate() {
        let mem = finite_or(old_memory[eid], 0.0);
        let a = pair[0] as usize;
        let b = pair[1] as usize;
        cell_memory_sum[a] += mem;
        cell_memory_sum[b] += mem;
        cell_edge_count[a] = cell_edge_count[a].saturating_add(1);
        cell_edge_count[b] = cell_edge_count[b].saturating_add(1);
    }

    for (eid, pair) in edge_pairs.iter().enumerate() {
        let mem = finite_or(old_memory[eid], 0.0);
        let a = pair[0] as usize;
        let b = pair[1] as usize;

        let mut acc = 0.0_f32;
        let mut cnt = 0_u32;

        if cell_edge_count[a] > 1 {
            acc += cell_memory_sum[a] - mem;
            cnt = cnt.saturating_add(cell_edge_count[a] - 1);
        }
        if cell_edge_count[b] > 1 {
            acc += cell_memory_sum[b] - mem;
            cnt = cnt.saturating_add(cell_edge_count[b] - 1);
        }
        if cnt == 0 {
            continue;
        }
        boundary_state.edge_internal[eid].convergence_memory =
            finite_or(lerp(mem, acc / cnt as f32, smooth_mix), mem).clamp(0.0, 1.0);
    }

    boundary_state.rollback_fraction.fill(0.0);
    boundary_state.backarc_tension.fill(0.0);
    boundary_state.slab_convergence_component.fill(0.0);
    boundary_state.slab_rollback_component.fill(0.0);

    let dip_density_scale = params.dip_density_scale.max(1e-4);
    let age_ref = params.age_ref.max(1e-4);
    let mut cell_rollback_count = vec![0_u32; cell_count];

    for (eid, pair) in edge_pairs.iter().enumerate() {
        if edge_types[eid] != BoundaryType::Subduction {
            continue;
        }

        let age_norm = finite_or(subduction_age_edge[eid] / age_ref, 0.0).clamp(0.0, 1.0);
        let density_ocean = finite_or(subduction_density_edge[eid], params.oceanic_base_density);
        let density_age_factor =
            ((density_ocean - params.oceanic_base_density) / dip_density_scale).clamp(0.0, 1.0);
        let negative_buoyancy_proxy =
            finite_or(0.5 * age_norm + 0.5 * density_age_factor, 0.0).clamp(0.0, 1.0);
        let memory = finite_or(boundary_state.edge_internal[eid].convergence_memory, 0.0);
        let slab_depth_est = params.subduction_depth_gain.max(0.0) * age_norm * memory;
        let suppression = finite_or(
            1.0 - convergence_norm_edge[eid] * params.rollback_suppression.max(0.0),
            1.0,
        )
        .clamp(0.0, 1.0);
        let rollback = finite_or(
            params.rollback_gain.max(0.0) * negative_buoyancy_proxy * slab_depth_est * suppression,
            0.0,
        )
        .clamp(0.0, params.rollback_fraction_max.max(0.0));

        let kinematic_coupling = edge_scores[eid].max(memory * 0.5).clamp(0.0, 1.0);
        let slab_pull_mag = finite_or(
            kinematic_coupling * negative_buoyancy_proxy * (1.0 + slab_depth_est),
            0.0,
        );
        let slab_conv = slab_pull_mag * (1.0 - rollback);
        let slab_roll = slab_pull_mag * rollback;
        let backarc = if rollback > params.rollback_threshold.max(0.0) {
            slab_pull_mag * rollback * params.backarc_tension_gain.max(0.0)
        } else {
            0.0
        };

        for cell in [pair[0] as usize, pair[1] as usize] {
            boundary_state.rollback_fraction[cell] += rollback;
            boundary_state.backarc_tension[cell] += backarc;
            boundary_state.slab_convergence_component[cell] += slab_conv;
            boundary_state.slab_rollback_component[cell] += slab_roll;
            cell_rollback_count[cell] = cell_rollback_count[cell].saturating_add(1);
        }
    }

    for (i, count) in cell_rollback_count.iter().enumerate().take(cell_count) {
        let denom = (*count).max(1) as f32;
        boundary_state.rollback_fraction[i] =
            finite_or(boundary_state.rollback_fraction[i] / denom, 0.0)
                .clamp(0.0, params.rollback_fraction_max.max(0.0));
        boundary_state.backarc_tension[i] =
            finite_or(boundary_state.backarc_tension[i] / denom, 0.0);
        boundary_state.slab_convergence_component[i] =
            finite_or(boundary_state.slab_convergence_component[i] / denom, 0.0);
        boundary_state.slab_rollback_component[i] =
            finite_or(boundary_state.slab_rollback_component[i] / denom, 0.0);
    }
    boundary_state.edge_types = edge_types;
    boundary_state.edge_activity = edge_scores;
    boundary_state.edge_convergent_regimes = edge_convergent_regimes;
    boundary_state.edge_convergent_plate = edge_convergent_plate;
}

fn advance_subduction_initiation(
    progress: f32,
    committed: bool,
    strongly_convergent: bool,
    has_oceanic_candidate: bool,
    convergence_norm: f32,
    negative_buoyancy: f32,
    immediately_eligible: bool,
    params: &GeologyParams,
) -> (f32, bool) {
    let rate = params.convergence_memory_rate.clamp(0.0, 1.0);
    if !strongly_convergent || !has_oceanic_candidate {
        return ((progress - rate).max(0.0), false);
    }
    if committed || immediately_eligible {
        return (1.0, true);
    }
    let next = (progress + rate * convergence_norm * negative_buoyancy).clamp(0.0, 1.0);
    let committed = next >= params.subduction_initiation_threshold.clamp(0.0, 1.0);
    (next, committed)
}

fn ordered_edge_pair(pair: [u32; 2]) -> (u32, u32) {
    if pair[0] < pair[1] {
        (pair[0], pair[1])
    } else {
        (pair[1], pair[0])
    }
}

fn classify_convergent_regime(
    a: VertexCrustState,
    b: VertexCrustState,
    plate_a: PlateId,
    plate_b: PlateId,
    boundary_type: BoundaryType,
) -> (ConvergentRegime, Option<PlateId>) {
    if a.crust_type == CrustType::Continental && b.crust_type == CrustType::Continental {
        return (ConvergentRegime::ContinentalCollision, None);
    }
    let candidate = match (
        a.crust_type == CrustType::Oceanic,
        b.crust_type == CrustType::Oceanic,
    ) {
        (true, false) => plate_a,
        (false, true) => plate_b,
        (true, true) if a.density > b.density => plate_a,
        (true, true) if b.density > a.density => plate_b,
        (true, true) if a.age >= b.age => plate_a,
        (true, true) => plate_b,
        (false, false) => return (ConvergentRegime::ContinentalCollision, None),
    };
    let regime = if boundary_type == BoundaryType::Subduction {
        ConvergentRegime::Subduction
    } else {
        ConvergentRegime::IncipientSubduction
    };
    (regime, Some(candidate))
}

#[derive(Clone, Copy)]
struct RelativeKinematics {
    rel_n: f32,
    rel_t: f32,
    convergence_norm: f32,
    divergence_norm: f32,
    transform_norm: f32,
    obliquity: f32,
}

fn relative_kinematics(
    pos_i: [f32; 3],
    pos_j: [f32; 3],
    state_i: Option<&PlateKinematicsState>,
    state_j: Option<&PlateKinematicsState>,
    plate_i: PlateId,
    plate_j: PlateId,
) -> RelativeKinematics {
    let edge_vec = [
        pos_j[0] - pos_i[0],
        pos_j[1] - pos_i[1],
        pos_j[2] - pos_i[2],
    ];
    let edge_len = length3(edge_vec).max(1e-5);
    let edge_dir = [
        edge_vec[0] / edge_len,
        edge_vec[1] / edge_len,
        edge_vec[2] / edge_len,
    ];
    let vel_i = plate_velocity_from_state(state_i, plate_i, pos_i);
    let vel_j = plate_velocity_from_state(state_j, plate_j, pos_j);
    let rel_v = [
        vel_j[0] - vel_i[0],
        vel_j[1] - vel_i[1],
        vel_j[2] - vel_i[2],
    ];
    decompose_relative_kinematics(rel_v, edge_dir)
}

fn decompose_relative_kinematics(
    relative_velocity: [f32; 3],
    boundary_normal: [f32; 3],
) -> RelativeKinematics {
    let rel_n = dot(relative_velocity, boundary_normal);
    let rel_mag = length3(relative_velocity);
    let rel_t = (rel_mag * rel_mag - rel_n * rel_n).max(0.0).sqrt();
    // rel_n is the time derivative of endpoint separation: positive opens the edge.
    let convergence = (-rel_n).max(0.0);
    let divergence = rel_n.max(0.0);
    let obliquity = rel_t / (convergence + divergence + rel_t + 1e-5);
    RelativeKinematics {
        rel_n,
        rel_t,
        convergence_norm: finite_or(convergence * 8.0, 0.0).clamp(0.0, 1.0),
        divergence_norm: finite_or(divergence * 8.0, 0.0).clamp(0.0, 1.0),
        transform_norm: finite_or(rel_t * 7.0, 0.0).clamp(0.0, 1.0),
        obliquity: finite_or(obliquity, 0.0).clamp(0.0, 1.0),
    }
}

pub(super) fn plate_velocity_for_cell(
    plate_states: &[PlateKinematicsState],
    plate_id: PlateId,
    pos: [f32; 3],
) -> [f32; 3] {
    plate_velocity_from_state(plate_states.get(plate_id.as_usize()), plate_id, pos)
}

pub(super) fn plate_kinematics_for_elapsed_years(
    plate_states: &[PlateKinematicsState],
    elapsed_years: f32,
) -> Vec<PlateKinematicsState> {
    let elapsed_myr = finite_or(elapsed_years / YEARS_PER_MYR, 0.0).max(0.0);
    let step_scale = elapsed_myr / PLATE_KINEMATIC_REFERENCE_STEP_MYR;
    plate_states
        .iter()
        .cloned()
        .map(|mut state| {
            state.angular_speed *= step_scale;
            state
        })
        .collect()
}

pub(super) fn update_plate_kinematics(
    plate_id: &[PlateId],
    plate_states: &mut [PlateKinematicsState],
    boundary_state: &BoundaryDynamicsState,
    params: &GeologyParams,
) {
    if plate_states.is_empty() {
        return;
    }

    let mut plate_activity = vec![0.0_f32; plate_states.len()];
    let mut plate_slab_convergence = vec![0.0_f32; plate_states.len()];
    let mut plate_slab_rollback = vec![0.0_f32; plate_states.len()];
    let mut plate_ridge_activity = vec![0.0_f32; plate_states.len()];
    let mut plate_collision_activity = vec![0.0_f32; plate_states.len()];
    let mut plate_count = vec![0_u32; plate_states.len()];
    let mut plate_boundary_count = vec![0_u32; plate_states.len()];

    for (i, plate) in plate_id.iter().enumerate() {
        let pid = plate.as_usize();
        if pid >= plate_states.len() {
            continue;
        }
        let activity =
            finite_or(boundary_state.activity.get(i).copied().unwrap_or(0.0), 0.0).clamp(0.0, 1.0);
        plate_activity[pid] += activity;
        if activity > 0.0 {
            plate_boundary_count[pid] = plate_boundary_count[pid].saturating_add(1);
        }
        plate_slab_convergence[pid] += finite_or(
            boundary_state
                .slab_convergence_component
                .get(i)
                .copied()
                .unwrap_or(0.0),
            0.0,
        )
        .max(0.0);
        plate_slab_rollback[pid] += finite_or(
            boundary_state
                .slab_rollback_component
                .get(i)
                .copied()
                .unwrap_or(0.0),
            0.0,
        )
        .max(0.0);
        match boundary_state
            .dominant_type
            .get(i)
            .copied()
            .unwrap_or(BoundaryType::PassiveMargin)
        {
            BoundaryType::Ridge | BoundaryType::Rift => plate_ridge_activity[pid] += activity,
            BoundaryType::Collision => plate_collision_activity[pid] += activity,
            _ => {}
        }
        plate_count[pid] = plate_count[pid].saturating_add(1);
    }

    let gain = params.plate_motion_gain.max(0.0);
    for pid in 0..plate_states.len() {
        let denom = plate_boundary_count[pid].max(1) as f32;
        let activity = finite_or(plate_activity[pid] / denom, 0.0).clamp(0.0, 1.0);
        let slab_convergence = finite_or(plate_slab_convergence[pid] / denom, 0.0).max(0.0);
        let slab_rollback = finite_or(plate_slab_rollback[pid] / denom, 0.0).max(0.0);
        let ridge_push = finite_or(plate_ridge_activity[pid] / denom, 0.0).clamp(0.0, 1.0);
        let collision_drag = finite_or(plate_collision_activity[pid] / denom, 0.0).clamp(0.0, 1.0);
        let slab_pull_drive = slab_convergence + 0.5 * slab_rollback;
        let ridge_push_drive = 0.35 * ridge_push;
        let activity_drive = 0.10 * activity;
        let driving_strength =
            finite_or(slab_pull_drive + ridge_push_drive + activity_drive, 0.0).max(0.0);
        let normalized_driving_strength =
            (driving_strength / EXPECTED_MOBILE_LID_DRIVE).clamp(0.0, MAX_NORMALIZED_DRIVE);
        let drag = 1.0 + 2.0 * collision_drag;
        let target_speed_km_per_myr =
            EARTH_PLATE_REFERENCE_SPEED_KM_PER_MYR * gain * normalized_driving_strength / drag;
        let reference_angular_step = finite_or(
            plate_states[pid].reference_angular_speed,
            plate_states[pid].angular_speed,
        )
        .clamp(0.0, 0.30);
        let kinematic_angular_step = (reference_angular_step * gain).clamp(0.0, 0.30);
        plate_states[pid].angular_speed = kinematic_angular_step;
        plate_states[pid].slab_pull_drive = slab_pull_drive;
        plate_states[pid].ridge_push_drive = ridge_push_drive;
        plate_states[pid].collision_drag = collision_drag;
        plate_states[pid].force_target_speed_km_per_myr = target_speed_km_per_myr;
        plate_states[pid].basal_target_speed_km_per_myr = 0.0;
        plate_states[pid].activity =
            finite_or(lerp(plate_states[pid].activity, activity, 0.20), activity).clamp(0.0, 1.0);
    }
}

fn classify_boundary_pair(
    rel_n: f32,
    rel_t: f32,
    a: VertexCrustState,
    b: VertexCrustState,
    params: &GeologyParams,
) -> (BoundaryType, f32) {
    if rel_n > DIVERGENT_THRESHOLD {
        let bt = if a.crust_type == CrustType::Continental && b.crust_type == CrustType::Continental
        {
            BoundaryType::Rift
        } else {
            BoundaryType::Ridge
        };
        return (bt, (rel_n * 8.0 + rel_t * 2.0).clamp(0.0, 1.0));
    }

    if rel_n < -CONVERGENT_THRESHOLD {
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
            let age_norm = (oceanic_state.age / params.age_ref.max(1e-4)).clamp(0.0, 1.0);
            let density_norm =
                (oceanic_state.density / params.mantle_density.max(1e-3)).clamp(0.0, 2.0);
            let age_gate = age_norm > params.subduction_initiation_threshold;
            let density_gate = density_norm > params.subduction_density_threshold;
            let age_coupled =
                (age_norm * params.subduction_age_coupling + density_norm).clamp(0.0, 2.0);
            if age_gate && density_gate || age_coupled > 1.0 {
                bt = BoundaryType::Subduction;
            }
        }
        return (bt, (-rel_n * 8.0 + rel_t).clamp(0.0, 1.0));
    }

    if rel_t > TRANSFORM_THRESHOLD {
        return (BoundaryType::Transform, (rel_t * 7.0).clamp(0.0, 1.0));
    }

    (BoundaryType::PassiveMargin, 0.03)
}

fn densest_oceanic(a: VertexCrustState, b: VertexCrustState) -> Option<VertexCrustState> {
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
    oceanic
}

fn oceanic_negative_buoyancy_proxy(state: VertexCrustState, params: &GeologyParams) -> f32 {
    let age_norm = finite_or(state.age / params.age_ref.max(1e-4), 0.0).clamp(0.0, 1.0);
    let density_age_factor = finite_or(
        (state.density - params.oceanic_base_density) / params.dip_density_scale.max(1e-4),
        0.0,
    )
    .clamp(0.0, 1.0);
    finite_or(0.5 * age_norm + 0.5 * density_age_factor, 0.0).clamp(0.0, 1.0)
}

fn subduction_gate(
    state: VertexCrustState,
    convergence_memory: f32,
    convergence_norm: f32,
    params: &GeologyParams,
) -> f32 {
    let age_norm = finite_or(state.age / params.age_ref.max(1e-4), 0.0).clamp(0.0, 1.0);
    let density_age_factor = finite_or(
        (state.density - params.oceanic_base_density) / params.dip_density_scale.max(1e-4),
        0.0,
    )
    .clamp(0.0, 1.0);
    finite_or(
        0.45 * age_norm
            + 0.30 * density_age_factor
            + 0.15 * convergence_memory.clamp(0.0, 1.0)
            + 0.10 * convergence_norm.clamp(0.0, 1.0),
        0.0,
    )
    .clamp(0.0, 1.0)
}

fn plate_velocity_from_state(
    state: Option<&PlateKinematicsState>,
    plate_id: PlateId,
    pos: [f32; 3],
) -> [f32; 3] {
    let seed = plate_id.as_u32();
    let fallback_axis = seeded_axis(seed ^ 0x27d4_eb2f);
    let angular_axis = state
        .map(|s| {
            [
                finite_or(s.angular_axis[0], fallback_axis[0]),
                finite_or(s.angular_axis[1], fallback_axis[1]),
                finite_or(s.angular_axis[2], fallback_axis[2]),
            ]
        })
        .unwrap_or(fallback_axis);
    let angular_speed = state
        .map(|s| finite_or(s.angular_speed, 0.12))
        .unwrap_or(0.12);
    let omega = [
        angular_axis[0] * angular_speed,
        angular_axis[1] * angular_speed,
        angular_axis[2] * angular_speed,
    ];
    cross3(omega, pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::geology_types::CrustType;

    fn plate_state() -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: [0.0, 1.0, 0.0],
            angular_speed: 0.0,
            reference_angular_speed: 0.0,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 0.0,
        }
    }

    fn oceanic_state(age: f32, density: f32) -> VertexCrustState {
        VertexCrustState {
            crust_type: CrustType::Oceanic,
            thickness: 0.45,
            density,
            age,
            stress: 0.0,
            temperature: 0.0,
            rigidity: 30e9,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: Default::default(),
        }
    }

    fn continental_state() -> VertexCrustState {
        VertexCrustState {
            crust_type: CrustType::Continental,
            ..oceanic_state(0.0, 2_700.0)
        }
    }

    fn opposing_z_rotation_states(left_speed: f32, right_speed: f32) -> Vec<PlateKinematicsState> {
        let mut left = plate_state();
        left.angular_axis = [0.0, 0.0, 1.0];
        left.angular_speed = left_speed;
        let mut right = plate_state();
        right.angular_axis = [0.0, 0.0, 1.0];
        right.angular_speed = right_speed;
        vec![left, right]
    }

    #[test]
    fn relative_normal_velocity_sign_matches_endpoint_separation() {
        let left_position = [1.0, 0.0, 0.0];
        let right_position = [0.995, 0.1, 0.0];
        let divergent_states = opposing_z_rotation_states(-0.1, 0.1);
        let convergent_states = opposing_z_rotation_states(0.1, -0.1);

        let divergent = relative_kinematics(
            left_position,
            right_position,
            divergent_states.first(),
            divergent_states.get(1),
            PlateId(0),
            PlateId(1),
        );
        let convergent = relative_kinematics(
            left_position,
            right_position,
            convergent_states.first(),
            convergent_states.get(1),
            PlateId(0),
            PlateId(1),
        );

        assert!(divergent.rel_n > 0.0);
        assert_eq!(divergent.convergence_norm, 0.0);
        assert!(divergent.divergence_norm > 0.0);
        assert!(convergent.rel_n < 0.0);
        assert!(convergent.convergence_norm > 0.0);
        assert_eq!(convergent.divergence_norm, 0.0);
    }

    #[test]
    fn boundary_classification_uses_separation_sign() {
        let crust = continental_state();
        let params = GeologyParams::default();

        assert_eq!(
            classify_boundary_pair(0.1, 0.0, crust, crust, &params).0,
            BoundaryType::Rift
        );
        assert_eq!(
            classify_boundary_pair(-0.1, 0.0, crust, crust, &params).0,
            BoundaryType::Collision
        );
    }

    #[test]
    fn relative_kinematics_separates_normal_and_tangent_components() {
        let normal = [1.0, 0.0, 0.0];

        let convergent = decompose_relative_kinematics([-0.1, 0.0, 0.0], normal);
        let divergent = decompose_relative_kinematics([0.1, 0.0, 0.0], normal);
        let transform = decompose_relative_kinematics([0.0, 0.1, 0.0], normal);

        assert!(convergent.convergence_norm > 0.0);
        assert_eq!(convergent.divergence_norm, 0.0);
        assert_eq!(convergent.transform_norm, 0.0);
        assert!(divergent.divergence_norm > 0.0);
        assert_eq!(divergent.convergence_norm, 0.0);
        assert_eq!(divergent.transform_norm, 0.0);
        assert_eq!(transform.convergence_norm, 0.0);
        assert_eq!(transform.divergence_norm, 0.0);
        assert!(transform.transform_norm > 0.0);
    }

    #[test]
    fn tangent_velocity_does_not_change_normal_components() {
        let normal = [1.0, 0.0, 0.0];
        let without_tangent = decompose_relative_kinematics([-0.08, 0.0, 0.0], normal);
        let with_tangent = decompose_relative_kinematics([-0.08, 0.12, 0.0], normal);

        assert!((with_tangent.rel_n - without_tangent.rel_n).abs() <= 1e-6);
        assert!((with_tangent.convergence_norm - without_tangent.convergence_norm).abs() <= 1e-6);
        assert_eq!(
            with_tangent.divergence_norm,
            without_tangent.divergence_norm
        );
        assert!(with_tangent.transform_norm > without_tangent.transform_norm);
    }

    #[test]
    fn swapping_edge_endpoints_preserves_physical_components() {
        let forward = decompose_relative_kinematics([-0.08, 0.12, 0.0], [1.0, 0.0, 0.0]);
        let reversed = decompose_relative_kinematics([0.08, -0.12, 0.0], [-1.0, 0.0, 0.0]);

        assert!((forward.rel_n - reversed.rel_n).abs() <= 1e-6);
        assert!((forward.rel_t - reversed.rel_t).abs() <= 1e-6);
        assert!((forward.convergence_norm - reversed.convergence_norm).abs() <= 1e-6);
        assert!((forward.divergence_norm - reversed.divergence_norm).abs() <= 1e-6);
        assert!((forward.transform_norm - reversed.transform_norm).abs() <= 1e-6);
        assert!((forward.obliquity - reversed.obliquity).abs() <= 1e-6);
    }

    #[test]
    fn relative_kinematics_is_invariant_under_global_rotation() {
        fn rotate_z(vector: [f32; 3], angle: f32) -> [f32; 3] {
            let (sin, cos) = angle.sin_cos();
            [
                cos * vector[0] - sin * vector[1],
                sin * vector[0] + cos * vector[1],
                vector[2],
            ]
        }

        let relative_velocity = [-0.08, 0.12, 0.03];
        let boundary_normal = [1.0, 0.0, 0.0];
        let reference = decompose_relative_kinematics(relative_velocity, boundary_normal);

        for angle in [
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            -std::f32::consts::FRAC_PI_2,
        ] {
            let rotated = decompose_relative_kinematics(
                rotate_z(relative_velocity, angle),
                rotate_z(boundary_normal, angle),
            );

            assert!((reference.rel_n - rotated.rel_n).abs() <= 1e-6);
            assert!((reference.rel_t - rotated.rel_t).abs() <= 1e-6);
            assert!((reference.convergence_norm - rotated.convergence_norm).abs() <= 1e-6);
            assert!((reference.divergence_norm - rotated.divergence_norm).abs() <= 1e-6);
            assert!((reference.transform_norm - rotated.transform_norm).abs() <= 1e-6);
            assert!((reference.obliquity - rotated.obliquity).abs() <= 1e-6);
        }
    }

    #[test]
    fn convergent_regime_separates_collision_from_subduction_onset() {
        let continental = continental_state();
        let oceanic = oceanic_state(10.0, 2_950.0);

        assert_eq!(
            classify_convergent_regime(
                continental,
                continental,
                PlateId(0),
                PlateId(1),
                BoundaryType::Collision,
            ),
            (ConvergentRegime::ContinentalCollision, None)
        );
        assert_eq!(
            classify_convergent_regime(
                oceanic,
                continental,
                PlateId(2),
                PlateId(3),
                BoundaryType::Collision,
            ),
            (ConvergentRegime::IncipientSubduction, Some(PlateId(2)))
        );
        assert_eq!(
            classify_convergent_regime(
                oceanic,
                continental,
                PlateId(2),
                PlateId(3),
                BoundaryType::Subduction,
            ),
            (ConvergentRegime::Subduction, Some(PlateId(2)))
        );
    }

    #[test]
    fn subduction_initiation_progress_commits_and_releases_from_kinematics() {
        let mut params = GeologyParams::default();
        params.convergence_memory_rate = 0.5;
        params.subduction_initiation_threshold = 0.5;

        let (progress, committed) =
            advance_subduction_initiation(0.3, false, true, true, 1.0, 0.5, false, &params);
        assert_eq!(progress, 0.55);
        assert!(committed);

        let (progress, committed) = advance_subduction_initiation(
            progress, committed, true, true, 0.0, 0.0, false, &params,
        );
        assert_eq!(progress, 1.0);
        assert!(committed);

        let (progress, committed) = advance_subduction_initiation(
            progress, committed, false, true, 0.0, 0.0, false, &params,
        );
        assert_eq!(progress, 0.5);
        assert!(!committed);
    }

    #[test]
    fn slab_pull_is_diagnostic_not_kinematic_authority() {
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let mut plate_states = vec![plate_state(), plate_state()];
        let mut boundary_state = BoundaryDynamicsState {
            dominant_type: vec![
                BoundaryType::Subduction,
                BoundaryType::Subduction,
                BoundaryType::PassiveMargin,
                BoundaryType::PassiveMargin,
            ],
            activity: vec![0.5, 0.5, 0.5, 0.5],
            slab_convergence_component: vec![1.0, 1.0, 0.0, 0.0],
            slab_rollback_component: vec![0.2, 0.2, 0.0, 0.0],
            ..Default::default()
        };

        update_plate_kinematics(
            &plate_id,
            &mut plate_states,
            &boundary_state,
            &GeologyParams::default(),
        );

        assert_eq!(plate_states[0].angular_speed, plate_states[1].angular_speed);
        assert!(plate_states[0].slab_pull_drive > plate_states[0].ridge_push_drive);
        assert_eq!(plate_states[1].slab_pull_drive, 0.0);
        assert!(plate_states[0].force_target_speed_km_per_myr > 0.0);

        boundary_state.slab_convergence_component.fill(0.0);
        boundary_state.slab_rollback_component.fill(0.0);
        let kinematic_speed = plate_states[0].angular_speed;
        update_plate_kinematics(
            &plate_id,
            &mut plate_states,
            &boundary_state,
            &GeologyParams::default(),
        );

        assert_eq!(plate_states[0].angular_speed, kinematic_speed);
    }

    #[test]
    fn reference_kinematics_is_stable_across_updates() {
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let mut plate_states = vec![plate_state(), plate_state()];
        plate_states[0].angular_speed = 0.10;
        plate_states[0].reference_angular_speed = 0.10;
        plate_states[1].angular_speed = 0.08;
        plate_states[1].reference_angular_speed = 0.08;
        let boundary_state = BoundaryDynamicsState {
            dominant_type: vec![BoundaryType::PassiveMargin; 4],
            activity: vec![0.0; 4],
            ..Default::default()
        };

        for _ in 0..80 {
            update_plate_kinematics(
                &plate_id,
                &mut plate_states,
                &boundary_state,
                &GeologyParams::default(),
            );
        }

        assert_eq!(plate_states[0].angular_speed, 0.10);
        assert_eq!(plate_states[1].angular_speed, 0.08);

        update_plate_kinematics(
            &plate_id,
            &mut plate_states,
            &boundary_state,
            &GeologyParams::default(),
        );

        assert_eq!(plate_states[0].angular_speed, 0.10);
        assert_eq!(plate_states[1].angular_speed, 0.08);
    }

    #[test]
    fn elapsed_time_scales_finite_euler_rotation_linearly() {
        let state = PlateKinematicsState {
            angular_speed: 0.10,
            reference_angular_speed: 0.10,
            ..plate_state()
        };

        let five_myr = plate_kinematics_for_elapsed_years(&[state], 5_000_000.0);
        let one_myr = plate_kinematics_for_elapsed_years(&[state], 1_000_000.0);
        let one_year = plate_kinematics_for_elapsed_years(&[state], 1.0);

        assert!((five_myr[0].angular_speed - 0.10).abs() < 1e-6);
        assert!((one_myr[0].angular_speed - 0.02).abs() < 1e-6);
        assert!((one_year[0].angular_speed - 0.10 / 5_000_000.0).abs() < 1e-12);
    }

    #[test]
    fn plate_velocity_uses_speed_not_boundary_activity_as_motion_scale() {
        let inactive = PlateKinematicsState {
            angular_speed: 0.10,
            reference_angular_speed: 0.10,
            activity: 0.0,
            ..plate_state()
        };
        let active = PlateKinematicsState {
            activity: 1.0,
            ..inactive
        };
        let pos = [1.0, 0.0, 0.0];

        let inactive_velocity = plate_velocity_from_state(Some(&inactive), PlateId(0), pos);
        let active_velocity = plate_velocity_from_state(Some(&active), PlateId(0), pos);

        assert_eq!(inactive_velocity, active_velocity);
    }

    #[test]
    fn boundary_reclassification_persists_edge_type_for_material_reaction() {
        let positions = vec![[1.0, 0.0, 0.0], [0.995, 0.1, 0.0]];
        let nbr_offsets = vec![0, 1, 2];
        let nbrs = vec![1, 0];
        let plate_id = vec![PlateId(0), PlateId(1)];
        let plate_states = opposing_z_rotation_states(-0.1, 0.1);
        let vertex_states = vec![oceanic_state(20.0, 3_000.0); 2];
        let mut boundary_state = BoundaryDynamicsState::default();

        reclassify_boundaries(
            ReclassifyBoundariesInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &plate_id,
                plate_states: &plate_states,
                vertex_states: &vertex_states,
                params: &GeologyParams::default(),
            },
            &mut boundary_state,
        );

        assert_eq!(boundary_state.edge_pairs, vec![[0, 1]]);
        assert_eq!(boundary_state.edge_types, vec![BoundaryType::Ridge]);
        assert_eq!(boundary_state.edge_activity.len(), 1);
        assert!(boundary_state.edge_activity[0] > 0.0);
    }

    #[test]
    fn boundary_reclassification_records_the_subducting_plate() {
        let positions = vec![[1.0, 0.0, 0.0], [0.995, 0.1, 0.0]];
        let nbr_offsets = vec![0, 1, 2];
        let nbrs = vec![1, 0];
        let plate_id = vec![PlateId(0), PlateId(1)];
        let plate_states = opposing_z_rotation_states(0.1, -0.1);
        let vertex_states = vec![oceanic_state(100.0, 3_200.0), continental_state()];
        let mut boundary_state = BoundaryDynamicsState::default();

        reclassify_boundaries(
            ReclassifyBoundariesInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &plate_id,
                plate_states: &plate_states,
                vertex_states: &vertex_states,
                params: &GeologyParams::default(),
            },
            &mut boundary_state,
        );

        assert_eq!(boundary_state.edge_types, vec![BoundaryType::Subduction]);
        assert_eq!(boundary_state.edge_convergent_plate, vec![Some(PlateId(0))]);
        assert_eq!(
            boundary_state.subducting_plate,
            vec![Some(PlateId(0)), Some(PlateId(0))]
        );
    }

    #[test]
    fn oceanic_buoyancy_proxy_uses_age_and_model_density_gain() {
        let params = GeologyParams::default();
        let young = oceanic_state(0.0, params.oceanic_base_density);
        let old = oceanic_state(
            params.age_ref,
            params.oceanic_base_density + params.age_density_gain,
        );

        assert_eq!(oceanic_negative_buoyancy_proxy(young, &params), 0.0);
        assert!(oceanic_negative_buoyancy_proxy(old, &params) > 0.5);
    }
}
