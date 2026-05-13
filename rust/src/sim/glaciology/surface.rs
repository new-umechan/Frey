use super::types::GlaciologyParams;
use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::world::{EraKind, World};

const RELIEF_NORMALIZER: f32 = 0.08;
const SOLID_PRECIP_ALL_SNOW_C: f32 = 0.0;
const SOLID_PRECIP_ALL_RAIN_C: f32 = 2.0;

pub(crate) fn run_glaciology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let params = GlaciologyParams::default();
    if world.clock.epoch == EraKind::Crust {
        return;
    }

    let spinup = environment_spinup_factor(world, &params);
    let alpha = blend_alpha(
        budget,
        (params.thickness_response_rate * spinup).clamp(0.0, 0.95),
    );
    let prev_ice_inventory = world.control.ice_inventory.max(0.0);
    let cell_count = world.state.geology.height.len();
    ensure_state_len(world, cell_count);

    let heights = world.state.geology.height.clone();
    let temperatures = world.state.climate.temperature.clone();
    let precipitation = world.state.climate.precipitation.clone();
    let nbr_offsets = world.mesh().nbr_offsets.clone();
    let nbrs = world.mesh().nbrs.clone();
    let prev_ice_thickness = world.state.glaciology.ice_thickness.clone();
    let prev_isostatic_adjustment = world.state.glaciology.isostatic_adjustment.clone();
    let mut accumulation_potential = vec![0.0f32; cell_count];
    let mut ablation_potential = vec![0.0f32; cell_count];
    let mut ice_candidate = vec![0.0f32; cell_count];
    let mut isostatic_candidate = vec![0.0f32; cell_count];
    let mut glacial_erosion_candidate = vec![0.0f32; cell_count];
    let mut total_ice_candidate = 0.0f32;
    let mut total_accum_potential = 0.0f32;
    let mut total_ablation_potential = 0.0f32;
    for i in 0..cell_count {
        let temp_c = temperatures.get(i).copied().unwrap_or(0.0);
        let precip_mm = precipitation.get(i).copied().unwrap_or(0.0).max(0.0);
        let prev_ice = prev_ice_thickness.get(i).copied().unwrap_or(0.0).max(0.0);
        let relief = local_relief(i, &heights, &nbr_offsets, &nbrs);

        // PDD系: solid precipitation fraction (0C..2Cで線形遷移)。
        let solid_frac = if temp_c <= SOLID_PRECIP_ALL_SNOW_C {
            1.0
        } else if temp_c >= SOLID_PRECIP_ALL_RAIN_C {
            0.0
        } else {
            (SOLID_PRECIP_ALL_RAIN_C - temp_c)
                / (SOLID_PRECIP_ALL_RAIN_C - SOLID_PRECIP_ALL_SNOW_C)
        };
        let accumulation_target =
            precip_mm * params.accumulation_gain.max(0.0) * solid_frac * spinup;
        let pdd = (temp_c - params.ablation_temp_threshold_c).max(0.0);
        let ablation_target = pdd * params.ablation_gain.max(0.0) * spinup;
        accumulation_potential[i] = accumulation_target.max(0.0);
        ablation_potential[i] = ablation_target.max(0.0);
        total_accum_potential += accumulation_potential[i];
        total_ablation_potential += ablation_potential[i];

        let next_raw = (prev_ice + accumulation_potential[i] - ablation_potential[i]).max(0.0);
        let candidate = lerp(prev_ice, next_raw, alpha).max(0.0);
        ice_candidate[i] = candidate;
        total_ice_candidate += candidate;

        let target_isostatic_adjustment =
            -candidate * params.ice_load_to_bedrock_coupling.max(0.0);
        let isostatic_alpha =
            blend_alpha(budget, (params.isostatic_adjustment_rate * spinup).clamp(0.0, 0.95));
        isostatic_candidate[i] = lerp(
            prev_isostatic_adjustment.get(i).copied().unwrap_or(0.0),
            target_isostatic_adjustment,
            isostatic_alpha,
        );
        glacial_erosion_candidate[i] = candidate * relief * params.erosion_gain.max(0.0);
    }

    let exchange_alpha = tau_to_alpha(params.ice_ocean_coupling_tau_ticks.max(1.0)) * spinup;
    let target_ice_inventory = apply_ice_ocean_mass_transfer(
        world,
        &params,
        prev_ice_inventory,
        total_ice_candidate,
        exchange_alpha,
    );
    let scale = if total_ice_candidate > 1e-6 {
        target_ice_inventory / total_ice_candidate
    } else {
        0.0
    };
    let state = &mut world.state.glaciology;
    for i in 0..cell_count {
        state.ice_thickness[i] = ice_candidate[i] * scale;
        state.ice_load[i] = state.ice_thickness[i];
        state.isostatic_adjustment[i] = isostatic_candidate[i];
        state.glacial_erosion_rate[i] = glacial_erosion_candidate[i] * scale;
        state.accumulation[i] = if total_accum_potential > 1e-6 {
            accumulation_potential[i] * ((target_ice_inventory - prev_ice_inventory).max(0.0) / total_accum_potential).clamp(0.0, 1.0)
        } else {
            0.0
        };
        state.ablation[i] = if total_ablation_potential > 1e-6 {
            ablation_potential[i] * ((prev_ice_inventory - target_ice_inventory).max(0.0) / total_ablation_potential).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let melt_source = state.ablation[i].max(0.0);
        state.glacial_melt_runoff[i] = melt_source * params.melt_runoff_gain.max(0.0);
    }
    world.control.ice_inventory = target_ice_inventory;
    update_sea_level_offset_from_inventory(world, &params);
}

