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

    for i in 0..cell_count {
        let pos_i = positions[i];
        let vel_i =
            plate_velocity_from_state(plate_states.get(plate_id[i].as_usize()), plate_id[i], pos_i);
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
                plate_states.get(plate_id[n].as_usize()),
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
