use crate::sim::geology_types::{CrustType, PlateId, StressTensor};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, GeologyStepMetrics, VertexCrustState,
};
use crate::GeologyParams;

use crate::sim::exec::{DEFAULT_DIFFUSION_WEIGHT, MAX_HEIGHT_DELTA_PER_STEP};

const SURFACE_HEIGHT_MIN: f32 = -1.0;
const SURFACE_HEIGHT_MAX: f32 = 1.0;
const SMOOTHING_DOMINANCE_TARGET: f32 = 1.5;
const RELIEF_RETENTION_FRACTION_PER_STEP: f32 = 0.08;
const THICKNESS_RECOVERY_RATE: f32 = 0.08;
const RIGIDITY_RECOVERY_RATE: f32 = 0.18;

#[derive(Clone, Copy, Default)]
struct ZeroMeanCorrectionStats {
    adjusted_cells_ratio: f32,
    mean_abs_correction: f32,
    std_delta: f32,
}

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
    let mut smoothing_limited_cells = 0u32;
    let mut smoothing_factor_sum = 0.0f32;
    let mut compressive_sum = 0.0f32;
    let mut tensile_sum = 0.0f32;
    let mut diffusive_raw_sum = 0.0f32;
    let mut isostatic_raw_sum = 0.0f32;

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
        let (base_rigidity_min, base_rigidity_max) = crust_rigidity_bounds(prev.crust_type);
        let rigidity = (prev.rigidity + 0.15 * prev.thickness - 0.20 * mantle_heat_i)
            .clamp(base_rigidity_min, base_rigidity_max);
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
        compressive_sum += compressive;
        tensile_sum += tensile;
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

        let mean_neighbor_height = if nbr_count == 0 {
            height_i
        } else {
            nbr_sum / nbr_count as f32
        };
        let diffusive_raw = if nbr_count == 0 {
            0.0
        } else {
            (mean_neighbor_height - height_i) * DEFAULT_DIFFUSION_WEIGHT
        };
        let density_ratio = (state.density / params.mantle_density.max(1e-3)).clamp(0.1, 1.4);
        let h_eq = finite_or(state.thickness * (1.0 - density_ratio), height_i);
        let isostatic_adjustment_rate = params.isostatic_adjustment_rate.max(0.0);
        let isostatic_raw = (h_eq - height_i) * isostatic_adjustment_rate;
        diffusive_raw_sum += diffusive_raw.abs();
        isostatic_raw_sum += isostatic_raw.abs();
        let endogenous_forcing = uplift.abs()
            + tectonic_subsidence.abs()
            + volcanism
            + plume * params.plume_gain.max(0.0);
        let local_relief = (mean_neighbor_height - height_i).abs() + (h_eq - height_i).abs();
        let smoothing_strength = diffusive_raw.abs() + isostatic_raw.abs();
        let smoothing_factor =
            smoothing_limiter(endogenous_forcing, smoothing_strength, local_relief);
        if smoothing_factor < 1.0 - 1e-6 {
            smoothing_limited_cells = smoothing_limited_cells.saturating_add(1);
        }
        smoothing_factor_sum += smoothing_factor;
        let diffusive = diffusive_raw * smoothing_factor;

        let raw_delta = finite_or(uplift - total_subsidence + diffusive, 0.0);
        let delta = raw_delta.clamp(-MAX_HEIGHT_DELTA_PER_STEP, MAX_HEIGHT_DELTA_PER_STEP);
        let mut next_h =
            finite_or(heights[i] + delta, heights[i]).clamp(SURFACE_HEIGHT_MIN, SURFACE_HEIGHT_MAX);

        if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift) && next_h < -0.02 {
            state.crust_type = CrustType::Oceanic;
            let (thickness_min, thickness_max) = crust_thickness_bounds(state.crust_type);
            state.thickness = (state.thickness - 0.010).clamp(thickness_min, thickness_max);
            state.age = 0.0;
            state.density = params.oceanic_base_density.max(1e-3);
        } else if boundary_type == BoundaryType::Collision && next_h > 0.20 {
            state.crust_type = CrustType::Continental;
            let (thickness_min, thickness_max) = crust_thickness_bounds(state.crust_type);
            state.thickness = (state.thickness + 0.008).clamp(thickness_min, thickness_max);
            state.age = params.age_ref.max(1e-4);
            state.density = params.continental_crust_density.max(1e-3);
        }

        let (thickness_min, thickness_max) = crust_thickness_bounds(state.crust_type);
        state.thickness = (state.thickness + uplift * 0.5 - tectonic_subsidence * 0.4
            + volcanism * params.volcanic_thickening_gain.max(0.0)
            + plume * 0.1)
            .clamp(thickness_min, thickness_max);
        let thickness_target =
            equilibrium_thickness(state.crust_type, next_h, boundary_type, plume);
        let thickness_recovery = THICKNESS_RECOVERY_RATE * (0.35 + 0.65 * boundary_activity);
        state.thickness = finite_or(
            state.thickness + (thickness_target - state.thickness) * thickness_recovery,
            state.thickness,
        )
        .clamp(thickness_min, thickness_max);
        state.age = finite_or(state.age, 0.0);
        state.density = finite_or(state.density, params.continental_crust_density.max(1e-3));
        state.thickness = finite_or(state.thickness, 0.65).clamp(thickness_min, thickness_max);
        state.temperature = finite_or(state.temperature, 0.5).clamp(0.0, 1.0);
        state.stress = finite_or(state.stress, 0.0);
        state.stress_tensor.xx = finite_or(state.stress_tensor.xx, 0.0);
        state.stress_tensor.yy = finite_or(state.stress_tensor.yy, 0.0);
        state.stress_tensor.xy = finite_or(state.stress_tensor.xy, 0.0);
        let density_ratio = (state.density / params.mantle_density.max(1e-3)).clamp(0.1, 1.4);
        let h_eq = finite_or(state.thickness * (1.0 - density_ratio), next_h);
        let isostatic_adjustment_rate = isostatic_adjustment_rate * smoothing_factor;
        next_h = finite_or(next_h + (h_eq - next_h) * isostatic_adjustment_rate, next_h)
            .clamp(SURFACE_HEIGHT_MIN, SURFACE_HEIGHT_MAX);
        let (rigidity_min, rigidity_max) = crust_rigidity_bounds(state.crust_type);
        let rigidity_target = equilibrium_rigidity(
            state.crust_type,
            state.thickness,
            mantle_heat_i,
            boundary_activity,
        );
        state.rigidity = finite_or(
            rigidity + (rigidity_target - rigidity) * RIGIDITY_RECOVERY_RATE,
            rigidity,
        )
        .clamp(rigidity_min, rigidity_max);

        boundary_sum += boundary_state.activity.get(i).copied().unwrap_or(0.0);

        next_vertex_states[i] = state;
        next_height[i] = next_h;
        next_volcanism[i] = volcanism;
        isostatic_equilibrium[i] = h_eq;
    }

    let zero_mean_stats = enforce_zero_mean_endogenous_height_change(heights, next_height);

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
        uplift_rate: finite_or(uplift_sum / denom, 0.0),
        subsidence_rate: finite_or(subsidence_sum / denom, 0.0),
        smoothing_limited_cells_ratio: finite_or(smoothing_limited_cells as f32 / denom, 0.0),
        mean_smoothing_factor: finite_or(smoothing_factor_sum / denom, 1.0),
        zero_mean_adjusted_cells_ratio: finite_or(zero_mean_stats.adjusted_cells_ratio, 0.0),
        zero_mean_mean_abs_correction: finite_or(zero_mean_stats.mean_abs_correction, 0.0),
        zero_mean_std_delta: finite_or(zero_mean_stats.std_delta, 0.0),
        mean_compressive: finite_or(compressive_sum / denom, 0.0),
        mean_tensile: finite_or(tensile_sum / denom, 0.0),
        mean_abs_diffusive_raw: finite_or(diffusive_raw_sum / denom, 0.0),
        mean_abs_isostatic_raw: finite_or(isostatic_raw_sum / denom, 0.0),
    }
}

