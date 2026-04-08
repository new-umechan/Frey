use crate::sim::exec::HYDROLOGY_MFD_ACTIVITY_THRESHOLD;
use crate::sim::glaciology::types::GlaciologyParams;
use crate::sim::hydrology::{
    run_hydrology_flow_step, run_hydrology_step, sync_erosion_height,
    HydrologyStepDetailBreakdown,
};
use crate::sim::world::{EraKind, World};

const GEOLOGY_HEIGHT_MIN: f32 = -1.2;
const GEOLOGY_HEIGHT_MAX: f32 = 1.2;

pub(super) fn run_geology_step_with_state(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    budget: u32,
) {
    crate::sim::geology::update_geology(world, geology_state, budget);
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
) -> bool {
    match world.clock.epoch {
        EraKind::Crust | EraKind::Environment => true,
        EraKind::Life | EraKind::Civilization | EraKind::History => geology_state
            .map(|state| {
                state
                    .cached_metrics
                    .geology_activity
                    .max(state.cached_metrics.boundary_activity)
                    > HYDROLOGY_MFD_ACTIVITY_THRESHOLD
            })
            .unwrap_or(true),
    }
}

pub(super) fn run_hydrology_step_unprofiled(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    budget: u32,
    run_mfd: bool,
) {
    let _ = run_hydrology_step_profiled(world, hydrology_state, budget, run_mfd);
}

pub(super) fn run_hydrology_step_profiled(
    world: &mut World,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
    budget: u32,
    run_mfd: bool,
) -> HydrologyStepDetailBreakdown {
    if run_mfd {
        run_hydrology_step(world, hydrology_state, budget)
    } else {
        run_hydrology_flow_step(world, hydrology_state, budget)
    }
}

pub(super) fn apply_hydrology_erosion_to_geology(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
    hydrology_state: &mut crate::sim::exec::HydrologyExecState,
) {
    let glaciology_params = GlaciologyParams::default();
    let erosion_thickness_coupling = world.control.erosion_thickness_coupling;
    let deposition_thickness_coupling = world.control.deposition_thickness_coupling;
    let mut deltas = Vec::new();
    {
        let geology = &mut world.state.geology;
        let count = geology
            .height
            .len()
            .min(geology.erosion_rate.len())
            .min(geology.deposition_rate.len())
            .min(world.state.glaciology.glacial_erosion_rate.len());
        deltas.reserve(count);
        for i in 0..count {
            let erosion = geology.erosion_rate[i].max(0.0);
            let deposition = geology.deposition_rate[i].max(0.0);
            let glacial_erosion = world.state.glaciology.glacial_erosion_rate[i].max(0.0)
                * glaciology_params.glacial_erosion_coupling.max(0.0);
            let delta = deposition - erosion - glacial_erosion;
            geology.height[i] =
                (geology.height[i] + delta).clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
            deltas.push((erosion, deposition));
        }
    }
    if let Some(dynamics) = geology_state.as_mut() {
        for (i, (erosion, deposition)) in deltas.into_iter().enumerate() {
            if i >= dynamics.vertex_states.len() {
                break;
            }
            dynamics.vertex_states[i].thickness = (dynamics.vertex_states[i].thickness
                - erosion * erosion_thickness_coupling.max(0.0)
                + deposition * deposition_thickness_coupling.max(0.0))
            .clamp(0.18, 1.25);
        }
    }

    let geology = &world.state.geology;
    sync_erosion_height(hydrology_state.as_mut(), &geology.height);
}