fn apply_ice_ocean_mass_transfer(
    world: &mut World,
    params: &GlaciologyParams,
    prev_ice_inventory: f32,
    next_ice_inventory: f32,
    exchange_alpha: f32,
) -> f32 {
    let coupling = params.sea_level_coupling.max(0.0);
    if coupling <= 0.0 {
        return next_ice_inventory.max(0.0);
    }
    // Preserve the pre-step combined proxy mass for this tick.
    let target_mass_proxy =
        world.control.ocean_water_inventory.max(0.0) + coupling * prev_ice_inventory.max(0.0);
    let available_ocean_ice_equiv = (world.control.ocean_water_inventory.max(0.0) / coupling).max(0.0);
    let max_growth = available_ocean_ice_equiv * exchange_alpha;
    let max_melt = prev_ice_inventory.max(0.0) * exchange_alpha;
    let raw_delta = next_ice_inventory.max(0.0) - prev_ice_inventory.max(0.0);
    let limited_delta = raw_delta.clamp(-max_melt, max_growth);
    let desired_ice = (prev_ice_inventory.max(0.0) + limited_delta).max(0.0);
    let max_supported_ice = (target_mass_proxy / coupling).max(0.0);
    let capped_ice = desired_ice.min(max_supported_ice);
    let desired_ocean = (target_mass_proxy - coupling * capped_ice).max(0.0);
    let delta_ice = capped_ice - prev_ice_inventory;
    if delta_ice.abs() <= params.mass_conservation_epsilon.max(0.0) {
        world.control.ocean_water_inventory = desired_ocean;
        return capped_ice;
    }
    world.control.ocean_water_inventory = desired_ocean;
    capped_ice
}

fn update_sea_level_offset_from_inventory(world: &mut World, params: &GlaciologyParams) {
    let spinup = environment_spinup_factor(world, params);
    let coupling = effective_ice_ocean_coupling(world, params, spinup);
    let target_water_inventory =
        effective_ocean_water_inventory(world.control.ocean_water_inventory, coupling);
    let current_offset = world.control.sea_level_offset;
    let current_inventory = sea_water_inventory_at_offset(&world.state.geology.height, current_offset);
    let target_offset = solve_sea_level_for_inventory(
        &world.state.geology.height,
        target_water_inventory,
        current_offset,
    );
    let effective_basin_area = mean_basin_area_between_offsets(
        &world.state.geology.height,
        current_offset,
        target_offset,
    )
    .max(1.0);
    let inventory_residual = target_water_inventory - current_inventory;
    let semi_implicit_target = current_offset + inventory_residual / effective_basin_area;
    let lower = current_offset.min(target_offset);
    let upper = current_offset.max(target_offset);
    let bounded_target = semi_implicit_target.clamp(lower, upper);
    let tau = effective_sea_level_tau_ticks(
        params.sea_level_relaxation_tau_ticks,
        effective_basin_area,
        world.state.geology.height.len(),
    );
    let alpha = tau_to_alpha(tau);
    world.control.sea_level_offset = lerp(
        current_offset,
        bounded_target,
        alpha,
    );
}

