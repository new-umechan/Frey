use crate::sim::geology_types::{CrustType, PlateId, StressTensor};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, GeologyStepMetrics, VertexCrustState,
};
use crate::GeologyParams;

use crate::sim::exec::{DEFAULT_DIFFUSION_WEIGHT, MAX_HEIGHT_DELTA_PER_STEP};

const SURFACE_HEIGHT_MIN: f32 = -1.0;
const SURFACE_HEIGHT_MAX: f32 = 1.0;

pub(super) struct SurfaceUpdateInput<'a> {
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub heights: &'a [f32],
    pub plate_id: &'a [PlateId],
    pub boundary_state: &'a BoundaryDynamicsState,
    pub mantle_heat: &'a [f32],
    pub plume_force: &'a [f32],
    pub params: &'a GeologyParams,
}

pub(super) struct SurfaceUpdateOutput<'a> {
    pub next_vertex_states: &'a mut [VertexCrustState],
    pub next_height: &'a mut [f32],
    pub next_volcanism: &'a mut [f32],
    pub next_vertex_buoyancy: &'a mut [f32],
}

pub(super) fn apply_stress_and_surface_update(
    input: SurfaceUpdateInput<'_>,
    output: &mut SurfaceUpdateOutput<'_>,
) -> GeologyStepMetrics {
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let heights = input.heights;
    let plate_id = input.plate_id;
    let boundary_state = input.boundary_state;
    let mantle_heat = input.mantle_heat;
    let plume_force = input.plume_force;
    let params = input.params;

    let next_vertex_states = &mut *output.next_vertex_states;
    let next_height = &mut *output.next_height;
    let next_volcanism = &mut *output.next_volcanism;
    let next_vertex_buoyancy = &mut *output.next_vertex_buoyancy;

    let cell_count = heights.len();
    let mut boundary_sum = 0.0_f32;
    let mut isostatic_equilibrium = vec![0.0; cell_count];

    for i in 0..cell_count {
        let boundary_type = boundary_state
            .dominant_type
            .get(i)
            .copied()
            .unwrap_or(BoundaryType::PassiveMargin);
        let boundary_activity =
            finite_or(boundary_state.activity.get(i).copied().unwrap_or(0.0), 0.0).clamp(0.0, 1.0);

        let mut tensor = boundary_tensor(boundary_type, boundary_activity);

        let plume = finite_or(plume_force.get(i).copied().unwrap_or(0.0), 0.0).max(0.0);
        tensor.xx += plume * 0.7;
        tensor.yy += plume * 0.7;
        let slab_conv = boundary_state
            .slab_convergence_component
            .get(i)
            .copied()
            .map(|v| finite_or(v, 0.0))
            .unwrap_or(0.0);
        let slab_roll = boundary_state
            .slab_rollback_component
            .get(i)
            .copied()
            .map(|v| finite_or(v, 0.0))
            .unwrap_or(0.0);
        tensor.xx -= slab_conv * 0.08;
        tensor.yy -= slab_conv * 0.08;
        tensor.xx += slab_roll * 0.05;
        tensor.yy += slab_roll * 0.03;
        let backarc_tension = boundary_state
            .backarc_tension
            .get(i)
            .copied()
            .map(|v| finite_or(v, 0.0))
            .unwrap_or(0.0);
        tensor.xx += backarc_tension;
        tensor.yy += backarc_tension;

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let plate_i = plate_id[i];
        let height_i = heights[i];
        let neighbors = &nbrs[start..end];
        let mut nbr_sum = 0.0;
        let mut nbr_count = 0usize;
        let mut nbr_stress_xx = 0.0;
        let mut nbr_stress_yy = 0.0;
        let mut nbr_stress_xy = 0.0;

        for &n_u32 in neighbors {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            nbr_sum += heights[n];
            nbr_count += 1;
            let n_tensor = next_vertex_states[n].stress_tensor;
            let atten = 0.18 - 0.06 * f32::from((plate_id[n] == plate_i) as u8);
            nbr_stress_xx += finite_or(n_tensor.xx, 0.0) * atten;
            nbr_stress_yy += finite_or(n_tensor.yy, 0.0) * atten;
            nbr_stress_xy += finite_or(n_tensor.xy, 0.0) * atten;
        }

        tensor.xx += nbr_stress_xx;
        tensor.yy += nbr_stress_yy;
        tensor.xy += nbr_stress_xy;

        let prev = next_vertex_states[i];
        let mantle_heat_i =
            finite_or(mantle_heat.get(i).copied().unwrap_or(0.5), 0.5).clamp(0.0, 1.0);
        let rigidity =
            (prev.rigidity + 0.15 * prev.thickness - 0.20 * mantle_heat_i).clamp(0.20, 1.40);
        let inv_rigidity = 1.0 / rigidity.max(1e-3);

        tensor.xx *= inv_rigidity;
        tensor.yy *= inv_rigidity;
        tensor.xy *= inv_rigidity;

        let stress_scalar = finite_or((tensor.xx + tensor.yy) * 0.5 + tensor.xy.abs() * 0.30, 0.0);
        let relax = params.stress_relaxation_rate.clamp(0.0, 1.0);
        let stress = finite_or(prev.stress * (1.0 - relax) + stress_scalar * relax, 0.0);

        let mut state = prev;
        state.temperature = mantle_heat_i;
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
        let rollback_fraction = finite_or(
            boundary_state
                .rollback_fraction
                .get(i)
                .copied()
                .unwrap_or(0.0),
            0.0,
        )
        .max(0.0);
        let convergence_memory = finite_or(
            boundary_state
                .edge_internal
                .get(i)
                .map(|s| s.convergence_memory)
                .unwrap_or(0.0),
            0.0,
        )
        .clamp(0.0, 1.0);

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
        let volcanism = finite_or(
            state.arc_volcanism
                + state.ridge_volcanism
                + state.hotspot_volcanism
                + state.backarc_volcanism,
            0.0,
        )
        .max(0.0);

        let tectonic_uplift = params.tectonic_uplift_gain.max(0.0) * compressive;
        let volcanic_uplift = volcanism * params.volcanic_uplift_gain.max(0.0);
        let uplift = tectonic_uplift + volcanic_uplift;
        let tectonic_subsidence = params.tectonic_subsidence_gain.max(0.0) * tensile;
        let thermal_subsidence = if state.crust_type == CrustType::Oceanic {
            let age_norm = (state.age / params.age_ref.max(1e-4)).clamp(0.0, 1.0);
            params.thermal_subsidence_gain.max(0.0) * age_norm.sqrt()
        } else {
            0.0
        };
        let total_subsidence = tectonic_subsidence + thermal_subsidence;

        let diffusive = if nbr_count == 0 {
            0.0
        } else {
            (nbr_sum / nbr_count as f32 - height_i) * DEFAULT_DIFFUSION_WEIGHT
        };
        let raw_delta = finite_or(uplift - total_subsidence + diffusive, 0.0);
        let delta = raw_delta.clamp(-MAX_HEIGHT_DELTA_PER_STEP, MAX_HEIGHT_DELTA_PER_STEP);
        let mut next_h =
            finite_or(heights[i] + delta, heights[i]).clamp(SURFACE_HEIGHT_MIN, SURFACE_HEIGHT_MAX);

        if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift) && next_h < -0.02 {
            state.crust_type = CrustType::Oceanic;
            state.thickness = (state.thickness - 0.010).clamp(0.20, 1.20);
            state.age = 0.0;
        } else if boundary_type == BoundaryType::Collision && next_h > 0.20 {
            state.crust_type = CrustType::Continental;
            state.thickness = (state.thickness + 0.008).clamp(0.20, 1.20);
            state.age = params.age_ref.max(1e-4);
        }

        state.thickness = (state.thickness + uplift * 0.5 - tectonic_subsidence * 0.4
            + volcanism * params.volcanic_thickening_gain.max(0.0)
            + plume * 0.1)
            .clamp(0.18, 1.25);
        state.age = finite_or(state.age, 0.0);
        state.density = finite_or(state.density, params.continental_crust_density.max(1e-3));
        state.thickness = finite_or(state.thickness, 0.65).clamp(0.18, 1.25);
        state.temperature = finite_or(state.temperature, 0.5).clamp(0.0, 1.0);
        state.stress = finite_or(state.stress, 0.0);
        state.stress_tensor.xx = finite_or(state.stress_tensor.xx, 0.0);
        state.stress_tensor.yy = finite_or(state.stress_tensor.yy, 0.0);
        state.stress_tensor.xy = finite_or(state.stress_tensor.xy, 0.0);
        let density_ratio = (state.density / params.mantle_density.max(1e-3)).clamp(0.1, 1.4);
        let h_eq = finite_or(state.thickness * (1.0 - density_ratio), next_h);
        next_h = finite_or(
            next_h + (h_eq - next_h) * params.isostatic_adjustment_rate.max(0.0),
            next_h,
        )
        .clamp(SURFACE_HEIGHT_MIN, SURFACE_HEIGHT_MAX);
        state.rigidity = rigidity;

        boundary_sum += boundary_state.activity.get(i).copied().unwrap_or(0.0);

        next_vertex_states[i] = state;
        next_height[i] = next_h;
        next_volcanism[i] = volcanism;
        isostatic_equilibrium[i] = h_eq;
    }

    enforce_zero_mean_endogenous_height_change(heights, next_height);

    let mut terrain_delta_sum = 0.0_f32;
    let mut uplift_sum = 0.0_f32;
    let mut subsidence_sum = 0.0_f32;
    for i in 0..cell_count {
        let delta = next_height[i] - heights[i];
        terrain_delta_sum += delta.abs();
        if delta > 0.0 {
            uplift_sum += delta;
        } else {
            subsidence_sum += -delta;
        }
        next_vertex_buoyancy[i] = finite_or(isostatic_equilibrium[i] - next_height[i], 0.0);
    }

    let denom = cell_count.max(1) as f32;
    GeologyStepMetrics {
        geology_activity: (terrain_delta_sum / denom).clamp(0.0, 1.0),
        boundary_activity: (boundary_sum / denom).clamp(0.0, 1.0),
        uplift_rate: uplift_sum / denom,
        subsidence_rate: subsidence_sum / denom,
    }
}

