use std::cmp::Ordering;

use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, CrustType, GeologyStepMetrics, PlateId, StressTensor,
    VertexCrustState,
};
use crate::GeologyParams;

use crate::sim::exec::{DEFAULT_DIFFUSION_WEIGHT, MAX_HEIGHT_DELTA_PER_STEP};

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_stress_and_surface_update(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    heights: &[f32],
    plate_id: &[PlateId],
    boundary_state: &BoundaryDynamicsState,
    mantle_heat: &[f32],
    plume_force: &[f32],
    next_vertex_states: &mut [VertexCrustState],
    next_height: &mut [f32],
    next_volcanism: &mut [f32],
    next_vertex_buoyancy: &mut [f32],
    params: &GeologyParams,
) -> GeologyStepMetrics {
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
        let slab_conv = boundary_state
            .slab_convergence_component
            .get(i)
            .copied()
            .unwrap_or(0.0);
        let slab_roll = boundary_state
            .slab_rollback_component
            .get(i)
            .copied()
            .unwrap_or(0.0);
        tensor.xx -= slab_conv * 0.08;
        tensor.yy -= slab_conv * 0.08;
        tensor.xx += slab_roll * 0.05;
        tensor.yy += slab_roll * 0.03;
        let backarc_tension = boundary_state
            .backarc_tension
            .get(i)
            .copied()
            .unwrap_or(0.0);
        tensor.xx += backarc_tension;
        tensor.yy += backarc_tension;

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
            let age_inc = params.age_advection_gain.max(0.0) * (0.6 + 0.4 * (1.0 - plume));
            state.age = (state.age + age_inc).clamp(0.0, params.age_ref.max(1e-4));
            let age_norm = (state.age / params.age_ref.max(1e-4)).clamp(0.0, 1.0);
            state.density =
                params.oceanic_base_density + params.age_density_gain.max(0.0) * age_norm.sqrt();
        } else {
            state.age = params.age_ref.max(1e-4);
            state.density = params.continental_crust_density.max(1e-3);
        }

        let compressive = (-stress).max(0.0);
        let tensile = stress.max(0.0);
        let boundary_type = boundary_state
            .dominant_type
            .get(i)
            .copied()
            .unwrap_or(BoundaryType::PassiveMargin);
        let boundary_activity = boundary_state.activity.get(i).copied().unwrap_or(0.0);
        let rollback_fraction = boundary_state
            .rollback_fraction
            .get(i)
            .copied()
            .unwrap_or(0.0);
        let convergence_memory = boundary_state
            .edge_internal
            .get(i)
            .map(|s| s.convergence_memory)
            .unwrap_or(0.0);

        state.arc_volcanism = if boundary_type == BoundaryType::Subduction {
            boundary_activity
                * (0.35 + 0.65 * convergence_memory)
                * params.arc_volcanism_gain.max(0.0)
        } else {
            0.0
        };
        state.ridge_volcanism = if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift)
        {
            boundary_activity * params.ridge_volcanism_gain.max(0.0)
        } else {
            0.0
        };
        state.hotspot_volcanism = plume * params.hotspot_volcanism_gain.max(0.0);
        state.backarc_volcanism = if rollback_fraction > params.rollback_threshold.max(0.0) {
            rollback_fraction * params.backarc_volcanism_gain.max(0.0)
        } else {
            0.0
        };
        let volcanism = state.arc_volcanism
            + state.ridge_volcanism
            + state.hotspot_volcanism
            + state.backarc_volcanism;

        let uplift = params.uplift_rate_gain.max(0.0) * compressive
            + volcanism * params.volcanic_uplift_gain.max(0.0);
        let subsidence = params.subsidence_rate_gain.max(0.0)
            * (tensile
                + if state.crust_type == CrustType::Oceanic {
                    (state.age / params.age_ref.max(1e-4)).clamp(0.0, 1.0) * 0.6
                } else {
                    0.0
                });

        let diffusive = if nbr_count == 0 {
            0.0
        } else {
            (nbr_sum / nbr_count as f32 - heights[i]) * DEFAULT_DIFFUSION_WEIGHT
        };
        let raw_delta = uplift - subsidence + diffusive;
        let delta = raw_delta.clamp(-MAX_HEIGHT_DELTA_PER_STEP, MAX_HEIGHT_DELTA_PER_STEP);
        let mut next_h = (heights[i] + delta).clamp(-1.0, 1.0);

        if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift) && next_h < -0.02 {
            state.crust_type = CrustType::Oceanic;
            state.thickness = (state.thickness - 0.010).clamp(0.20, 1.20);
            state.age = 0.0;
        } else if boundary_type == BoundaryType::Collision && next_h > 0.20 {
            state.crust_type = CrustType::Continental;
            state.thickness = (state.thickness + 0.008).clamp(0.20, 1.20);
            state.age = params.age_ref.max(1e-4);
        }

        state.thickness = (state.thickness + uplift * 0.5 - subsidence * 0.4
            + volcanism * params.volcanic_thickening_gain.max(0.0)
            + plume * 0.1)
            .clamp(0.18, 1.25);
        let density_ratio = (state.density / params.mantle_density.max(1e-3)).clamp(0.1, 1.4);
        let h_eq = state.thickness * (1.0 - density_ratio);
        next_h = (next_h + (h_eq - next_h) * params.isostasy_rate.max(0.0)).clamp(-1.0, 1.0);
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
        next_volcanism[i] = volcanism;
        next_vertex_buoyancy[i] = h_eq - next_h;
    }

    let denom = cell_count.max(1) as f32;
    GeologyStepMetrics {
        geology_activity: (terrain_delta_sum / denom).clamp(0.0, 1.0),
        boundary_activity: (boundary_sum / denom).clamp(0.0, 1.0),
        uplift_rate: uplift_sum / denom,
        subsidence_rate: subsidence_sum / denom,
    }
}

pub(super) fn preserve_target_sea_ratio(height: &mut [f32], target_sea_ratio: f32, strength: f32) {
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