fn ensure_state_len(world: &mut World, cell_count: usize) {
    let state = &mut world.state.glaciology;
    if state.ice_thickness.len() != cell_count {
        state.ice_thickness.resize(cell_count, 0.0);
    }
    if state.ice_load.len() != cell_count {
        state.ice_load.resize(cell_count, 0.0);
    }
    if state.accumulation.len() != cell_count {
        state.accumulation.resize(cell_count, 0.0);
    }
    if state.ablation.len() != cell_count {
        state.ablation.resize(cell_count, 0.0);
    }
    if state.isostatic_adjustment.len() != cell_count {
        state.isostatic_adjustment.resize(cell_count, 0.0);
    }
    if state.applied_isostatic_adjustment.len() != cell_count {
        state.applied_isostatic_adjustment.resize(cell_count, 0.0);
    }
    if state.glacial_erosion_rate.len() != cell_count {
        state.glacial_erosion_rate.resize(cell_count, 0.0);
    }
    if state.glacial_melt_runoff.len() != cell_count {
        state.glacial_melt_runoff.resize(cell_count, 0.0);
    }
}

fn local_relief(i: usize, height: &[f32], nbr_offsets: &[u32], nbrs: &[u32]) -> f32 {
    if i + 1 >= nbr_offsets.len() || i >= height.len() {
        return 0.0;
    }
    let start = nbr_offsets[i] as usize;
    let end = nbr_offsets[i + 1] as usize;
    if end <= start || end > nbrs.len() {
        return 0.0;
    }
    let h = height[i];
    let mut sum = 0.0f32;
    let mut count = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        if n >= height.len() {
            continue;
        }
        sum += (h - height[n]).abs();
        count += 1.0;
    }
    if count <= 0.0 {
        return 0.0;
    }
    (sum / count / RELIEF_NORMALIZER).clamp(0.0, 1.0)
}