fn enforce_zero_mean_endogenous_height_change(prev_height: &[f32], next_height: &mut [f32]) {
    if prev_height.len() != next_height.len() || next_height.is_empty() {
        return;
    }

    let mut residual = next_height
        .iter()
        .zip(prev_height.iter())
        .map(|(next, prev)| next - prev)
        .sum::<f32>();
    if !residual.is_finite() || residual.abs() <= 1e-6 {
        return;
    }

    for _ in 0..8 {
        let lowering = residual > 0.0;
        let active = next_height
            .iter()
            .enumerate()
            .filter(|(_, value)| {
                if lowering {
                    **value > SURFACE_HEIGHT_MIN + 1e-6
                } else {
                    **value < SURFACE_HEIGHT_MAX - 1e-6
                }
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if active.is_empty() {
            break;
        }

        let correction = residual / active.len() as f32;
        let mut applied = 0.0f32;
        for index in active {
            let room = if lowering {
                next_height[index] - SURFACE_HEIGHT_MIN
            } else {
                SURFACE_HEIGHT_MAX - next_height[index]
            };
            let delta = correction.abs().min(room.max(0.0));
            if delta <= 0.0 {
                continue;
            }
            if lowering {
                next_height[index] -= delta;
                applied += delta;
            } else {
                next_height[index] += delta;
                applied -= delta;
            }
        }

        residual -= applied;
        if residual.abs() <= 1e-6 {
            break;
        }
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

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::enforce_zero_mean_endogenous_height_change;

    #[test]
    fn endogenous_height_change_preserves_global_mean() {
        let prev = vec![0.10, -0.20, 0.05, -0.05];
        let mut next = vec![0.18, -0.10, 0.12, 0.02];

        enforce_zero_mean_endogenous_height_change(&prev, &mut next);

        let mean_delta = next
            .iter()
            .zip(prev.iter())
            .map(|(next, prev)| next - prev)
            .sum::<f32>()
            / prev.len() as f32;
        assert!(mean_delta.abs() <= 1e-6);
    }

    #[test]
    fn endogenous_height_change_respects_clamps() {
        let prev = vec![0.95, 0.90, -0.95, -0.90];
        let mut next = vec![1.0, 1.0, -0.70, -0.60];

        enforce_zero_mean_endogenous_height_change(&prev, &mut next);

        assert!(next.iter().all(|value| (-1.0..=1.0).contains(value)));
        let mean_delta = next
            .iter()
            .zip(prev.iter())
            .map(|(next, prev)| next - prev)
            .sum::<f32>()
            / prev.len() as f32;
        assert!(mean_delta.abs() <= 1e-6);
    }
}
