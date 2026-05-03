use crate::sim::exec::HYDROLOGY_MFD_ACTIVITY_THRESHOLD;
use crate::sim::glaciology::types::GlaciologyParams;
use crate::sim::hydrology::{
    run_hydrology_flow_step, run_hydrology_step, sync_erosion_height, HydrologyStepDetailBreakdown,
};
use crate::sim::state::erosion::ErosionAutomatonState;
use crate::sim::world::{EraKind, World};

const GEOLOGY_HEIGHT_MIN: f32 = -1.2;
const GEOLOGY_HEIGHT_MAX: f32 = 1.2;

pub(super) fn run_geology_step_with_state(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    budget: u32,
) {
    crate::sim::geology::update_geology(world, geology_state, budget);
    preserve_crust_freeboard(world);
}

fn preserve_crust_freeboard(world: &mut World) {
    if world.clock.epoch != EraKind::Crust {
        return;
    }

    let target_land_ratio = world.clock.transition.last_land_ratio.clamp(0.05, 0.95);
    let height = &mut world.state.geology.height;
    if height.is_empty() {
        return;
    }

    let mut sorted = height
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if sorted.is_empty() {
        return;
    }
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let target_sea_ratio = 1.0 - target_land_ratio;
    let sea_idx = ((sorted.len() as f32) * target_sea_ratio).floor() as usize;
    let sea_level = sorted[sea_idx.min(sorted.len().saturating_sub(1))];
    if !sea_level.is_finite() || sea_level.abs() <= 1e-6 {
        return;
    }

    for value in height.iter_mut() {
        *value = (*value - sea_level).clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
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