fn solve_sea_level_for_inventory(
    heights: &[f32],
    target_inventory: f32,
    fallback_offset: f32,
) -> f32 {
    if heights.is_empty() {
        return fallback_offset;
    }
    let mut min_h = f32::INFINITY;
    let mut max_h = f32::NEG_INFINITY;
    for h in heights.iter().copied() {
        min_h = min_h.min(h);
        max_h = max_h.max(h);
    }
    if !min_h.is_finite() || !max_h.is_finite() {
        return fallback_offset;
    }
    let mut lo = min_h - 1.0;
    let mut hi = max_h + 1.0;
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        let inventory = sea_water_inventory_at_offset(heights, mid);
        if inventory < target_inventory {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn effective_ocean_water_inventory(
    ocean_water_inventory: f32,
    coupling: f32,
) -> f32 {
    if coupling <= 0.0 {
        return ocean_water_inventory.max(0.0);
    }
    // Ocean inventory already includes ice-ocean mass transfer in this runtime.
    ocean_water_inventory.max(0.0)
}

fn environment_spinup_factor(world: &World, params: &GlaciologyParams) -> f32 {
    if world.clock.epoch != EraKind::Environment {
        return 1.0;
    }
    let elapsed_ticks = world
        .clock
        .tick
        .saturating_sub(world.clock.transition.era_enter_tick) as f32;
    let window = params.environment_spinup_ticks.max(1) as f32;
    (elapsed_ticks / window).clamp(0.0, 1.0)
}

fn effective_ice_ocean_coupling(world: &World, params: &GlaciologyParams, spinup: f32) -> f32 {
    if world.clock.epoch != EraKind::Environment {
        return params.sea_level_coupling.max(0.0);
    }
    let elapsed_ticks = world
        .clock
        .tick
        .saturating_sub(world.clock.transition.era_enter_tick) as f32;
    let tau = params.ice_ocean_coupling_tau_ticks.max(1.0);
    let coupling_ramp = 1.0 - (-elapsed_ticks / tau).exp();
    let phase = spinup.min(coupling_ramp);
    params.sea_level_coupling.max(0.0) * phase
}

fn tau_to_alpha(tau_ticks: f32) -> f32 {
    let tau = tau_ticks.max(1.0);
    (1.0 - (-1.0 / tau).exp()).clamp(0.0, 1.0)
}

fn effective_sea_level_tau_ticks(
    base_tau_ticks: f32,
    effective_basin_area: f32,
    cell_count: usize,
) -> f32 {
    let base_tau = base_tau_ticks.max(1.0);
    if cell_count == 0 {
        return base_tau;
    }
    let area_fraction = (effective_basin_area / cell_count as f32)
        .clamp(1.0 / cell_count as f32, 1.0);
    base_tau / area_fraction.sqrt()
}

fn sea_water_inventory_at_offset(heights: &[f32], sea_level_offset: f32) -> f32 {
    heights
        .iter()
        .copied()
        .map(|h| (sea_level_offset - h).max(0.0))
        .sum()
}

fn sea_basin_area_at_offset(heights: &[f32], sea_level_offset: f32) -> f32 {
    heights
        .iter()
        .copied()
        .filter(|h| *h < sea_level_offset)
        .count() as f32
}

fn mean_basin_area_between_offsets(
    heights: &[f32],
    current_offset: f32,
    target_offset: f32,
) -> f32 {
    let current_area = sea_basin_area_at_offset(heights, current_offset);
    let target_area = sea_basin_area_at_offset(heights, target_offset);
    0.5 * (current_area + target_area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::{EraKind, GeologyState, World, WorldMesh};
    use crate::PlateId;

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                [0.0, 0.8, 0.6],
                [0.7, 0.2, 0.6],
                [0.4, -0.7, 0.6],
                [-0.6, -0.1, 0.8],
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let geology = GeologyState {
            height: vec![0.45, 0.15, -0.25, 0.05],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
            smoothing_limited_cells_ratio: 0.0,
            mean_smoothing_factor: 1.0,
            zero_mean_adjusted_cells_ratio: 0.0,
            zero_mean_mean_abs_correction: 0.0,
            zero_mean_std_delta: 0.0,
        };
        World::new(mesh, geology)
    }

    #[test]
    fn cold_wet_cells_increase_ice_thickness() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Environment;
        world.state.climate.temperature = vec![-8.0, -6.0, -10.0, -7.0];
        world.state.climate.precipitation = vec![1800.0, 1400.0, 1200.0, 1000.0];

        run_glaciology_step(&mut world, 1);

        assert!(world
            .state
            .glaciology
            .ice_thickness
            .iter()
            .any(|v| *v > 0.0));
    }

    #[test]
    fn warm_cells_generate_melt_runoff() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Environment;
        world.state.glaciology.ice_thickness = vec![12.0, 8.0, 5.0, 3.0];
        world.control.ice_inventory = 28.0;
        world.state.climate.temperature = vec![5.0, 7.0, 6.0, 4.0];
        world.state.climate.precipitation = vec![400.0, 300.0, 200.0, 150.0];

        run_glaciology_step(&mut world, 1);

        assert!(world
            .state
            .glaciology
            .glacial_melt_runoff
            .iter()
            .any(|v| *v > 0.0));
    }

    #[test]
    fn crust_era_skips_glaciology_and_sea_level_update() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Crust;
        world.control.sea_level_offset = 0.07;
        world.state.climate.temperature = vec![-12.0, -11.0, -10.0, -9.0];
        world.state.climate.precipitation = vec![2000.0, 1800.0, 1600.0, 1400.0];

        run_glaciology_step(&mut world, 1);

        assert!((world.control.sea_level_offset - 0.07).abs() < 1e-6);
        assert!(world
            .state
            .glaciology
            .ice_thickness
            .iter()
            .all(|value| value.abs() <= f32::EPSILON));
    }

    #[test]
    fn cold_growth_updates_sea_level_and_ice_load() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Environment;
        world.control.sea_level_offset = 0.02;
        world.state.climate.temperature = vec![-12.0, -11.0, -10.0, -9.0];
        world.state.climate.precipitation = vec![2000.0, 1800.0, 1600.0, 1400.0];

        run_glaciology_step(&mut world, 1);

        assert!(world.control.sea_level_offset.is_finite());
        assert!((world.control.sea_level_offset - 0.02).abs() > 1e-6);
        assert!(world.state.glaciology.ice_load.iter().any(|v| *v > 0.0));
        assert!(world
            .state
            .glaciology
            .isostatic_adjustment
            .iter()
            .any(|v| *v < 0.0));
    }

    #[test]
    fn sea_basin_area_grows_monotonically_with_offset() {
        let heights = vec![-0.3, -0.1, 0.05, 0.2];
        let low = sea_basin_area_at_offset(&heights, -0.2);
        let mid = sea_basin_area_at_offset(&heights, 0.0);
        let high = sea_basin_area_at_offset(&heights, 0.3);

        assert!(low <= mid);
        assert!(mid <= high);
    }
}
