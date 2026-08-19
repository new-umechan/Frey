use crate::sim::geology_types::{CrustType, PlateId, StressTensor};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, GeologyStepMetrics, VertexCrustState,
};
use crate::GeologyParams;

use crate::sim::exec::{
    DEFAULT_DIFFUSION_WEIGHT, GEOLOGY_HEIGHT_MAX, GEOLOGY_HEIGHT_MIN, MAX_HEIGHT_DELTA_PER_STEP,
};
const SMOOTHING_DOMINANCE_TARGET: f32 = 1.5;
const RELIEF_RETENTION_FRACTION_PER_STEP: f32 = 0.08;
const THICKNESS_RECOVERY_RATE: f32 = 0.08;
const RIGIDITY_RECOVERY_RATE: f32 = 0.18;
const MARINE_UPHILL_DIFFUSION_FLOOR: f32 = 0.04;
const MARINE_UPHILL_DIFFUSION_CEILING: f32 = 0.35;
const MARINE_SHALLOW_MIXING_DEPTH: f32 = 0.10;
const COASTAL_LAND_DOWN_DIFFUSION_FLOOR: f32 = 0.20;
const COASTAL_LAND_FREEBOARD_BAND: f32 = 0.08;
const STRESS_PROXY_LIMIT: f32 = 2.0;

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
    pub activity_scale: f32,
    pub params: &'a GeologyParams,
}

pub(super) struct SurfaceUpdateOutput<'a> {
    pub next_vertex_states: &'a mut [VertexCrustState],
    pub next_height: &'a mut [f32],
    pub next_volcanism: &'a mut [f32],
    pub next_vertex_buoyancy: &'a mut [f32],
}

#[derive(Clone, Copy)]
struct BoundarySurfaceForcingInput {
    boundary_type: BoundaryType,
    convergence: f32,
    divergence: f32,
    obliquity: f32,
    subduction_gate: f32,
    compressive: f32,
    tensile: f32,
    volcanism: f32,
    rollback_fraction: f32,
    slab_rollback: f32,
    is_subducting: bool,
    is_overriding: bool,
}

#[derive(Clone, Copy, Default)]
struct BoundarySurfaceForcing {
    tectonic_uplift: f32,
    volcanic_uplift: f32,
    tectonic_subsidence: f32,
}