fn smoothing_limiter(endogenous_forcing: f32, smoothing_strength: f32, local_relief: f32) -> f32 {
    if !endogenous_forcing.is_finite() || !smoothing_strength.is_finite() {
        return 1.0;
    }
    if smoothing_strength <= 1e-6 {
        return 1.0;
    }

    let forcing_budget = endogenous_forcing.max(1e-4) * SMOOTHING_DOMINANCE_TARGET;
    let relief_budget = local_relief.max(1e-4) * RELIEF_RETENTION_FRACTION_PER_STEP;
    let allowable_smoothing = forcing_budget.min(relief_budget).max(1e-6);
    if smoothing_strength <= allowable_smoothing {
        1.0
    } else {
        (allowable_smoothing / smoothing_strength).clamp(0.0, 1.0)
    }
}

fn equilibrium_thickness(
    crust_type: CrustType,
    height: f32,
    boundary_type: BoundaryType,
    plume: f32,
) -> f32 {
    let boundary_bonus = match boundary_type {
        BoundaryType::Subduction | BoundaryType::Collision => 0.06,
        BoundaryType::Ridge | BoundaryType::Rift => -0.04,
        BoundaryType::Transform | BoundaryType::PassiveMargin => 0.0,
    };
    let plume_bonus = plume.clamp(0.0, 1.0) * 0.04;
    let base = match crust_type {
        CrustType::Oceanic => 0.35 + (-height).clamp(0.0, 0.6) * 0.25,
        CrustType::Continental => 0.65 + height.clamp(0.0, 0.6) * 0.20,
    };
    (base + boundary_bonus + plume_bonus).clamp(0.22, 1.05)
}

