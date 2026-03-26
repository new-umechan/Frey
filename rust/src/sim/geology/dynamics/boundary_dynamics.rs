use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, CrustType, PlateId, PlateKinematicsState, VertexCrustState,
};
use crate::GeologyParams;

use crate::sim::exec::math::{cross3, dot, length3, seeded_axis};
use crate::sim::exec::{lerp, CONVERGENT_THRESHOLD, DIVERGENT_THRESHOLD, TRANSFORM_THRESHOLD};

pub(super) fn reclassify_boundaries(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_states: &[PlateKinematicsState],
    vertex_states: &[VertexCrustState],
    boundary_state: &mut BoundaryDynamicsState,
    params: &GeologyParams,
) {
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

    if boundary_state.edge_pairs != edge_pairs {
        boundary_state.edge_pairs = edge_pairs.clone();
        boundary_state.edge_internal = vec![Default::default(); edge_pairs.len()];
    } else if boundary_state.edge_internal.len() != edge_pairs.len() {
        boundary_state.edge_internal = vec![Default::default(); edge_pairs.len()];
    }

    let mut convergence_norm_edge = vec![0.0_f32; edge_pairs.len()];
    let mut subduction_age_edge = vec![0.0_f32; edge_pairs.len()];
    let mut subduction_density_edge = vec![0.0_f32; edge_pairs.len()];
    let mut edge_types = vec![BoundaryType::PassiveMargin; edge_pairs.len()];
    let mut edge_scores = vec![0.0_f32; edge_pairs.len()];

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

        let (bt, score) = classify_boundary_pair(
            rel.rel_n,
            rel.rel_t,
            vertex_states[i],
            vertex_states[j],
            params,
        );
        edge_types[eid] = bt;
        edge_scores[eid] = score.clamp(0.0, 1.0);

        if bt == BoundaryType::Subduction {
            convergence_norm_edge[eid] = (rel.rel_n * 8.0).clamp(0.0, 1.0);
            if let Some(oceanic) = densest_oceanic(vertex_states[i], vertex_states[j]) {
                subduction_age_edge[eid] = oceanic.age.max(0.0);
                subduction_density_edge[eid] = oceanic.density.max(0.0);
            }
        }
    }

    boundary_state
        .dominant_type
        .fill(BoundaryType::PassiveMargin);
    boundary_state.activity.fill(0.0);
    for (eid, pair) in edge_pairs.iter().enumerate() {
        let bt = edge_types[eid];
        let score = edge_scores[eid];
        for cell in [pair[0] as usize, pair[1] as usize] {
            if score > boundary_state.activity[cell] {
                boundary_state.activity[cell] = score;
                boundary_state.dominant_type[cell] = bt;
            }
        }
    }

    let memory_rate = params.convergence_memory_rate.clamp(0.0, 1.0);
    for (eid, edge_internal) in boundary_state.edge_internal.iter_mut().enumerate() {
        let prev = edge_internal.convergence_memory;
        let next = prev + (convergence_norm_edge[eid] - prev) * memory_rate;
        edge_internal.convergence_memory = next.clamp(0.0, 1.0);
    }

    let mut incident_edges = vec![Vec::<usize>::new(); cell_count];
    for (eid, pair) in edge_pairs.iter().enumerate() {
        incident_edges[pair[0] as usize].push(eid);
        incident_edges[pair[1] as usize].push(eid);
    }

    let smooth_mix = params.convergence_memory_spatial_smooth.clamp(0.0, 1.0);
    let old_memory = boundary_state
        .edge_internal
        .iter()
        .map(|edge| edge.convergence_memory)
        .collect::<Vec<_>>();
    for (eid, pair) in edge_pairs.iter().enumerate() {
        let mut acc = 0.0_f32;
        let mut cnt = 0_u32;
        for &cid in &[pair[0] as usize, pair[1] as usize] {
            for &other in &incident_edges[cid] {
                if other == eid {
                    continue;
                }
                acc += old_memory[other];
                cnt = cnt.saturating_add(1);
            }
        }
        if cnt == 0 {
            continue;
        }
        boundary_state.edge_internal[eid].convergence_memory =
            lerp(old_memory[eid], acc / cnt as f32, smooth_mix).clamp(0.0, 1.0);
    }

    boundary_state.rollback_fraction.fill(0.0);
    boundary_state.backarc_tension.fill(0.0);
    boundary_state.slab_convergence_component.fill(0.0);
    boundary_state.slab_rollback_component.fill(0.0);

    let mantle_density = params.mantle_density.max(1e-3);
    let dip_density_scale = params.dip_density_scale.max(1e-4);
    let age_ref = params.age_ref.max(1e-4);
    let mut cell_rollback_count = vec![0_u32; cell_count];

    for (eid, pair) in edge_pairs.iter().enumerate() {
        if edge_types[eid] != BoundaryType::Subduction {
            continue;
        }

        let age_norm = (subduction_age_edge[eid] / age_ref).clamp(0.0, 1.0);
        let density_ocean = subduction_density_edge[eid];
        let dip_factor = ((density_ocean - mantle_density) / dip_density_scale).clamp(0.0, 1.0);
        let memory = boundary_state.edge_internal[eid].convergence_memory;
        let slab_depth_est = params.subduction_depth_gain.max(0.0) * age_norm * memory;
        let suppression = (1.0 - convergence_norm_edge[eid] * params.rollback_suppression.max(0.0))
            .clamp(0.0, 1.0);
        let rollback =
            (params.rollback_gain.max(0.0) * age_norm * dip_factor * slab_depth_est * suppression)
                .clamp(0.0, params.rollback_fraction_max.max(0.0));

        let slab_pull_mag = edge_scores[eid].max(0.0)
            * (density_ocean - mantle_density).max(0.0)
            * (1.0 + slab_depth_est);
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

    for i in 0..cell_count {
        let denom = cell_rollback_count[i].max(1) as f32;
        boundary_state.rollback_fraction[i] = (boundary_state.rollback_fraction[i] / denom)
            .clamp(0.0, params.rollback_fraction_max.max(0.0));
        boundary_state.backarc_tension[i] /= denom;
        boundary_state.slab_convergence_component[i] /= denom;
        boundary_state.slab_rollback_component[i] /= denom;
    }
}