fn boundary_surface_forcing(
    input: BoundarySurfaceForcingInput,
    params: &GeologyParams,
) -> BoundarySurfaceForcing {
    let collision_uplift = if input.boundary_type == BoundaryType::Collision {
        params.tectonic_uplift_gain.max(0.0) * input.convergence * (1.0 - 0.40 * input.obliquity)
    } else {
        0.0
    };
    let arc_uplift = if input.boundary_type == BoundaryType::Subduction && input.is_overriding {
        0.35 * params.tectonic_uplift_gain.max(0.0) * input.convergence * input.subduction_gate
    } else {
        0.0
    };
    let ridge_uplift = if input.boundary_type == BoundaryType::Ridge {
        0.20 * params.tectonic_uplift_gain.max(0.0) * input.divergence
    } else {
        0.0
    };
    let trench_subsidence =
        if input.boundary_type == BoundaryType::Subduction && input.is_subducting {
            params.trench_gain.max(0.0)
                * params.tectonic_subsidence_gain.max(0.0)
                * input.convergence
                * input.subduction_gate
        } else {
            0.0
        };
    let backarc_subsidence =
        if input.boundary_type == BoundaryType::Subduction && input.is_overriding {
            params.tectonic_subsidence_gain.max(0.0) * input.rollback_fraction * input.slab_rollback
        } else {
            0.0
        };
    let rift_subsidence = if input.boundary_type == BoundaryType::Rift {
        params.tectonic_subsidence_gain.max(0.0) * input.divergence
    } else {
        0.0
    };

    BoundarySurfaceForcing {
        tectonic_uplift: params.tectonic_uplift_gain.max(0.0) * input.compressive
            + collision_uplift
            + arc_uplift
            + ridge_uplift,
        volcanic_uplift: input.volcanism * params.volcanic_uplift_gain.max(0.0),
        tectonic_subsidence: params.tectonic_subsidence_gain.max(0.0) * input.tensile
            + trench_subsidence
            + backarc_subsidence
            + rift_subsidence,
    }
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
    let activity_scale = input.activity_scale.clamp(0.0, 1.0);
    let stress_memory_scale = activity_scale;
    let params = input.params;

    let next_vertex_states = &mut *output.next_vertex_states;
    let next_height = &mut *output.next_height;
    let next_volcanism = &mut *output.next_volcanism;
    let next_vertex_buoyancy = &mut *output.next_vertex_buoyancy;

    let cell_count = heights.len();
    let mut boundary_sum = 0.0_f32;
    let mut isostatic_equilibrium = vec![0.0; cell_count];
    let mut zero_mean_weights = vec![0.0; cell_count];
    let mut smoothing_limited_cells = 0u32;
    let mut smoothing_factor_sum = 0.0f32;
    let mut compressive_sum = 0.0f32;
    let mut tensile_sum = 0.0f32;
    let mut tectonic_uplift_sum = 0.0f32;
    let mut volcanic_uplift_sum = 0.0f32;
    let mut tectonic_subsidence_sum = 0.0f32;
    let mut thermal_subsidence_sum = 0.0f32;
    let mut thickness_equilibrium_gap_sum = 0.0f32;
    let mut isostatic_equilibrium_gap_sum = 0.0f32;
    let mut isostatic_reference_freeboard_sum = 0.0f32;
    let mut isostatic_compensated_anomaly_sum = 0.0f32;
    let mut density_ratio_sum = 0.0f32;
    let mut diffusive_raw_sum = 0.0f32;
    let mut diffusive_applied_sum = 0.0f32;
    let mut diffusive_land_down_raw_sum = 0.0f32;
    let mut diffusive_land_up_raw_sum = 0.0f32;
    let mut diffusive_ocean_down_raw_sum = 0.0f32;
    let mut diffusive_ocean_up_raw_sum = 0.0f32;
    let mut diffusive_ocean_up_applied_sum = 0.0f32;
    let mut isostatic_raw_sum = 0.0f32;
    let mut isostatic_applied_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_sum = 0.0f32;
    let mut isostatic_compensated_anomaly_applied_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_oceanic_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_orogenic_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_stable_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_stable_rift_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_stable_passive_transform_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_stable_passive_margin_sum = 0.0f32;
    let mut isostatic_reference_freeboard_applied_continental_stable_transform_sum = 0.0f32;
    let mut isostatic_reference_freeboard_raw_continental_stable_passive_margin_sum = 0.0f32;
    let mut isostatic_reference_freeboard_raw_continental_stable_transform_sum = 0.0f32;
    let mut passive_margin_continental_cells = 0u32;
    let mut passive_margin_isostatic_adjustment_rate_sum = 0.0f32;
    let mut passive_margin_smoothing_factor_sum = 0.0f32;
    let mut terrain_signed_delta_sum = 0.0f32;
    let mut min_surface_write_delta = f32::INFINITY;
    let mut max_surface_write_delta = f32::NEG_INFINITY;
    let mut surface_range_clamp_delta_sum = 0.0f32;
    let mut surface_raw_delta_sum = 0.0f32;
    let mut surface_step_delta_sum = 0.0f32;
    let mut surface_step_clamp_delta_sum = 0.0f32;
    let mut surface_pre_isostatic_delta_sum = 0.0f32;
    let mut surface_pre_zero_mean_delta_sum = 0.0f32;
    let mut debug_surface_max_delta_abs = -1.0f32;
    let mut debug_surface_max_delta_index = 0usize;
    let mut debug_surface_max_delta_raw_delta = 0.0f32;
    let mut debug_surface_max_delta_step_delta = 0.0f32;
    let mut debug_surface_max_delta_thermal_subsidence = 0.0f32;
    let mut debug_surface_max_delta_diffusive = 0.0f32;
    let mut debug_surface_max_delta_uplift = 0.0f32;
    let mut debug_surface_max_delta_tectonic_subsidence = 0.0f32;
    let mut debug_surface_max_delta_tensile = 0.0f32;
    let mut debug_surface_max_delta_stress = 0.0f32;
    let mut debug_surface_max_delta_height_before = 0.0f32;
    let mut debug_surface_max_delta_height_after_pre_isostatic = 0.0f32;

    for i in 0..cell_count {
        let boundary_type = boundary_state
            .dominant_type
            .get(i)
            .copied()
            .unwrap_or(BoundaryType::PassiveMargin);
        let boundary_activity =
            finite_or(boundary_state.activity.get(i).copied().unwrap_or(0.0), 0.0).clamp(0.0, 1.0)
                * activity_scale;
        let convergence =
            boundary_component(&boundary_state.convergence_component, i) * activity_scale;
        let divergence =
            boundary_component(&boundary_state.divergence_component, i) * activity_scale;
        let transform = boundary_component(&boundary_state.transform_component, i) * activity_scale;
        let obliquity = boundary_component(&boundary_state.obliquity, i);
        let subduction_gate = boundary_component(&boundary_state.subduction_gate, i);

        let mut tensor = boundary_tensor(
            boundary_type,
            boundary_activity,
            convergence,
            divergence,
            transform,
            obliquity,
        );

        let plume =
            finite_or(plume_force.get(i).copied().unwrap_or(0.0), 0.0).max(0.0) * activity_scale;
        tensor.xx += plume * 0.7;
        tensor.yy += plume * 0.7;
        let slab_conv = boundary_state
            .slab_convergence_component
            .get(i)
            .copied()
            .map(|v| finite_or(v, 0.0))
            .unwrap_or(0.0);
        let slab_conv = slab_conv * activity_scale;
        let slab_roll = boundary_state
            .slab_rollback_component
            .get(i)
            .copied()
            .map(|v| finite_or(v, 0.0))
            .unwrap_or(0.0);
        let slab_roll = slab_roll * activity_scale;
        tensor.xx -= slab_conv * (0.06 + 0.05 * subduction_gate);
        tensor.yy -= slab_conv * (0.06 + 0.05 * subduction_gate);
        tensor.xx += slab_roll * 0.05;
        tensor.yy += slab_roll * 0.03;
        let backarc_tension = boundary_state
            .backarc_tension
            .get(i)
            .copied()
            .map(|v| finite_or(v, 0.0))
            .unwrap_or(0.0);
        let backarc_tension = backarc_tension * activity_scale;
        tensor.xx += backarc_tension;
        tensor.yy += backarc_tension;

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let plate_i = plate_id[i];
        let subducting_plate = boundary_state.subducting_plate.get(i).copied().flatten();
        let is_subducting = subducting_plate == Some(plate_i);
        let is_overriding = subducting_plate.is_some() && !is_subducting;
        let height_i = heights[i];
        let neighbors = &nbrs[start..end];
        let mut nbr_sum = 0.0;
        let mut nbr_count = 0usize;
        let mut land_neighbor_count = 0usize;
        let mut nbr_stress_xx = 0.0;
        let mut nbr_stress_yy = 0.0;
        let mut nbr_stress_xy = 0.0;

        for &n_u32 in neighbors {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            let neighbor_height = heights[n];
            nbr_sum += neighbor_height;
            nbr_count += 1;
            if neighbor_height > 0.0 {
                land_neighbor_count += 1;
            }
            let n_tensor = next_vertex_states[n].stress_tensor;
            let atten = 0.18 - 0.06 * f32::from((plate_id[n] == plate_i) as u8);
            nbr_stress_xx += finite_or(n_tensor.xx, 0.0) * atten * stress_memory_scale;
            nbr_stress_yy += finite_or(n_tensor.yy, 0.0) * atten * stress_memory_scale;
            nbr_stress_xy += finite_or(n_tensor.xy, 0.0) * atten * stress_memory_scale;
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
        tensor.xx = finite_or(tensor.xx, 0.0).clamp(-STRESS_PROXY_LIMIT, STRESS_PROXY_LIMIT);
        tensor.yy = finite_or(tensor.yy, 0.0).clamp(-STRESS_PROXY_LIMIT, STRESS_PROXY_LIMIT);
        tensor.xy = finite_or(tensor.xy, 0.0).clamp(-STRESS_PROXY_LIMIT, STRESS_PROXY_LIMIT);

        let stress_scalar = finite_or((tensor.xx + tensor.yy) * 0.5 + tensor.xy.abs() * 0.30, 0.0)
            .clamp(-STRESS_PROXY_LIMIT, STRESS_PROXY_LIMIT);
        let relax = params.stress_relaxation_rate.clamp(0.0, 1.0);
        let carried_stress = prev.stress * stress_memory_scale;
        let stress = finite_or(carried_stress * (1.0 - relax) + stress_scalar * relax, 0.0)
            .clamp(-STRESS_PROXY_LIMIT, STRESS_PROXY_LIMIT);

        let mut state = prev;
        state.temperature = mantle_heat_i;
        state.stress_tensor = tensor;
        state.stress = stress;

        if state.crust_type == CrustType::Oceanic {
            let age_inc =
                params.age_advection_gain.max(0.0) * (0.6 + 0.4 * (1.0 - plume)) * activity_scale;
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
        let subduction_memory = if boundary_type == BoundaryType::Subduction {
            finite_or(
                (slab_conv + slab_roll)
                    .max(boundary_activity)
                    .max(subduction_gate),
                0.0,
            )
            .clamp(0.0, 1.0)
        } else {
            0.0
        };

        state.arc_volcanism = if boundary_type == BoundaryType::Subduction && is_overriding {
            boundary_activity
                * (0.45 + 0.55 * subduction_gate)
                * (0.35 + 0.65 * subduction_memory)
                * params.arc_volcanism_gain.max(0.0)
        } else {
            0.0
        };
        state.ridge_volcanism = if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift)
        {
            boundary_activity
                * (0.40 + 0.60 * divergence.max(0.0).clamp(0.0, 1.0))
                * params.ridge_volcanism_gain.max(0.0)
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

        let forcing = boundary_surface_forcing(
            BoundarySurfaceForcingInput {
                boundary_type,
                convergence,
                divergence,
                obliquity,
                subduction_gate,
                compressive,
                tensile,
                volcanism,
                rollback_fraction,
                slab_rollback: slab_roll,
                is_subducting,
                is_overriding,
            },
            params,
        );
        let tectonic_uplift = forcing.tectonic_uplift;
        let volcanic_uplift = forcing.volcanic_uplift;
        let uplift = tectonic_uplift + volcanic_uplift;
        let tectonic_subsidence = forcing.tectonic_subsidence;
        let thermal_subsidence = if state.crust_type == CrustType::Oceanic {
            let age_norm = (state.age / params.age_ref.max(1e-4)).clamp(0.0, 1.0);
            params.thermal_subsidence_gain.max(0.0) * age_norm.sqrt() * activity_scale
        } else {
            0.0
        };
        let total_subsidence = tectonic_subsidence + thermal_subsidence;
        tectonic_uplift_sum += tectonic_uplift.abs();
        volcanic_uplift_sum += volcanic_uplift.abs();
        tectonic_subsidence_sum += tectonic_subsidence.abs();
        thermal_subsidence_sum += thermal_subsidence.abs();

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
        let coastal_land_fraction = if nbr_count == 0 {
            0.0
        } else {
            land_neighbor_count as f32 / nbr_count as f32
        };
        let coastal_ocean_fraction = 1.0 - coastal_land_fraction;
        let diffusive_raw =
            apply_marine_uphill_diffusion_limit(diffusive_raw, height_i, coastal_land_fraction);
        let diffusive_raw = apply_coastal_land_down_diffusion_limit(
            diffusive_raw,
            height_i,
            coastal_ocean_fraction,
        );
        let diffusive_raw = diffusive_raw * activity_scale;
        let density_ratio = (state.density / params.mantle_density.max(1e-3)).clamp(0.1, 1.4);
        density_ratio_sum += density_ratio;
        let (reference_freeboard, compensated_anomaly) = isostatic_components(
            state.crust_type,
            state.thickness,
            state.age,
            params.age_ref,
            density_ratio,
            boundary_type,
            plume,
        );
        let h_eq = finite_or(reference_freeboard + compensated_anomaly, height_i);
        isostatic_reference_freeboard_sum += reference_freeboard.abs();
        isostatic_compensated_anomaly_sum += compensated_anomaly.abs();
        let isostatic_adjustment_rate = local_isostatic_relaxation_rate(
            params.isostatic_adjustment_rate.max(0.0),
            state.rigidity,
            mantle_heat_i,
            state.thickness,
            state.crust_type,
        ) * activity_scale;
        let isostatic_raw = (h_eq - height_i) * isostatic_adjustment_rate;
        diffusive_raw_sum += diffusive_raw.abs();
        if height_i > 0.0 {
            if diffusive_raw < 0.0 {
                diffusive_land_down_raw_sum += diffusive_raw.abs();
            } else {
                diffusive_land_up_raw_sum += diffusive_raw.abs();
            }
        } else if diffusive_raw < 0.0 {
            diffusive_ocean_down_raw_sum += diffusive_raw.abs();
        } else {
            diffusive_ocean_up_raw_sum += diffusive_raw.abs();
        }
        isostatic_raw_sum += isostatic_raw.abs();
        let endogenous_forcing = uplift.abs()
            + tectonic_subsidence.abs()
            + volcanism
            + plume * params.plume_gain.max(0.0);
        zero_mean_weights[i] = endogenous_forcing.max(0.0);
        let local_relief = (mean_neighbor_height - height_i).abs() + (h_eq - height_i).abs();
        let smoothing_strength = diffusive_raw.abs() + isostatic_raw.abs();
        let smoothing_factor =
            smoothing_limiter(endogenous_forcing, smoothing_strength, local_relief);
        if smoothing_factor < 1.0 - 1e-6 {
            smoothing_limited_cells = smoothing_limited_cells.saturating_add(1);
        }
        smoothing_factor_sum += smoothing_factor;
        let diffusive = diffusive_raw * smoothing_factor;
        diffusive_applied_sum += diffusive.abs();
        if height_i <= 0.0 && diffusive > 0.0 {
            diffusive_ocean_up_applied_sum += diffusive.abs();
        }

        let raw_delta = finite_or(uplift - total_subsidence + diffusive, 0.0);
        surface_raw_delta_sum += raw_delta.abs();
        let delta = raw_delta.clamp(-MAX_HEIGHT_DELTA_PER_STEP, MAX_HEIGHT_DELTA_PER_STEP);
        surface_step_delta_sum += delta.abs();
        surface_step_clamp_delta_sum += (delta - raw_delta).abs();
        let unclamped_next_h = finite_or(heights[i] + delta, heights[i]);
        let mut next_h = unclamped_next_h.clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
        surface_range_clamp_delta_sum += (next_h - unclamped_next_h).abs();
        surface_pre_isostatic_delta_sum += (next_h - heights[i]).abs();
        let pre_isostatic_delta_abs = (next_h - heights[i]).abs();
        if pre_isostatic_delta_abs > debug_surface_max_delta_abs {
            debug_surface_max_delta_abs = pre_isostatic_delta_abs;
            debug_surface_max_delta_index = i;
            debug_surface_max_delta_raw_delta = raw_delta;
            debug_surface_max_delta_step_delta = delta;
            debug_surface_max_delta_thermal_subsidence = thermal_subsidence;
            debug_surface_max_delta_diffusive = diffusive;
            debug_surface_max_delta_uplift = uplift;
            debug_surface_max_delta_tectonic_subsidence = tectonic_subsidence;
            debug_surface_max_delta_tensile = tensile;
            debug_surface_max_delta_stress = stress;
            debug_surface_max_delta_height_before = heights[i];
            debug_surface_max_delta_height_after_pre_isostatic = next_h;
        }

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
        let thickness_target = equilibrium_thickness(
            state.crust_type,
            state.age,
            params.age_ref,
            boundary_type,
            plume,
        );
        thickness_equilibrium_gap_sum += (thickness_target - state.thickness).abs();
        let thickness_recovery = local_thickness_recovery_rate(
            THICKNESS_RECOVERY_RATE,
            state.crust_type,
            boundary_type,
            boundary_activity,
            mantle_heat_i,
            state.stress,
        ) * activity_scale;
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
        let (reference_freeboard, compensated_anomaly) = isostatic_components(
            state.crust_type,
            state.thickness,
            state.age,
            params.age_ref,
            density_ratio,
            boundary_type,
            plume,
        );
        let h_eq = finite_or(reference_freeboard + compensated_anomaly, next_h);
        isostatic_equilibrium_gap_sum += (h_eq - next_h).abs();
        let isostatic_adjustment_rate = isostatic_adjustment_rate * smoothing_factor;
        let mut isostatic_reference_freeboard_applied =
            (reference_freeboard - next_h) * isostatic_adjustment_rate;
        if state.crust_type == CrustType::Oceanic && isostatic_reference_freeboard_applied > 0.0 {
            isostatic_reference_freeboard_applied *=
                marine_shoreline_attenuation(next_h, coastal_land_fraction);
        }
        let isostatic_compensated_anomaly_applied = compensated_anomaly * isostatic_adjustment_rate;
        let isostatic_applied =
            isostatic_reference_freeboard_applied + isostatic_compensated_anomaly_applied;
        isostatic_applied_sum += isostatic_applied.abs();
        isostatic_reference_freeboard_applied_sum += isostatic_reference_freeboard_applied.abs();
        isostatic_compensated_anomaly_applied_sum += isostatic_compensated_anomaly_applied.abs();
        match state.crust_type {
            CrustType::Oceanic => {
                isostatic_reference_freeboard_applied_oceanic_sum +=
                    isostatic_reference_freeboard_applied;
            }
            CrustType::Continental => {
                isostatic_reference_freeboard_applied_continental_sum +=
                    isostatic_reference_freeboard_applied;
                if matches!(
                    boundary_type,
                    BoundaryType::Collision | BoundaryType::Subduction
                ) {
                    isostatic_reference_freeboard_applied_continental_orogenic_sum +=
                        isostatic_reference_freeboard_applied;
                } else {
                    isostatic_reference_freeboard_applied_continental_stable_sum +=
                        isostatic_reference_freeboard_applied;
                    if matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift) {
                        isostatic_reference_freeboard_applied_continental_stable_rift_sum +=
                            isostatic_reference_freeboard_applied;
                    } else {
                        isostatic_reference_freeboard_applied_continental_stable_passive_transform_sum +=
                            isostatic_reference_freeboard_applied;
                        if matches!(boundary_type, BoundaryType::PassiveMargin) {
                            isostatic_reference_freeboard_applied_continental_stable_passive_margin_sum +=
                                isostatic_reference_freeboard_applied;
                            isostatic_reference_freeboard_raw_continental_stable_passive_margin_sum +=
                                reference_freeboard;
                            passive_margin_continental_cells =
                                passive_margin_continental_cells.saturating_add(1);
                            passive_margin_isostatic_adjustment_rate_sum +=
                                isostatic_adjustment_rate;
                            passive_margin_smoothing_factor_sum += smoothing_factor;
                        } else if matches!(boundary_type, BoundaryType::Transform) {
                            isostatic_reference_freeboard_applied_continental_stable_transform_sum +=
                                isostatic_reference_freeboard_applied;
                            isostatic_reference_freeboard_raw_continental_stable_transform_sum +=
                                reference_freeboard;
                        }
                    }
                }
            }
        }
        next_h = finite_or(next_h + isostatic_applied, next_h)
            .clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
        let (rigidity_min, rigidity_max) = crust_rigidity_bounds(state.crust_type);
        let rigidity_target = equilibrium_rigidity(
            state.crust_type,
            state.thickness,
            mantle_heat_i,
            boundary_activity,
        );
        state.rigidity = finite_or(
            rigidity + (rigidity_target - rigidity) * RIGIDITY_RECOVERY_RATE * activity_scale,
            rigidity,
        )
        .clamp(rigidity_min, rigidity_max);

        boundary_sum += boundary_state.activity.get(i).copied().unwrap_or(0.0);

        next_vertex_states[i] = state;
        next_height[i] = next_h;
        surface_pre_zero_mean_delta_sum += (next_h - heights[i]).abs();
        next_volcanism[i] = volcanism;
        isostatic_equilibrium[i] = h_eq;
    }

    let pre_zero_mean_height = next_height.to_vec();
    let zero_mean_stats =
        enforce_zero_mean_endogenous_height_change(heights, next_height, &zero_mean_weights);
    let surface_zero_mean_delta_sum = pre_zero_mean_height
        .iter()
        .zip(next_height.iter())
        .map(|(before, after)| (after - before).abs())
        .sum::<f32>();

    let mut terrain_delta_sum = 0.0_f32;
    let mut uplift_sum = 0.0_f32;
    let mut subsidence_sum = 0.0_f32;
    for i in 0..cell_count {
        let delta = next_height[i] - heights[i];
        terrain_delta_sum += delta.abs();
        terrain_signed_delta_sum += delta;
        min_surface_write_delta = min_surface_write_delta.min(delta);
        max_surface_write_delta = max_surface_write_delta.max(delta);
        if delta > 0.0 {
            uplift_sum += delta;
        } else {
            subsidence_sum += -delta;
        }
        next_vertex_buoyancy[i] = finite_or(isostatic_equilibrium[i] - next_height[i], 0.0);
    }
    let bedrock_zero_level_coastal_band_ratio = zero_level_coastal_band_ratio(next_height, 0.02);
    let (bedrock_freeboard_p10, bedrock_freeboard_p50, bedrock_freeboard_p90) =
        positive_height_percentiles(next_height);

    let denom = cell_count.max(1) as f32;
    GeologyStepMetrics {
        geology_activity: (terrain_delta_sum / denom).clamp(0.0, 1.0),
        boundary_activity: (boundary_sum / denom).clamp(0.0, 1.0),
        plate_id_churn_rate: 0.0,
        boundary_crossing_substeps: 0.0,
        boundary_topology_event_cell_count: 0.0,
        boundary_topology_constrained_segment_count: 0.0,
        boundary_motion_raw_expected_cell_count: 0.0,
        boundary_motion_accumulated_expected_cell_count: 0.0,
        boundary_motion_component_budget_cell_count: 0.0,
        boundary_motion_transferable_component_budget_cell_count: 0.0,
        boundary_motion_plate_consistency_budget_cell_count: 0.0,
        boundary_motion_plate_consistency_deferred_cell_count: 0.0,
        boundary_motion_plate_consistency_donor_limited_cell_count: 0.0,
        boundary_motion_plate_consistency_outgoing_limited_cell_count: 0.0,
        boundary_motion_plate_consistency_incoming_limited_cell_count: 0.0,
        boundary_motion_plate_consistency_net_area_limited_cell_count: 0.0,
        boundary_motion_plate_consistency_max_projected_out_ratio: 0.0,
        boundary_motion_actual_transfer_cell_count: 0.0,
        boundary_motion_patch_rejected_component_count: 0.0,
        boundary_motion_patch_rejected_budget_cell_count: 0.0,
        boundary_motion_source_fragment_rejected_component_count: 0.0,
        boundary_motion_source_fragment_rejected_budget_cell_count: 0.0,
        boundary_motion_target_disconnected_rejected_component_count: 0.0,
        boundary_motion_target_disconnected_rejected_budget_cell_count: 0.0,
        boundary_motion_budget_utilization_ratio: 1.0,
        boundary_motion_plate_consistency_limited_ratio: 0.0,
        boundary_motion_component_limited_ratio: 0.0,
        material_reconstruction_hard_capacity_assigned_cell_count: 0.0,
        material_reconstruction_closure_assigned_cell_count: 0.0,
        material_reconstruction_rebalanced_cell_count: 0.0,
        material_reconstruction_capacity_mismatch_cell_count: 0.0,
        material_reconstruction_non_dominant_assignment_cell_count: 0.0,
        material_reconstruction_mean_assigned_confidence: 0.0,
        persistent_material_gap_ratio: 0.0,
        persistent_material_overlap_ratio: 0.0,
        persistent_material_unsupported_gap_ratio: 0.0,
        persistent_material_subduction_overlap_ratio: 0.0,
        persistent_material_collision_overlap_ratio: 0.0,
        persistent_material_unsupported_overlap_ratio: 0.0,
        persistent_material_element_count: 0.0,
        persistent_material_ownership_marker_count: 0.0,
        marker_empty_candidate_cell_count: 0.0,
        marker_single_candidate_cell_count: 0.0,
        marker_mixed_candidate_cell_count: 0.0,
        marker_changed_empty_candidate_cell_count: 0.0,
        marker_changed_single_candidate_cell_count: 0.0,
        marker_changed_mixed_candidate_cell_count: 0.0,
        marker_reversed_empty_candidate_cell_count: 0.0,
        marker_reversed_single_candidate_cell_count: 0.0,
        marker_reversed_mixed_candidate_cell_count: 0.0,
        marker_changed_divergent_cell_count: 0.0,
        marker_changed_subduction_cell_count: 0.0,
        marker_changed_collision_cell_count: 0.0,
        marker_changed_transform_cell_count: 0.0,
        orphan_cell_count: 0.0,
        single_cell_plate_count: 0.0,
        activity_scale,
        runtime_rebuild_applied: 0.0,
        mean_abs_surface_write_delta: finite_or(terrain_delta_sum / denom, 0.0),
        mean_signed_surface_write_delta: finite_or(terrain_signed_delta_sum / denom, 0.0),
        min_surface_write_delta: finite_or(min_surface_write_delta, 0.0),
        max_surface_write_delta: finite_or(max_surface_write_delta, 0.0),
        mean_abs_surface_range_clamp_delta: finite_or(surface_range_clamp_delta_sum / denom, 0.0),
        mean_abs_surface_raw_delta: finite_or(surface_raw_delta_sum / denom, 0.0),
        mean_abs_surface_step_delta: finite_or(surface_step_delta_sum / denom, 0.0),
        mean_abs_surface_step_clamp_delta: finite_or(surface_step_clamp_delta_sum / denom, 0.0),
        mean_abs_surface_pre_isostatic_delta: finite_or(
            surface_pre_isostatic_delta_sum / denom,
            0.0,
        ),
        mean_abs_surface_output_delta: 0.0,
        mean_abs_surface_pre_zero_mean_delta: finite_or(
            surface_pre_zero_mean_delta_sum / denom,
            0.0,
        ),
        mean_abs_surface_zero_mean_delta: finite_or(surface_zero_mean_delta_sum / denom, 0.0),
        debug_surface_max_delta_index: debug_surface_max_delta_index as f32,
        debug_surface_max_delta_raw_delta: finite_or(debug_surface_max_delta_raw_delta, 0.0),
        debug_surface_max_delta_step_delta: finite_or(debug_surface_max_delta_step_delta, 0.0),
        debug_surface_max_delta_thermal_subsidence: finite_or(
            debug_surface_max_delta_thermal_subsidence,
            0.0,
        ),
        debug_surface_max_delta_diffusive: finite_or(debug_surface_max_delta_diffusive, 0.0),
        debug_surface_max_delta_uplift: finite_or(debug_surface_max_delta_uplift, 0.0),
        debug_surface_max_delta_tectonic_subsidence: finite_or(
            debug_surface_max_delta_tectonic_subsidence,
            0.0,
        ),
        debug_surface_max_delta_tensile: finite_or(debug_surface_max_delta_tensile, 0.0),
        debug_surface_max_delta_stress: finite_or(debug_surface_max_delta_stress, 0.0),
        debug_surface_max_delta_height_before: finite_or(
            debug_surface_max_delta_height_before,
            0.0,
        ),
        debug_surface_max_delta_height_after_pre_isostatic: finite_or(
            debug_surface_max_delta_height_after_pre_isostatic,
            0.0,
        ),
        uplift_rate: finite_or(uplift_sum / denom, 0.0),
        subsidence_rate: finite_or(subsidence_sum / denom, 0.0),
        smoothing_limited_cells_ratio: finite_or(smoothing_limited_cells as f32 / denom, 0.0),
        mean_smoothing_factor: finite_or(smoothing_factor_sum / denom, 1.0),
        zero_mean_adjusted_cells_ratio: finite_or(zero_mean_stats.adjusted_cells_ratio, 0.0),
        zero_mean_mean_abs_correction: finite_or(zero_mean_stats.mean_abs_correction, 0.0),
        zero_mean_std_delta: finite_or(zero_mean_stats.std_delta, 0.0),
        mean_compressive: finite_or(compressive_sum / denom, 0.0),
        mean_tensile: finite_or(tensile_sum / denom, 0.0),
        mean_abs_tectonic_uplift: finite_or(tectonic_uplift_sum / denom, 0.0),
        mean_abs_volcanic_uplift: finite_or(volcanic_uplift_sum / denom, 0.0),
        mean_abs_tectonic_subsidence: finite_or(tectonic_subsidence_sum / denom, 0.0),
        mean_abs_thermal_subsidence: finite_or(thermal_subsidence_sum / denom, 0.0),
        mean_abs_thickness_equilibrium_gap: finite_or(thickness_equilibrium_gap_sum / denom, 0.0),
        mean_abs_isostatic_equilibrium_gap: finite_or(isostatic_equilibrium_gap_sum / denom, 0.0),
        mean_abs_isostatic_reference_freeboard: finite_or(
            isostatic_reference_freeboard_sum / denom,
            0.0,
        ),
        mean_abs_isostatic_compensated_anomaly: finite_or(
            isostatic_compensated_anomaly_sum / denom,
            0.0,
        ),
        mean_density_ratio: finite_or(density_ratio_sum / denom, 0.0),
        mean_abs_diffusive_raw: finite_or(diffusive_raw_sum / denom, 0.0),
        mean_abs_diffusive_applied: finite_or(diffusive_applied_sum / denom, 0.0),
        mean_abs_diffusive_land_down_raw: finite_or(diffusive_land_down_raw_sum / denom, 0.0),
        mean_abs_diffusive_land_up_raw: finite_or(diffusive_land_up_raw_sum / denom, 0.0),
        mean_abs_diffusive_ocean_down_raw: finite_or(diffusive_ocean_down_raw_sum / denom, 0.0),
        mean_abs_diffusive_ocean_up_raw: finite_or(diffusive_ocean_up_raw_sum / denom, 0.0),
        mean_abs_diffusive_ocean_up_applied: finite_or(diffusive_ocean_up_applied_sum / denom, 0.0),
        mean_abs_isostatic_raw: finite_or(isostatic_raw_sum / denom, 0.0),
        mean_abs_isostatic_applied: finite_or(isostatic_applied_sum / denom, 0.0),
        mean_abs_isostatic_reference_freeboard_applied: finite_or(
            isostatic_reference_freeboard_applied_sum / denom,
            0.0,
        ),
        mean_abs_isostatic_compensated_anomaly_applied: finite_or(
            isostatic_compensated_anomaly_applied_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_applied_oceanic: finite_or(
            isostatic_reference_freeboard_applied_oceanic_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_applied_continental: finite_or(
            isostatic_reference_freeboard_applied_continental_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_applied_continental_orogenic: finite_or(
            isostatic_reference_freeboard_applied_continental_orogenic_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_applied_continental_stable: finite_or(
            isostatic_reference_freeboard_applied_continental_stable_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_applied_continental_stable_rift: finite_or(
            isostatic_reference_freeboard_applied_continental_stable_rift_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_transform:
            finite_or(
                isostatic_reference_freeboard_applied_continental_stable_passive_transform_sum
                    / denom,
                0.0,
            ),
        mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_margin:
            finite_or(
                isostatic_reference_freeboard_applied_continental_stable_passive_margin_sum / denom,
                0.0,
            ),
        mean_signed_isostatic_reference_freeboard_applied_continental_stable_transform: finite_or(
            isostatic_reference_freeboard_applied_continental_stable_transform_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_raw_continental_stable_passive_margin: finite_or(
            isostatic_reference_freeboard_raw_continental_stable_passive_margin_sum / denom,
            0.0,
        ),
        mean_signed_isostatic_reference_freeboard_raw_continental_stable_transform: finite_or(
            isostatic_reference_freeboard_raw_continental_stable_transform_sum / denom,
            0.0,
        ),
        passive_margin_continental_cell_ratio: finite_or(
            passive_margin_continental_cells as f32 / denom,
            0.0,
        ),
        mean_passive_margin_isostatic_adjustment_rate: finite_or(
            passive_margin_isostatic_adjustment_rate_sum
                / (passive_margin_continental_cells as f32).max(1.0),
            0.0,
        ),
        mean_passive_margin_smoothing_factor: finite_or(
            passive_margin_smoothing_factor_sum
                / (passive_margin_continental_cells as f32).max(1.0),
            0.0,
        ),
        passive_margin_reference_freeboard_effective_applied_factor: finite_or(
            isostatic_reference_freeboard_applied_continental_stable_passive_margin_sum
                / isostatic_reference_freeboard_raw_continental_stable_passive_margin_sum.max(1e-6),
            0.0,
        ),
        crust_recentering_shift: 0.0,
        crust_recentering_pre_band_ratio: bedrock_zero_level_coastal_band_ratio,
        crust_recentering_post_band_ratio: bedrock_zero_level_coastal_band_ratio,
        bedrock_zero_level_coastal_band_ratio,
        bedrock_freeboard_p10,
        bedrock_freeboard_p50,
        bedrock_freeboard_p90,
    }
}

fn zero_level_coastal_band_ratio(heights: &[f32], band: f32) -> f32 {
    if heights.is_empty() {
        return 0.0;
    }
    let in_band = heights
        .iter()
        .filter(|&&height| height.abs() <= band)
        .count();
    in_band as f32 / heights.len() as f32
}

fn positive_height_percentiles(heights: &[f32]) -> (f32, f32, f32) {
    let mut values = heights
        .iter()
        .copied()
        .filter(|height| *height > 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (
        percentile_sorted(&values, 0.10),
        percentile_sorted(&values, 0.50),
        percentile_sorted(&values, 0.90),
    )
}

fn percentile_sorted(values: &[f32], quantile: f32) -> f32 {
    if values.len() == 1 {
        return values[0];
    }
    let q = quantile.clamp(0.0, 1.0);
    let position = q * (values.len() - 1) as f32;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return values[lower];
    }
    let weight = position - lower as f32;
    values[lower] * (1.0 - weight) + values[upper] * weight
}

fn smoothing_limiter(endogenous_forcing: f32, smoothing_strength: f32, local_relief: f32) -> f32 {
    if !endogenous_forcing.is_finite() || !smoothing_strength.is_finite() {
        return 1.0;
    }
    if smoothing_strength <= 1e-6 {
        return 1.0;
    }
    if smoothing_strength <= endogenous_forcing.max(0.0) {
        return 1.0;
    }

    let forcing_budget = endogenous_forcing.max(1e-4) * SMOOTHING_DOMINANCE_TARGET;
    let relief_budget = local_relief.max(1e-4) * RELIEF_RETENTION_FRACTION_PER_STEP;
    let minimum_retained_smoothing = if endogenous_forcing > 0.0 {
        smoothing_strength * 0.25
    } else {
        0.0
    };
    let allowable_smoothing = forcing_budget
        .min(relief_budget)
        .max(minimum_retained_smoothing)
        .max(1e-6);
    if smoothing_strength <= allowable_smoothing {
        1.0
    } else {
        (allowable_smoothing / smoothing_strength).clamp(0.0, 1.0)
    }
}

fn apply_marine_uphill_diffusion_limit(
    diffusive_raw: f32,
    height: f32,
    coastal_land_fraction: f32,
) -> f32 {
    if diffusive_raw <= 0.0 || height > 0.0 {
        return diffusive_raw;
    }

    diffusive_raw * marine_shoreline_attenuation(height, coastal_land_fraction)
}

fn marine_shoreline_attenuation(height: f32, coastal_land_fraction: f32) -> f32 {
    let depth = (-height).max(0.0);
    let shallow_factor = (1.0 - depth / MARINE_SHALLOW_MIXING_DEPTH).clamp(0.0, 1.0);
    let coastal_supply = coastal_land_fraction.clamp(0.0, 1.0).sqrt();
    (MARINE_UPHILL_DIFFUSION_FLOOR
        + (MARINE_UPHILL_DIFFUSION_CEILING - MARINE_UPHILL_DIFFUSION_FLOOR)
            * shallow_factor
            * coastal_supply)
        .clamp(0.0, 1.0)
}

fn apply_coastal_land_down_diffusion_limit(
    diffusive_raw: f32,
    height: f32,
    coastal_ocean_fraction: f32,
) -> f32 {
    if diffusive_raw >= 0.0 || height <= 0.0 {
        return diffusive_raw;
    }

    let freeboard_norm = (height / COASTAL_LAND_FREEBOARD_BAND).clamp(0.0, 1.0);
    let shoreline_exposure = coastal_ocean_fraction.clamp(0.0, 1.0).sqrt();
    let attenuation = 1.0
        - (1.0 - COASTAL_LAND_DOWN_DIFFUSION_FLOOR) * (1.0 - freeboard_norm) * shoreline_exposure;
    diffusive_raw * attenuation.clamp(0.0, 1.0)
}

fn equilibrium_thickness(
    crust_type: CrustType,
    age: f32,
    age_ref: f32,
    boundary_type: BoundaryType,
    plume: f32,
) -> f32 {
    let age_norm = (age / age_ref.max(1e-4)).clamp(0.0, 1.0);
    let (reference_thickness, _) =
        reference_isostatic_column(crust_type, age_norm, boundary_type, plume);
    reference_thickness
}

fn isostatic_components(
    crust_type: CrustType,
    thickness: f32,
    age: f32,
    age_ref: f32,
    density_ratio: f32,
    boundary_type: BoundaryType,
    plume: f32,
) -> (f32, f32) {
    let age_norm = (age / age_ref.max(1e-4)).clamp(0.0, 1.0);
    let (reference_thickness, reference_freeboard) =
        reference_isostatic_column(crust_type, age_norm, boundary_type, plume);
    let compensated_anomaly = finite_or(
        (thickness - reference_thickness) * (1.0 - density_ratio),
        0.0,
    );
    (reference_freeboard, compensated_anomaly)
}

fn reference_isostatic_column(
    crust_type: CrustType,
    age_norm: f32,
    boundary_type: BoundaryType,
    plume: f32,
) -> (f32, f32) {
    let plume_bonus = plume.clamp(0.0, 1.0) * 0.03;
    match crust_type {
        CrustType::Oceanic => {
            let ridge_bonus = match boundary_type {
                BoundaryType::Ridge | BoundaryType::Rift => 0.02,
                BoundaryType::Subduction => -0.01,
                BoundaryType::Collision | BoundaryType::Transform | BoundaryType::PassiveMargin => {
                    0.0
                }
            };
            let reference_thickness = 0.30 + age_norm.sqrt() * 0.08 + plume_bonus * 0.5;
            let reference_freeboard =
                -0.08 - age_norm.sqrt() * 0.04 + ridge_bonus + plume_bonus * 0.5;
            (
                reference_thickness.clamp(0.22, 0.55),
                reference_freeboard.clamp(-0.18, 0.01),
            )
        }
        CrustType::Continental => {
            let collision_bonus = match boundary_type {
                BoundaryType::Collision | BoundaryType::Subduction => 0.02,
                BoundaryType::Ridge | BoundaryType::Rift => -0.015,
                BoundaryType::Transform | BoundaryType::PassiveMargin => 0.0,
            };
            let reference_thickness = 0.66 + collision_bonus + plume_bonus * 0.5;
            let stable_base = match boundary_type {
                BoundaryType::Collision | BoundaryType::Subduction => 0.03,
                BoundaryType::Ridge | BoundaryType::Rift => 0.015,
                BoundaryType::Transform | BoundaryType::PassiveMargin => 0.012,
            };
            let reference_freeboard = stable_base + collision_bonus * 0.25 + plume_bonus * 0.35;
            (
                reference_thickness.clamp(0.48, 0.95),
                reference_freeboard.clamp(0.005, 0.08),
            )
        }
    }
}

fn local_isostatic_relaxation_rate(
    base_rate: f32,
    rigidity: f32,
    mantle_heat: f32,
    thickness: f32,
    crust_type: CrustType,
) -> f32 {
    let (rigidity_min, rigidity_max) = crust_rigidity_bounds(crust_type);
    let rigidity_norm =
        ((rigidity - rigidity_min) / (rigidity_max - rigidity_min).max(1e-4)).clamp(0.0, 1.0);
    let thermal_softening = 0.45 + 0.55 * mantle_heat.clamp(0.0, 1.0);
    let thickness_drag = 1.0 + (thickness - 0.35).max(0.0) * 0.8;
    let mobility = thermal_softening * (1.05 - 0.55 * rigidity_norm) / thickness_drag.max(0.25);
    let driver = (base_rate * mobility.max(0.05)).clamp(0.0, 1.0);
    1.0 - (-driver).exp()
}

fn local_thickness_recovery_rate(
    base_rate: f32,
    crust_type: CrustType,
    boundary_type: BoundaryType,
    boundary_activity: f32,
    mantle_heat: f32,
    stress: f32,
) -> f32 {
    let boundary_drive = match boundary_type {
        BoundaryType::Collision | BoundaryType::Subduction => 0.70,
        BoundaryType::Ridge | BoundaryType::Rift => 0.55,
        BoundaryType::Transform => 0.30,
        BoundaryType::PassiveMargin => 0.18,
    };
    let crust_mobility = match crust_type {
        CrustType::Oceanic => 0.80,
        CrustType::Continental => 0.45,
    };
    let thermal_drive = 0.35 + 0.65 * mantle_heat.clamp(0.0, 1.0);
    let stress_drive = 1.0 - (-stress.abs() * 6.0).exp();
    let activity_drive = boundary_drive * boundary_activity.clamp(0.0, 1.0);
    let driver =
        base_rate * crust_mobility * thermal_drive * (0.60 * activity_drive + 0.40 * stress_drive);
    1.0 - (-driver).exp()
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
    weights: &[f32],
) -> ZeroMeanCorrectionStats {
    if prev_height.len() != next_height.len() || next_height.is_empty() {
        return ZeroMeanCorrectionStats::default();
    }

    let std_before = height_std_dev(next_height);
    let mut adjusted = vec![false; next_height.len()];
    let mut total_abs_correction = 0.0f32;
    let residual = next_height
        .iter()
        .zip(prev_height.iter())
        .map(|(next, prev)| next - prev)
        .sum::<f32>();
    if !residual.is_finite() || residual.abs() <= 1e-6 {
        return ZeroMeanCorrectionStats::default();
    }

    let target_sign = residual.signum();
    let same_sign_delta_sum = next_height
        .iter()
        .zip(prev_height.iter())
        .map(|(next, prev)| next - prev)
        .filter(|delta| delta.is_finite() && delta.signum() == target_sign)
        .map(f32::abs)
        .sum::<f32>();
    if !same_sign_delta_sum.is_finite() || same_sign_delta_sum <= 1e-6 {
        return ZeroMeanCorrectionStats::default();
    }

    let max_weight = weights
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0_f32, f32::max);
    let mut capacities = vec![0.0_f32; next_height.len()];
    let mut capacity_sum = 0.0_f32;
    for (index, (next, prev)) in next_height.iter().zip(prev_height.iter()).enumerate() {
        let delta = *next - *prev;
        if !delta.is_finite() || delta.signum() != target_sign {
            continue;
        }
        let weight = weights.get(index).copied().unwrap_or(0.0);
        let weight_norm = if max_weight > 1e-6 {
            finite_or(weight / max_weight, 0.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let correction_capacity = delta.abs() * (1.0 - 0.90 * weight_norm);
        capacities[index] = correction_capacity;
        capacity_sum += correction_capacity;
    }

    let mut remaining = residual.abs();
    if capacity_sum > 1e-6 {
        let correction_scale = (remaining / capacity_sum).clamp(0.0, 1.0);
        for (index, (next, prev)) in next_height.iter_mut().zip(prev_height.iter()).enumerate() {
            let delta = *next - *prev;
            if !delta.is_finite() || delta.signum() != target_sign {
                continue;
            }
            let correction = capacities[index] * correction_scale;
            if correction <= 0.0 {
                continue;
            }
            *next -= target_sign * correction;
            adjusted[index] = true;
            total_abs_correction += correction;
            remaining -= correction;
        }
    }

    if remaining > 1e-6 {
        let remaining_same_sign_delta_sum = next_height
            .iter()
            .zip(prev_height.iter())
            .map(|(next, prev)| next - prev)
            .filter(|delta| delta.is_finite() && delta.signum() == target_sign)
            .map(f32::abs)
            .sum::<f32>();
        if remaining_same_sign_delta_sum > 1e-6 {
            let correction_fraction = (remaining / remaining_same_sign_delta_sum).clamp(0.0, 1.0);
            for (index, (next, prev)) in next_height.iter_mut().zip(prev_height.iter()).enumerate()
            {
                let delta = *next - *prev;
                if !delta.is_finite() || delta.signum() != target_sign {
                    continue;
                }
                let correction = delta.abs() * correction_fraction;
                if correction <= 0.0 {
                    continue;
                }
                *next -= target_sign * correction;
                adjusted[index] = true;
                total_abs_correction += correction;
            }
        }
    }

    for value in next_height.iter_mut() {
        *value = value.clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
    }

    let adjusted_cells = adjusted.into_iter().filter(|value| *value).count() as f32;
    let denom = prev_height.len().max(1) as f32;
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

fn boundary_tensor(
    boundary_type: BoundaryType,
    activity: f32,
    convergence: f32,
    divergence: f32,
    transform: f32,
    obliquity: f32,
) -> StressTensor {
    let a = activity.clamp(0.0, 1.0);
    let c = convergence.max(a * 0.35).clamp(0.0, 1.0);
    let d = divergence.max(a * 0.35).clamp(0.0, 1.0);
    let t = transform.max(a * 0.35).clamp(0.0, 1.0);
    let oblique_compression = (1.0 - 0.40 * obliquity.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    match boundary_type {
        BoundaryType::Subduction | BoundaryType::Collision => StressTensor {
            xx: -0.09 * c * oblique_compression,
            yy: -0.09 * c * oblique_compression,
            xy: 0.03 * t,
        },
        BoundaryType::Ridge | BoundaryType::Rift => StressTensor {
            xx: 0.07 * d,
            yy: 0.07 * d,
            xy: 0.02 * t,
        },
        BoundaryType::Transform => StressTensor {
            xx: 0.0,
            yy: 0.0,
            xy: 0.08 * t,
        },
        BoundaryType::PassiveMargin => StressTensor {
            xx: 0.0,
            yy: 0.0,
            xy: 0.0,
        },
    }
}

fn boundary_component(values: &[f32], index: usize) -> f32 {
    values
        .get(index)
        .copied()
        .map(|value| finite_or(value, 0.0).clamp(0.0, 1.0))
        .unwrap_or(0.0)
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
    use super::{
        apply_stress_and_surface_update, boundary_surface_forcing,
        enforce_zero_mean_endogenous_height_change, smoothing_limiter, BoundarySurfaceForcingInput,
        SurfaceUpdateInput, SurfaceUpdateOutput,
    };
    use crate::sim::exec::{GEOLOGY_HEIGHT_MAX, GEOLOGY_HEIGHT_MIN};
    use crate::sim::geology_types::{CrustType, PlateId, StressTensor};
    use crate::sim::world::{BoundaryDynamicsState, BoundaryType, VertexCrustState};
    use crate::GeologyParams;

    fn oceanic_vertex(params: &GeologyParams) -> VertexCrustState {
        VertexCrustState {
            crust_type: CrustType::Oceanic,
            thickness: 0.35,
            density: params.oceanic_base_density + params.age_density_gain,
            age: params.age_ref,
            stress: 0.0,
            temperature: 0.5,
            rigidity: 0.46,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: StressTensor::default(),
        }
    }

    fn direct_forcing_input(boundary_type: BoundaryType) -> BoundarySurfaceForcingInput {
        BoundarySurfaceForcingInput {
            boundary_type,
            convergence: 0.0,
            divergence: 0.0,
            obliquity: 0.0,
            subduction_gate: 1.0,
            compressive: 0.0,
            tensile: 0.0,
            volcanism: 0.0,
            rollback_fraction: 0.0,
            slab_rollback: 0.0,
            is_subducting: false,
            is_overriding: false,
        }
    }

    fn run_two_cell_subduction(subducting_plate: PlateId) -> (Vec<f32>, Vec<f32>) {
        let nbr_offsets = vec![0, 0, 0];
        let nbrs = vec![];
        let heights = vec![-0.08, -0.08];
        let plate_id = vec![PlateId(0), PlateId(1)];
        let boundary_state = BoundaryDynamicsState {
            dominant_type: vec![BoundaryType::Subduction; 2],
            activity: vec![0.8; 2],
            convergence_component: vec![1.0; 2],
            subduction_gate: vec![1.0; 2],
            subducting_plate: vec![Some(subducting_plate); 2],
            ..BoundaryDynamicsState::default()
        };
        let mantle_heat = vec![0.4; 2];
        let plume_force = vec![0.0; 2];
        let params = GeologyParams::default();
        let mut next_vertex_states = vec![oceanic_vertex(&params); 2];
        let mut next_height = heights.clone();
        let mut next_volcanism = vec![0.0; 2];
        let mut next_vertex_buoyancy = vec![0.0; 2];
        let mut output = SurfaceUpdateOutput {
            next_vertex_states: &mut next_vertex_states,
            next_height: &mut next_height,
            next_volcanism: &mut next_volcanism,
            next_vertex_buoyancy: &mut next_vertex_buoyancy,
        };

        let _ = apply_stress_and_surface_update(
            SurfaceUpdateInput {
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                heights: &heights,
                plate_id: &plate_id,
                boundary_state: &boundary_state,
                mantle_heat: &mantle_heat,
                plume_force: &plume_force,
                activity_scale: 1.0,
                params: &params,
            },
            &mut output,
        );

        (next_height, next_volcanism)
    }

    #[test]
    fn subduction_forcing_separates_subducting_and_overriding_sides() {
        let params = GeologyParams::default();
        let subducting = boundary_surface_forcing(
            BoundarySurfaceForcingInput {
                convergence: 1.0,
                is_subducting: true,
                ..direct_forcing_input(BoundaryType::Subduction)
            },
            &params,
        );
        let overriding = boundary_surface_forcing(
            BoundarySurfaceForcingInput {
                convergence: 1.0,
                volcanism: 0.5,
                is_overriding: true,
                ..direct_forcing_input(BoundaryType::Subduction)
            },
            &params,
        );

        assert!(subducting.tectonic_subsidence > 0.0);
        assert_eq!(subducting.tectonic_uplift, 0.0);
        assert_eq!(subducting.volcanic_uplift, 0.0);
        assert_eq!(overriding.tectonic_subsidence, 0.0);
        assert!(overriding.tectonic_uplift > 0.0);
        assert!(overriding.volcanic_uplift > 0.0);
    }

    #[test]
    fn artificial_subduction_boundary_places_relief_on_the_intended_sides() {
        let (height, volcanism) = run_two_cell_subduction(PlateId(0));

        assert!(height[0] < height[1]);
        assert_eq!(volcanism[0], 0.0);
        assert!(volcanism[1] > 0.0);
    }

    #[test]
    fn mirrored_subduction_boundary_mirrors_the_surface_response() {
        let (forward_height, forward_volcanism) = run_two_cell_subduction(PlateId(0));
        let (mirrored_height, mirrored_volcanism) = run_two_cell_subduction(PlateId(1));

        assert!((forward_height[0] - mirrored_height[1]).abs() <= 1e-6);
        assert!((forward_height[1] - mirrored_height[0]).abs() <= 1e-6);
        assert!((forward_volcanism[0] - mirrored_volcanism[1]).abs() <= 1e-6);
        assert!((forward_volcanism[1] - mirrored_volcanism[0]).abs() <= 1e-6);
    }

    #[test]
    fn normal_boundary_forcing_is_monotonic_with_normal_speed() {
        let params = GeologyParams::default();
        let forcing = |convergence| {
            boundary_surface_forcing(
                BoundarySurfaceForcingInput {
                    convergence,
                    is_subducting: true,
                    ..direct_forcing_input(BoundaryType::Subduction)
                },
                &params,
            )
            .tectonic_subsidence
        };

        assert_eq!(forcing(0.0), 0.0);
        assert!(forcing(0.5) > forcing(0.25));
        assert!(forcing(1.0) > forcing(0.5));
    }

    #[test]
    fn divergent_and_transform_forcing_remain_distinct() {
        let params = GeologyParams::default();
        let ridge = boundary_surface_forcing(
            BoundarySurfaceForcingInput {
                divergence: 1.0,
                ..direct_forcing_input(BoundaryType::Ridge)
            },
            &params,
        );
        let rift = boundary_surface_forcing(
            BoundarySurfaceForcingInput {
                divergence: 1.0,
                ..direct_forcing_input(BoundaryType::Rift)
            },
            &params,
        );
        let transform =
            boundary_surface_forcing(direct_forcing_input(BoundaryType::Transform), &params);

        assert!(ridge.tectonic_uplift > 0.0);
        assert_eq!(ridge.tectonic_subsidence, 0.0);
        assert_eq!(rift.tectonic_uplift, 0.0);
        assert!(rift.tectonic_subsidence > 0.0);
        assert_eq!(transform.tectonic_uplift, 0.0);
        assert_eq!(transform.tectonic_subsidence, 0.0);
        assert_eq!(transform.volcanic_uplift, 0.0);
    }

    #[test]
    fn obliquity_does_not_increase_convergence_specific_forcing() {
        let params = GeologyParams::default();
        let forcing = |obliquity| {
            boundary_surface_forcing(
                BoundarySurfaceForcingInput {
                    convergence: 1.0,
                    obliquity,
                    ..direct_forcing_input(BoundaryType::Collision)
                },
                &params,
            )
            .tectonic_uplift
        };

        assert!(forcing(0.5) <= forcing(0.0));
        assert!(forcing(1.0) <= forcing(0.5));
    }

    #[test]
    fn endogenous_height_change_preserves_global_mean() {
        let prev = vec![0.10, -0.20, 0.05, -0.05];
        let mut next = vec![0.18, -0.10, 0.12, 0.02];
        let weights = vec![1.0, 1.0, 1.0, 1.0];

        let stats = enforce_zero_mean_endogenous_height_change(&prev, &mut next, &weights);

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
        let prev = vec![1.15, 1.10, -1.15, -1.10];
        let mut next = vec![GEOLOGY_HEIGHT_MAX, GEOLOGY_HEIGHT_MAX, -0.70, -0.60];
        let weights = vec![1.0, 1.0, 1.0, 1.0];

        let stats = enforce_zero_mean_endogenous_height_change(&prev, &mut next, &weights);

        assert!(next
            .iter()
            .all(|value| (GEOLOGY_HEIGHT_MIN..=GEOLOGY_HEIGHT_MAX).contains(value)));
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
    fn endogenous_height_change_damps_same_sign_delta_instead_of_creating_boundary_uplift() {
        let prev = vec![0.0, 0.0, 0.0, 0.0];
        let mut next = vec![-0.12, -0.06, 0.03, 0.03];
        let boundary_focused_weights = vec![0.0, 0.0, 0.0, 100.0];

        let stats =
            enforce_zero_mean_endogenous_height_change(&prev, &mut next, &boundary_focused_weights);

        let mean_delta = next
            .iter()
            .zip(prev.iter())
            .map(|(next, prev)| next - prev)
            .sum::<f32>()
            / prev.len() as f32;
        assert!(mean_delta.abs() <= 1e-6);
        assert!(next[0] > -0.12);
        assert!(next[1] > -0.06);
        assert!((next[2] - 0.03).abs() <= 1e-6);
        assert!((next[3] - 0.03).abs() <= 1e-6);
        assert!(stats.adjusted_cells_ratio > 0.0);
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

    #[test]
    fn activity_scale_zero_quenches_carried_stress_memory() {
        let nbr_offsets = vec![0, 0];
        let nbrs = vec![];
        let heights = vec![-0.98];
        let plate_id = vec![PlateId(0)];
        let boundary_state = BoundaryDynamicsState {
            dominant_type: vec![BoundaryType::PassiveMargin],
            activity: vec![0.0],
            ..BoundaryDynamicsState::default()
        };
        let mantle_heat = vec![0.5];
        let plume_force = vec![0.0];
        let params = GeologyParams::default();

        let mut next_vertex_states = vec![VertexCrustState {
            crust_type: CrustType::Oceanic,
            thickness: 0.30,
            density: params.oceanic_base_density,
            age: params.age_ref,
            stress: 8.0814184e36,
            temperature: 0.5,
            rigidity: 0.46,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: StressTensor {
                xx: 8.0814184e36,
                yy: 8.0814184e36,
                xy: 0.0,
            },
        }];
        let mut next_height = heights.clone();
        let mut next_volcanism = vec![0.0];
        let mut next_vertex_buoyancy = vec![0.0];
        let mut output = SurfaceUpdateOutput {
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
                plate_id: &plate_id,
                boundary_state: &boundary_state,
                mantle_heat: &mantle_heat,
                plume_force: &plume_force,
                activity_scale: 0.0,
                params: &params,
            },
            &mut output,
        );

        assert_eq!(metrics.debug_surface_max_delta_stress, 0.0);
        assert_eq!(metrics.debug_surface_max_delta_tensile, 0.0);
        assert_eq!(metrics.debug_surface_max_delta_tectonic_subsidence, 0.0);
        assert_eq!(metrics.mean_abs_surface_step_delta, 0.0);
        assert_eq!(next_height, heights);
    }

    #[test]
    fn oceanic_thermal_subsidence_contributes_to_surface_forcing() {
        let nbr_offsets = vec![0, 0];
        let nbrs = vec![];
        let heights = vec![-0.12];
        let plate_id = vec![PlateId(0)];
        let boundary_state = BoundaryDynamicsState {
            dominant_type: vec![BoundaryType::PassiveMargin],
            activity: vec![0.0],
            ..BoundaryDynamicsState::default()
        };
        let mantle_heat = vec![0.4];
        let plume_force = vec![0.0];
        let params = GeologyParams::default();
        let mut next_vertex_states = vec![oceanic_vertex(&params)];
        let mut next_height = heights.clone();
        let mut next_volcanism = vec![0.0];
        let mut next_vertex_buoyancy = vec![0.0];
        let mut output = SurfaceUpdateOutput {
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
                plate_id: &plate_id,
                boundary_state: &boundary_state,
                mantle_heat: &mantle_heat,
                plume_force: &plume_force,
                activity_scale: 1.0,
                params: &params,
            },
            &mut output,
        );

        assert!(metrics.mean_abs_thermal_subsidence > 0.0);
        assert!(metrics.mean_abs_surface_raw_delta > 0.0);
    }

    #[test]
    fn slab_component_strengthens_subduction_arc_volcanism() {
        let nbr_offsets = vec![0, 0];
        let nbrs = vec![];
        let heights = vec![-0.08];
        let plate_id = vec![PlateId(0)];
        let mantle_heat = vec![0.4];
        let plume_force = vec![0.0];
        let params = GeologyParams::default();

        let run = |slab_convergence: f32| {
            let boundary_state = BoundaryDynamicsState {
                dominant_type: vec![BoundaryType::Subduction],
                activity: vec![0.1],
                subducting_plate: vec![Some(PlateId(1))],
                slab_convergence_component: vec![slab_convergence],
                slab_rollback_component: vec![0.0],
                ..BoundaryDynamicsState::default()
            };
            let mut next_vertex_states = vec![oceanic_vertex(&params)];
            let mut next_height = heights.clone();
            let mut next_volcanism = vec![0.0];
            let mut next_vertex_buoyancy = vec![0.0];
            let mut output = SurfaceUpdateOutput {
                next_vertex_states: &mut next_vertex_states,
                next_height: &mut next_height,
                next_volcanism: &mut next_volcanism,
                next_vertex_buoyancy: &mut next_vertex_buoyancy,
            };
            let _ = apply_stress_and_surface_update(
                SurfaceUpdateInput {
                    nbr_offsets: &nbr_offsets,
                    nbrs: &nbrs,
                    heights: &heights,
                    plate_id: &plate_id,
                    boundary_state: &boundary_state,
                    mantle_heat: &mantle_heat,
                    plume_force: &plume_force,
                    activity_scale: 1.0,
                    params: &params,
                },
                &mut output,
            );
            next_volcanism[0]
        };

        assert!(run(0.6) > run(0.0));
    }
}
