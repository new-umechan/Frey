use crate::sim::exec::{GEOLOGY_HEIGHT_MAX, GEOLOGY_HEIGHT_MIN, HYDROLOGY_MFD_ACTIVITY_THRESHOLD};
use crate::sim::glaciology::types::GlaciologyParams;
use crate::sim::hydrology::{
    run_hydrology_flow_step, run_hydrology_step, sync_erosion_height, HydrologyStepDetailBreakdown,
};
use crate::sim::state::erosion::ErosionAutomatonState;
use crate::sim::world::{EraKind, World};

const CRUST_COASTAL_BAND: f32 = 0.02;
const CRUST_COASTAL_BAND_TARGET_RATIO: f32 = 0.12;
const CRUST_FREEBOARD_INFLATION_TRIGGER_BAND: f32 = 0.20;
const CRUST_FREEBOARD_TARGET_P50: f32 = 0.03;
const CRUST_SHORELINE_EXPANSION_RANGE: f32 = 0.05;

pub(super) fn run_geology_step_with_state(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    budget: u32,
) {
    crate::sim::geology::update_geology(world, geology_state, budget);
    let recenter_stats = preserve_crust_freeboard(world);
    if let Some(state) = geology_state.as_mut() {
        state.cached_metrics.crust_recentering_shift = recenter_stats.shift;
        state.cached_metrics.crust_recentering_pre_band_ratio = recenter_stats.pre_band_ratio;
        state.cached_metrics.crust_recentering_post_band_ratio = recenter_stats.post_band_ratio;
        state.cached_metrics.bedrock_zero_level_coastal_band_ratio = recenter_stats.post_band_ratio;
    }
}

#[derive(Clone, Copy, Default)]
struct CrustRecenteringStats {
    shift: f32,
    pre_band_ratio: f32,
    post_band_ratio: f32,
}

fn preserve_crust_freeboard(world: &mut World) -> CrustRecenteringStats {
    if world.clock.epoch != EraKind::Crust {
        return CrustRecenteringStats::default();
    }

    let target_land_ratio = world.clock.transition.last_land_ratio.clamp(0.05, 0.95);
    let height = &mut world.state.geology.height;
    if height.is_empty() {
        return CrustRecenteringStats::default();
    }
    let pre_band_ratio = coastal_band_ratio(height, CRUST_COASTAL_BAND);

    let mut sorted = height
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if sorted.is_empty() {
        return CrustRecenteringStats::default();
    }
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let target_sea_ratio = 1.0 - target_land_ratio;
    let sea_idx = ((sorted.len() as f32) * target_sea_ratio).floor() as usize;
    let sea_level = sorted[sea_idx.min(sorted.len().saturating_sub(1))];
    if !sea_level.is_finite() || sea_level.abs() <= 1e-6 {
        return CrustRecenteringStats {
            shift: 0.0,
            pre_band_ratio,
            post_band_ratio: pre_band_ratio,
        };
    }

    for value in height.iter_mut() {
        *value = (*value - sea_level).clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
    }
    if pre_band_ratio > CRUST_FREEBOARD_INFLATION_TRIGGER_BAND {
        inflate_crust_freeboard(height);
    }
    let mut post_band_ratio = coastal_band_ratio(height, CRUST_COASTAL_BAND);
    if post_band_ratio > CRUST_COASTAL_BAND_TARGET_RATIO {
        expand_shoreline_freeboard(height);
        post_band_ratio = coastal_band_ratio(height, CRUST_COASTAL_BAND);
    }
    CrustRecenteringStats {
        shift: sea_level,
        pre_band_ratio,
        post_band_ratio,
    }
}

fn inflate_crust_freeboard(heights: &mut [f32]) {
    let land_p50 = signed_abs_percentile(heights, true, 0.50);
    let ocean_p50 = signed_abs_percentile(heights, false, 0.50);
    if land_p50 >= CRUST_FREEBOARD_TARGET_P50 && ocean_p50 >= CRUST_FREEBOARD_TARGET_P50 {
        return;
    }

    for value in heights.iter_mut() {
        let abs_height = value.abs();
        if abs_height <= f32::EPSILON || abs_height >= CRUST_FREEBOARD_TARGET_P50 {
            continue;
        }
        let inflated = remap_low_freeboard(abs_height, CRUST_FREEBOARD_TARGET_P50);
        *value = value.signum() * inflated;
    }
}

