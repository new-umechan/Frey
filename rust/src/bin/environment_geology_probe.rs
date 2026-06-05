use frey_wasm::sim;
use frey_wasm::sim::world::FeedbackQueue;
use frey_wasm::sim::ExecWorldPhase;
use frey_wasm::GeologyParams;

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..GeologyParams::default()
    };
    let (mut world, hydrology_state) =
        sim::headless::init_world_for_headless_runner("alpha", 6, geology_params.clone())
            .unwrap_or_else(|err| panic!("failed to init world: {err}"));
    let mut hydrology_state = Some(hydrology_state);
    let mut feedback = FeedbackQueue::new(world.cell_count());
    while world.clock.tick < 800 {
        sim::exec_world_with_feedback_and_hydrology(
            &mut world,
            &mut feedback,
            &mut hydrology_state,
        );
        sim::hydrology::sync_hydrology_state_for_headless_runner(
            &mut world,
            hydrology_state
                .as_mut()
                .expect("hydrology state should exist during environment probe"),
            &geology_params,
        );
    }
    let hydrology_state =
        hydrology_state.expect("hydrology state should exist after environment probe");
    let mut feedback = FeedbackQueue::new(world.cell_count());

    let before = world.state.geology.height.clone();
    let _ = sim::exec_world_slice_with_hydrology(
        &mut world,
        &mut feedback,
        &mut Some(hydrology_state.clone()),
        ExecWorldPhase::Geology,
        1,
    );
    let after = &world.state.geology.height;
    let metrics = world
        .exec_scratch
        .geology_dynamics
        .as_ref()
        .map(|state| state.cached_metrics)
        .unwrap_or_default();

    let count = before.len().min(after.len()).max(1) as f32;
    let mean_abs_delta = before
        .iter()
        .zip(after.iter())
        .map(|(a, b)| (b - a).abs())
        .sum::<f32>()
        / count;
    let mean_signed_delta = before
        .iter()
        .zip(after.iter())
        .map(|(a, b)| b - a)
        .sum::<f32>()
        / count;

    println!(
        "{{\"tick\":{},\"era\":\"{}\",\"mean_abs_delta\":{},\"mean_signed_delta\":{},\"land_ratio\":{},\"activity_scale\":{},\"mean_abs_surface_write_delta\":{},\"mean_abs_surface_range_clamp_delta\":{},\"mean_compressive\":{},\"mean_tensile\":{},\"mean_abs_diffusive_raw\":{},\"mean_abs_isostatic_raw\":{},\"mean_abs_thermal_subsidence\":{}}}",
        world.clock.tick,
        world.clock.epoch.as_key(),
        mean_abs_delta,
        mean_signed_delta,
        world.metrics().land_ratio,
        metrics.activity_scale,
        metrics.mean_abs_surface_write_delta,
        metrics.mean_abs_surface_range_clamp_delta,
        metrics.mean_compressive,
        metrics.mean_tensile,
        metrics.mean_abs_diffusive_raw,
        metrics.mean_abs_isostatic_raw,
        metrics.mean_abs_thermal_subsidence,
    );
}
