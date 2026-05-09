use super::types::GlaciologyParams;
use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::world::{EraKind, World};

const RELIEF_NORMALIZER: f32 = 0.08;

pub(crate) fn run_glaciology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let params = GlaciologyParams::default();
    if world.clock.epoch == EraKind::Crust {
        return;
    }

    let alpha = blend_alpha(budget, params.thickness_response_rate.clamp(0.01, 0.95));
    let prev_ice_inventory = world.control.ice_inventory.max(0.0);
    let cell_count = world.state.geology.height.len();
    ensure_state_len(world, cell_count);

    let heights = world.state.geology.height.clone();
    let temperatures = world.state.climate.temperature.clone();
    let precipitation = world.state.climate.precipitation.clone();
    let nbr_offsets = world.mesh().nbr_offsets.clone();
    let nbrs = world.mesh().nbrs.clone();

    let state = &mut world.state.glaciology;
    let mut total_ice = 0.0f32;
    for i in 0..cell_count {
        let temp_c = temperatures.get(i).copied().unwrap_or(0.0);
        let precip_mm = precipitation.get(i).copied().unwrap_or(0.0).max(0.0);
        let prev_ice = state.ice_thickness[i].max(0.0);
        let relief = local_relief(i, &heights, &nbr_offsets, &nbrs);

        let cold_excess = (params.accum_temp_threshold_c - temp_c).max(0.0);
        let warm_excess = (temp_c - params.ablation_temp_threshold_c).max(0.0);

        let accumulation_target = precip_mm
            * params.accumulation_gain.max(0.0)
            * (1.0 + cold_excess * params.accumulation_temp_sensitivity.max(0.0));
        let ablation_target = warm_excess * params.ablation_gain.max(0.0) * (1.0 + relief * 0.35);

        state.accumulation[i] = accumulation_target.max(0.0);
        state.ablation[i] = ablation_target.max(0.0);

        let next_raw = (prev_ice + state.accumulation[i] - state.ablation[i]).max(0.0);
        state.ice_thickness[i] = lerp(prev_ice, next_raw, alpha).max(0.0);
        state.ice_load[i] = state.ice_thickness[i];
        let target_isostatic_adjustment =
            -state.ice_load[i] * params.ice_load_to_bedrock_coupling.max(0.0);
        let isostatic_alpha =
            blend_alpha(budget, params.isostatic_adjustment_rate.clamp(0.01, 0.95));
        state.isostatic_adjustment[i] = lerp(
            state.isostatic_adjustment[i],
            target_isostatic_adjustment,
            isostatic_alpha,
        );
        total_ice += state.ice_thickness[i];

        let melt_source = (state.ablation[i] - state.accumulation[i]).max(0.0);
        state.glacial_melt_runoff[i] = melt_source * params.melt_runoff_gain.max(0.0);
        state.glacial_erosion_rate[i] =
            state.ice_thickness[i] * relief * params.erosion_gain.max(0.0);
    }
    let total_ice = apply_ice_ocean_mass_transfer(world, &params, prev_ice_inventory, total_ice);
    world.control.ice_inventory = total_ice;
    update_sea_level_offset_from_inventory(world, &params);
}

fn apply_ice_ocean_mass_transfer(
    world: &mut World,
    params: &GlaciologyParams,
    prev_ice_inventory: f32,
    next_ice_inventory: f32,
) -> f32 {
    let coupling = params.sea_level_coupling.max(0.0);
    if coupling <= 0.0 {
        return next_ice_inventory.max(0.0);
    }
    // Preserve the pre-step combined proxy mass for this tick.
    let target_mass_proxy =
        world.control.ocean_water_inventory.max(0.0) + coupling * prev_ice_inventory.max(0.0);
    let max_supported_ice = (target_mass_proxy / coupling).max(0.0);
    let capped_ice = next_ice_inventory.max(0.0).min(max_supported_ice);
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
    let target_offset = solve_sea_level_for_inventory(
        &world.state.geology.height,
        target_water_inventory,
        world.control.sea_level_offset,
    );
    let alpha = tau_to_alpha(params.sea_level_relaxation_tau_ticks.max(1.0));
    world.control.sea_level_offset = lerp(
        world.control.sea_level_offset,
        target_offset,
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

fn sea_water_inventory_at_offset(heights: &[f32], sea_level_offset: f32) -> f32 {
    heights
        .iter()
        .copied()
        .map(|h| (sea_level_offset - h).max(0.0))
        .sum()
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
}