#[derive(Clone, Copy)]
struct RelativeKinematics {
    rel_n: f32,
    rel_t: f32,
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
    let rel_n = dot(rel_v, edge_dir);
    let rel_mag = length3(rel_v);
    let rel_t = (rel_mag * rel_mag - rel_n * rel_n).max(0.0).sqrt();
    RelativeKinematics { rel_n, rel_t }
}

pub(super) fn plate_velocity_for_cell(
    plate_states: &[PlateKinematicsState],
    plate_id: PlateId,
    pos: [f32; 3],
) -> [f32; 3] {
    plate_velocity_from_state(plate_states.get(plate_id.as_usize()), plate_id, pos)
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
    let mut plate_count = vec![0_u32; plate_states.len()];

    for i in 0..plate_id.len() {
        let pid = plate_id[i].as_usize();
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
        let damping = match dominant_plate_boundary_type(
            PlateId(pid as u32),
            plate_id,
            &boundary_state.dominant_type,
        ) {
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

fn classify_boundary_pair(
    rel_n: f32,
    rel_t: f32,
    a: VertexCrustState,
    b: VertexCrustState,
    params: &GeologyParams,
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
        return (bt, (rel_n * 8.0 + rel_t).clamp(0.0, 1.0));
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

fn dominant_plate_boundary_type(
    plate: PlateId,
    plate_id: &[PlateId],
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

fn plate_velocity_from_state(
    state: Option<&PlateKinematicsState>,
    plate_id: PlateId,
    pos: [f32; 3],
) -> [f32; 3] {
    let seed = plate_id.as_u32();
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
