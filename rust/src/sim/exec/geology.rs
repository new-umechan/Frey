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
    let default_params = crate::GeologyParams::default();
    let (erosion_thickness_coupling, deposition_thickness_coupling) = world
        .runtime
        .hydrology_dynamics
        .as_ref()
        .map(|state| {
            (
                state.params.erosion_thickness_coupling,
                state.params.deposition_thickness_coupling,
            )
        })
        .unwrap_or((
            default_params.erosion_thickness_coupling,
            default_params.deposition_thickness_coupling,
        ));
    let mut deltas = Vec::new();
    {
        let geology = &mut world.state.geology;
        let count = geology
            .height
            .len()
            .min(geology.erosion_rate.len())
            .min(geology.deposition_rate.len());
        deltas.reserve(count);
        for i in 0..count {
            let erosion = geology.erosion_rate[i].max(0.0);
            let deposition = geology.deposition_rate[i].max(0.0);
            let delta = deposition - erosion;
            geology.height[i] =
                (geology.height[i] + delta).clamp(GEOLOGY_HEIGHT_MIN, GEOLOGY_HEIGHT_MAX);
            deltas.push((erosion, deposition));
        }
    }
    if let Some(dynamics) = world.runtime.geology_dynamics.as_mut() {
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
    if let Some(state) = world.runtime.hydrology_dynamics.as_mut() {
        if state.height.len() == geology.height.len() {
            state.height.clone_from(&geology.height);
        }
    }
}