fn equilibrium_rigidity(
    crust_type: CrustType,
    thickness: f32,
    mantle_heat: f32,
    boundary_activity: f32,
) -> f32 {
    let crust_base = match crust_type {
        CrustType::Oceanic => 0.46,
        CrustType::Continental => 0.72,
    };
    let thickness_term = (thickness - 0.35).max(0.0) * 0.45;
    let thermal_term = mantle_heat.clamp(0.0, 1.0) * 0.18;
    let activity_term = boundary_activity.clamp(0.0, 1.0) * 0.04;
    (crust_base + thickness_term - thermal_term + activity_term).clamp(0.24, 1.10)
}

fn crust_thickness_bounds(crust_type: CrustType) -> (f32, f32) {
    match crust_type {
        CrustType::Oceanic => (0.24, 1.10),
        CrustType::Continental => (0.48, 1.25),
    }
}

fn crust_rigidity_bounds(crust_type: CrustType) -> (f32, f32) {
    match crust_type {
        CrustType::Oceanic => (0.24, 1.10),
        CrustType::Continental => (0.38, 1.40),
    }
}

fn enforce_zero_mean_endogenous_height_change(
    prev_height: &[f32],
    next_height: &mut [f32],
) -> ZeroMeanCorrectionStats {
    if prev_height.len() != next_height.len() || next_height.is_empty() {
        return ZeroMeanCorrectionStats::default();
    }

    let std_before = height_std_dev(next_height);
    let mut adjusted = vec![false; next_height.len()];
    let mut total_abs_correction = 0.0f32;
    let mut residual = next_height
        .iter()
        .zip(prev_height.iter())
        .map(|(next, prev)| next - prev)
        .sum::<f32>();
    if !residual.is_finite() || residual.abs() <= 1e-6 {
        return ZeroMeanCorrectionStats::default();
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
            adjusted[index] = true;
            total_abs_correction += delta;
        }

        residual -= applied;
        if residual.abs() <= 1e-6 {
            break;
        }
    }

    let adjusted_cells = adjusted.into_iter().filter(|value| *value).count() as f32;
    let denom = next_height.len().max(1) as f32;
    let std_after = height_std_dev(next_height);
    ZeroMeanCorrectionStats {
        adjusted_cells_ratio: adjusted_cells / denom,
        mean_abs_correction: total_abs_correction / denom,
        std_delta: std_after - std_before,
    }
}

fn height_std_dev(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let len = values.len() as f32;
    let mean = values.iter().copied().sum::<f32>() / len;
    let variance = values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f32>()
        / len;
    variance.max(0.0).sqrt()
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
    use super::{enforce_zero_mean_endogenous_height_change, smoothing_limiter};

    #[test]
    fn endogenous_height_change_preserves_global_mean() {
        let prev = vec![0.10, -0.20, 0.05, -0.05];
        let mut next = vec![0.18, -0.10, 0.12, 0.02];

        let stats = enforce_zero_mean_endogenous_height_change(&prev, &mut next);

        let mean_delta = next
            .iter()
            .zip(prev.iter())
            .map(|(next, prev)| next - prev)
            .sum::<f32>()
            / prev.len() as f32;
        assert!(mean_delta.abs() <= 1e-6);
        assert!(stats.adjusted_cells_ratio > 0.0);
        assert!(stats.std_delta.is_finite());
    }

    #[test]
    fn endogenous_height_change_respects_clamps() {
        let prev = vec![0.95, 0.90, -0.95, -0.90];
        let mut next = vec![1.0, 1.0, -0.70, -0.60];

        let stats = enforce_zero_mean_endogenous_height_change(&prev, &mut next);

        assert!(next.iter().all(|value| (-1.0..=1.0).contains(value)));
        let mean_delta = next
            .iter()
            .zip(prev.iter())
            .map(|(next, prev)| next - prev)
            .sum::<f32>()
            / prev.len() as f32;
        assert!(mean_delta.abs() <= 1e-6);
        assert!(stats.mean_abs_correction >= 0.0);
    }

    #[test]
    fn smoothing_limiter_reduces_excess_smoothing() {
        let factor = smoothing_limiter(0.01, 0.20, 0.05);

        assert!(factor < 1.0);
        assert!(factor >= 0.25);
    }

    #[test]
    fn smoothing_limiter_keeps_balanced_case_unchanged() {
        let factor = smoothing_limiter(0.10, 0.05, 0.05);

        assert!((factor - 1.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn smoothing_limiter_preserves_relief_when_forcing_is_weak() {
        let factor = smoothing_limiter(0.0, 0.02, 0.10);

        assert!(factor < 1.0);
    }
}