fn expand_shoreline_freeboard(heights: &mut [f32]) {
    for value in heights.iter_mut() {
        let abs_height = value.abs();
        if abs_height <= f32::EPSILON || abs_height > CRUST_SHORELINE_EXPANSION_RANGE {
            continue;
        }
        let expanded = remap_shoreline_freeboard(abs_height);
        *value = value.signum() * expanded;
    }
}

fn remap_shoreline_freeboard(abs_height: f32) -> f32 {
    let normalized = (abs_height / CRUST_SHORELINE_EXPANSION_RANGE).clamp(0.0, 1.0);
    let pushed = normalized.sqrt();
    CRUST_COASTAL_BAND
        + (CRUST_SHORELINE_EXPANSION_RANGE - CRUST_COASTAL_BAND) * pushed
}

fn remap_low_freeboard(abs_height: f32, target: f32) -> f32 {
    let normalized = (abs_height / target).clamp(0.0, 1.0);
    target * normalized.sqrt()
}

fn signed_abs_percentile(heights: &[f32], positive: bool, quantile: f32) -> f32 {
    let mut values = heights
        .iter()
        .copied()
        .filter(|height| if positive { *height > 0.0 } else { *height < 0.0 })
        .map(|height| height.abs())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    percentile_sorted(&values, quantile)
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

fn coastal_band_ratio(heights: &[f32], band: f32) -> f32 {
    if heights.is_empty() {
        return 0.0;
    }
    let in_band = heights
        .iter()
        .filter(|&&height| height.abs() <= band)
        .count();
    in_band as f32 / heights.len() as f32
}

#[cfg(test)]
mod tests {
    use super::{
        coastal_band_ratio, expand_shoreline_freeboard, inflate_crust_freeboard, percentile_sorted,
        CRUST_COASTAL_BAND,
    };

    #[test]
    fn inflate_crust_freeboard_raises_low_freeboard_without_flipping_sign() {
        let mut heights: Vec<f32> = vec![
            -0.040, -0.004, -0.003, -0.002, -0.0015, -0.001, -0.0008, 0.0007, 0.001, 0.0014,
            0.0018, 0.0022, 0.003, 0.060,
        ];
        let preserved_large_relief = [heights[0], heights[heights.len() - 1]];
        let original_signs = heights.iter().map(|value| (*value).signum()).collect::<Vec<_>>();
        let mut before_land = heights
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .collect::<Vec<_>>();
        before_land.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let before_land_p50 = percentile_sorted(&before_land, 0.50);

        inflate_crust_freeboard(&mut heights);

        let mut after_land = heights
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .collect::<Vec<_>>();
        after_land.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let after_land_p50 = percentile_sorted(&after_land, 0.50);
        let after_signs = heights.iter().map(|value| (*value).signum()).collect::<Vec<_>>();
        assert!(after_land_p50 > before_land_p50);
        assert_eq!(original_signs, after_signs);
        assert_eq!(heights[0], preserved_large_relief[0]);
        assert_eq!(heights[heights.len() - 1], preserved_large_relief[1]);
    }

    #[test]
    fn shoreline_expansion_pushes_nonzero_cells_outside_coastal_band_without_flipping_sign() {
        let mut heights: Vec<f32> = vec![
            -0.019, -0.015, -0.010, -0.006, -0.003, 0.002, 0.004, 0.007, 0.011, 0.016, 0.019,
        ];
        let original_signs = heights.iter().map(|value| (*value).signum()).collect::<Vec<_>>();
        let before = coastal_band_ratio(&heights, CRUST_COASTAL_BAND);

        expand_shoreline_freeboard(&mut heights);

        let after = coastal_band_ratio(&heights, CRUST_COASTAL_BAND);
        let after_signs = heights.iter().map(|value| (*value).signum()).collect::<Vec<_>>();
        assert!(after < before);
        assert!(
            heights
                .iter()
                .all(|value| value.abs() <= f32::EPSILON || value.abs() > CRUST_COASTAL_BAND)
        );
        assert_eq!(original_signs, after_signs);
    }
}

pub(super) fn apply_glaciology_forcing_to_geology(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    let count = world
        .state
        .geology
        .height
        .len()
        .min(world.state.glaciology.isostatic_adjustment.len())
        .min(world.state.glaciology.applied_isostatic_adjustment.len());
    for i in 0..count {
        let target = world.state.glaciology.isostatic_adjustment[i];
        let applied = world.state.glaciology.applied_isostatic_adjustment[i];
        let delta = target - applied;
        if delta.abs() <= f32::EPSILON {
            continue;
        }
        world.state.geology.height[i] =
            (world.state.geology.height[i] + delta).clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
        world.state.glaciology.applied_isostatic_adjustment[i] = target;
    }
    sync_erosion_height(hydrology_state.as_mut(), &world.state.geology.height);
}

pub(super) fn should_run_hydrology_mfd_for_geology(
    world: &World,
    geology_state: Option<&crate::sim::world::GeologyDynamicsState>,
    hydrology_state: Option<&ErosionAutomatonState>,
) -> bool {
    match world.clock.epoch {
        EraKind::Crust | EraKind::Environment => true,
        EraKind::Life | EraKind::Civilization | EraKind::History => {
            if has_hydrology_relevant_height_change(world, hydrology_state) {
                return true;
            }
            geology_state
                .map(|state| {
                    state
                        .cached_metrics
                        .geology_activity
                        .max(state.cached_metrics.boundary_activity)
                        > HYDROLOGY_MFD_ACTIVITY_THRESHOLD
                })
                .unwrap_or(true)
        }
    }
}

fn has_hydrology_relevant_height_change(
    world: &World,
    hydrology_state: Option<&ErosionAutomatonState>,
) -> bool {
    const HEIGHT_CHANGE_EPS: f32 = 1e-6;

    let Some(state) = hydrology_state else {
        return true;
    };
    if state.height.len() != world.state.geology.height.len() {
        return true;
    }
    state
        .height
        .iter()
        .zip(world.state.geology.height.iter())
        .any(|(previous, current)| (*current - *previous).abs() > HEIGHT_CHANGE_EPS)
}

pub(super) fn run_hydrology_step_unprofiled(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    geology_state: Option<&crate::sim::world::GeologyDynamicsState>,
    budget: u32,
    run_mfd: bool,
) {
    let _ = run_hydrology_step_profiled(world, hydrology_state, geology_state, budget, run_mfd);
}

pub(super) fn run_hydrology_step_profiled(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    geology_state: Option<&crate::sim::world::GeologyDynamicsState>,
    budget: u32,
    run_mfd: bool,
) -> HydrologyStepDetailBreakdown {
    if run_mfd {
        run_hydrology_step(world, hydrology_state, budget, geology_state)
    } else {
        run_hydrology_flow_step(world, hydrology_state, budget)
    }
}

pub(super) fn apply_hydrology_erosion_to_geology(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    if world.clock.epoch == EraKind::Crust {
        let geology = &world.state.geology;
        sync_erosion_height(hydrology_state.as_mut(), &geology.height);
        return;
    }

    let glaciology_params = GlaciologyParams::default();
    let erosion_thickness_coupling = world.control.erosion_thickness_coupling;
    let deposition_thickness_coupling = world.control.deposition_thickness_coupling;
    let thickness_erosion_scale = erosion_thickness_coupling.max(0.0);
    let thickness_deposition_scale = deposition_thickness_coupling.max(0.0);
    let glacial_erosion_scale = glaciology_params.glacial_erosion_coupling.max(0.0);
    let mobile_sediment_budget = hydrology_state
        .as_ref()
        .map(|state| {
            state
                .sediment
                .iter()
                .copied()
                .map(|value| value.max(0.0))
                .sum::<f32>()
                + state
                    .sink_storage_sediment
                    .iter()
                    .copied()
                    .map(|value| value.max(0.0))
                    .sum::<f32>()
        })
        .unwrap_or(0.0);
    let count = world
        .state
        .geology
        .height
        .len()
        .min(world.state.hydrology.erosion_rate.len())
        .min(world.state.hydrology.deposition_rate.len())
        .min(world.state.glaciology.glacial_erosion_rate.len());
    let total_fluvial_erosion = world
        .state
        .hydrology
        .erosion_rate
        .iter()
        .take(count)
        .map(|value| value.max(0.0))
        .sum::<f32>();
    let total_requested_deposition = world
        .state
        .hydrology
        .deposition_rate
        .iter()
        .take(count)
        .map(|value| value.max(0.0))
        .sum::<f32>();
    let total_glacial_erosion = world
        .state
        .glaciology
        .glacial_erosion_rate
        .iter()
        .take(count)
        .map(|value| value.max(0.0) * glacial_erosion_scale)
        .sum::<f32>();
    let available_deposition_budget =
        total_fluvial_erosion + mobile_sediment_budget * thickness_deposition_scale.min(1.0);
    let deposition_scale = if total_requested_deposition <= 1e-8 || total_fluvial_erosion <= 1e-8 {
        if total_requested_deposition <= 1e-8 || available_deposition_budget <= 1e-8 {
            0.0
        } else {
            (available_deposition_budget / total_requested_deposition).clamp(0.0, 1.0)
        }
    } else {
        (available_deposition_budget / total_requested_deposition).clamp(0.0, 1.0)
    };
    let total_applied_deposition = total_requested_deposition * deposition_scale;
    let fluvial_export = (total_requested_deposition - total_applied_deposition).max(0.0);
    let glacial_export = total_glacial_erosion.max(0.0);
    let marine_increment = fluvial_export + glacial_export;
    if let Some(dynamics) = geology_state.as_mut() {
        let thickness_count = count.min(dynamics.vertex_states.len());
        for i in 0..thickness_count {
            let erosion = world.state.hydrology.erosion_rate[i].max(0.0);
            let deposition = world.state.hydrology.deposition_rate[i].max(0.0) * deposition_scale;
            let glacial_erosion =
                world.state.glaciology.glacial_erosion_rate[i].max(0.0) * glacial_erosion_scale;
            let delta = deposition - erosion - glacial_erosion;
            world.state.hydrology.deposition_rate[i] = deposition;
            world.state.geology.height[i] = (world.state.geology.height[i] + delta)
                .clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
            dynamics.vertex_states[i].thickness = (dynamics.vertex_states[i].thickness
                - erosion * thickness_erosion_scale
                + deposition * thickness_deposition_scale)
                .clamp(0.18, 1.25);
        }
        for i in thickness_count..count {
            let erosion = world.state.hydrology.erosion_rate[i].max(0.0);
            let deposition = world.state.hydrology.deposition_rate[i].max(0.0) * deposition_scale;
            let glacial_erosion =
                world.state.glaciology.glacial_erosion_rate[i].max(0.0) * glacial_erosion_scale;
            let delta = deposition - erosion - glacial_erosion;
            world.state.hydrology.deposition_rate[i] = deposition;
            world.state.geology.height[i] = (world.state.geology.height[i] + delta)
                .clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
        }
    } else {
        for i in 0..count {
            let erosion = world.state.hydrology.erosion_rate[i].max(0.0);
            let deposition = world.state.hydrology.deposition_rate[i].max(0.0) * deposition_scale;
            let glacial_erosion =
                world.state.glaciology.glacial_erosion_rate[i].max(0.0) * glacial_erosion_scale;
            let delta = deposition - erosion - glacial_erosion;
            world.state.hydrology.deposition_rate[i] = deposition;
            world.state.geology.height[i] = (world.state.geology.height[i] + delta)
                .clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
        }
    }

    world.control.global_sediment_export += marine_increment;
    world.control.marine_sediment_mass += marine_increment;
    world.control.solid_earth_mass_proxy = world.state.geology.height.iter().copied().sum();

    let geology = &world.state.geology;
    sync_erosion_height(hydrology_state.as_mut(), &geology.height);
}
