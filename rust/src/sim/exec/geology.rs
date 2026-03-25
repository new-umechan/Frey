use crate::sim::exec::HYDROLOGY_MFD_ACTIVITY_THRESHOLD;
use crate::sim::hydrology::{
    run_hydrology_flow_step, run_hydrology_step, HydrologyStepDetailBreakdown,
};
use crate::sim::world::{EraKind, World};

const GEOLOGY_HEIGHT_MIN: f32 = -1.2;
const GEOLOGY_HEIGHT_MAX: f32 = 1.2;

pub(super) fn run_geology_step(world: &mut World, budget: u32) {
    crate::sim::geology::update_geology(world, budget);
}

pub(super) fn should_run_hydrology_mfd(world: &World) -> bool {
    match world.clock.epoch {
        EraKind::Crust | EraKind::Environment => true,
        EraKind::Life | EraKind::Civilization | EraKind::History => world
            .runtime
            .geology_dynamics
            .as_ref()
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

pub(super) fn run_hydrology_step_unprofiled(world: &mut World, budget: u32, run_mfd: bool) {
    let _ = run_hydrology_step_profiled(world, budget, run_mfd);
}

pub(super) fn run_hydrology_step_profiled(
    world: &mut World,
    budget: u32,
    run_mfd: bool,
) -> HydrologyStepDetailBreakdown {
    if run_mfd {
        run_hydrology_step(world, budget)
    } else {
        run_hydrology_flow_step(world, budget)
    }
}

pub(super) fn apply_hydrology_erosion_to_geology(world: &mut World) {
    let geology = &mut world.state.geology;
    let count = geology
        .height
        .len()
        .min(geology.erosion_rate.len())
        .min(geology.deposition_rate.len());
    for i in 0..count {
        let erosion = geology.erosion_rate[i].max(0.0);
        let deposition = geology.deposition_rate[i].max(0.0);
        let delta = deposition - erosion;
        geology.height[i] =
            (geology.height[i] + delta).clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
    }

    if let Some(state) = world.runtime.hydrology_dynamics.as_mut() {
        if state.height.len() == geology.height.len() {
            state.height.clone_from(&geology.height);
        }
    }
}
